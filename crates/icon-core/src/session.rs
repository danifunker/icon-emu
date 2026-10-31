//! Headless, scriptable ICON networks for automated testing (e.g. RustyBackup
//! verifying QNX 2 filesystem writes), and the engine behind `icon-cli serve`.
//!
//! A [`Rig`] is one ICONET segment: node slot 0 is the LEXICON fileserver,
//! slots 1.. are ICON workstations (nodes 16, 17, ...). Everything waits on
//! what is on each console rather than on fixed timings: a boot completes
//! when `Login:` appears, a command completes when the `$` prompt returns.
//! Each console's output is collected as it scrolls, so it can be longer
//! than one screen.
//!
//! ```no_run
//! use icon_core::session::Session;
//! let rom = std::fs::read("icon1_rom_combined.bin").unwrap();
//! let hd = std::fs::read("lexicon-4.10.01-10mb.img").unwrap();
//! let mut s = Session::fileserver(&rom, Some(hd), None).unwrap();
//! s.login("root", "superuser").unwrap();
//! println!("{}", s.run("chkfsys 1").unwrap());
//! ```

use crate::devices::nvram::presets;
use crate::devices::wd1002::Disk;
use crate::lan::Lan;
use crate::machine::{Config, Machine, CPU_HZ, STRAPS_NORMAL};

/// Screen sampling interval (0.5 ms): fine enough to see a console between
/// the steps of a fast scroll.
const SLICE: u64 = (CPU_HZ / 2000) as u64;
const FRAME: u64 = (CPU_HZ / 60) as u64;
/// A screen is used once the frame buffer has held still this many samples.
const STABLE_SAMPLES: u32 = 3;

pub const FILESERVER: usize = 0;

#[derive(Debug)]
pub enum SessionError {
    /// Waited this many emulated seconds without seeing the expected text.
    Timeout { node: usize, waiting_for: String, seconds: f64, screen: String },
    BadImage(String),
    NoSuchNode(usize),
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionError::Timeout { node, waiting_for, seconds, screen } => {
                write!(f, "slot {node}: timed out after {seconds:.0} s waiting for {waiting_for:?}; screen:\n{screen}")
            }
            SessionError::BadImage(m) => write!(f, "bad image: {m}"),
            SessionError::NoSuchNode(n) => write!(f, "no powered-on machine in slot {n}"),
        }
    }
}
impl std::error::Error for SessionError {}

/// Per-console capture state.
#[derive(Default)]
struct Console {
    transcript: Vec<String>,
    last_screen: Vec<String>,
    last_hash: u64,
    still: u32,
}

pub struct Rig {
    pub lan: Lan,
    rom: Vec<u8>,
    /// A different boot ROM for the workstations (default: the same one).
    pub workstation_rom: Option<Vec<u8>>,
    consoles: Vec<Console>,
    /// Memory of every machine powered on: 512, or 1024 for the 1 MB upgrade
    /// card (needs the Rel 2.11 ROM).
    pub ram_kb: u32,
    /// Emulated seconds allowed for a boot / a single command.
    pub boot_timeout: f64,
    pub command_timeout: f64,
    /// The host clock as the ICON should read it: **local** wall-clock time,
    /// as Unix-style seconds (the ICON had no time zone). The fileserver's
    /// clock starts from it at power-on, folded into 1994 (`rtc::icon_time`).
    /// The default is UTC, since the core has no time-zone data; `icon-cli`
    /// sets local time, and the web page passes the browser's.
    pub clock: fn() -> i64,
    /// Each slot's own NVRAM, if it has one: used as it is at power-on
    /// (instead of the built-in image for the slot and `ram_kb`), and kept
    /// at power-off with whatever the ICON wrote to it, as the real EEPROM.
    pub nvram: Vec<Option<[u16; 16]>>,
    /// Finished print jobs from every machine's parallel port, oldest first.
    pub printouts: Vec<Printout>,
    /// Which slots have a printer on their parallel port (from their next
    /// power-on; default: the fileserver's only, as at most schools).
    pub printers: Vec<bool>,
}

/// One print job: the bytes a machine sent its printer, ended by the
/// printer going idle for [`PRINT_IDLE`] seconds.
#[derive(Clone, Debug)]
pub struct Printout {
    pub slot: usize,
    pub data: Vec<u8>,
    /// Emulated seconds when it finished.
    pub seconds: f64,
}

/// Seconds without a byte that end a print job.
pub const PRINT_IDLE: f64 = 3.0;

/// UTC seconds: the default `Rig::clock` (the core has no time-zone data).
pub fn utc_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

impl Rig {
    /// An empty segment with room for a fileserver and `workstations` ICONs.
    pub fn new(rom: &[u8], workstations: usize) -> Result<Self, SessionError> {
        if rom.len() != 0x8000 {
            return Err(SessionError::BadImage("ROM must be the 32 KB interleaved image".into()));
        }
        let slots = 1 + workstations;
        Ok(Rig {
            lan: Lan::new(slots),
            rom: rom.to_vec(),
            workstation_rom: None,
            consoles: (0..slots).map(|_| Console::default()).collect(),
            ram_kb: 512,
            clock: utc_now,
            boot_timeout: 180.0,
            command_timeout: 300.0,
            nvram: vec![None; slots],
            printouts: Vec::new(),
            printers: (0..slots).map(|s| s == FILESERVER).collect(),
        })
    }

    pub fn slots(&self) -> usize {
        self.lan.nodes.len()
    }

    /// Add another workstation slot; returns its slot number.
    pub fn add_workstation(&mut self) -> usize {
        self.lan.nodes.push(None);
        self.lan.grow();
        self.consoles.push(Console::default());
        self.nvram.push(None);
        self.printers.push(false);
        self.lan.nodes.len() - 1
    }

    /// Power on the fileserver (slot 0) with optional disks. With no floppy it
    /// boots from the hard disk (the floppy is then QNX drive 2); with one it
    /// boots from the floppy (drive 1; hard-disk partitions are drives 2, 3).
    pub fn power_on_fileserver(&mut self, hard_disk: Option<Vec<u8>>, floppy: Option<Vec<u8>>) -> Result<(), SessionError> {
        let nvram = self.boot_nvram(FILESERVER);
        let mut m = Machine::new(Config {
            rom: self.rom.clone(),
            patch_800k_floppy: false,
            ram_kb: self.ram_kb,
            nvram,
            straps: STRAPS_NORMAL,
            host_time: (self.clock)(),
            disk_controller: true,
        });
        if let Some(hd) = hard_disk {
            m.board.attach_hard_disk(Some(Disk::hard_auto(hd)));
        }
        if let Some(fd) = floppy {
            m.board.insert_floppy(Some(Disk::floppy_from_image(fd).ok_or_else(|| SessionError::BadImage("unknown floppy image size".into()))?));
        }
        m.board.parallel.online = self.printers[FILESERVER];
        self.lan.power_on(FILESERVER, m);
        self.consoles[FILESERVER] = Console::default();
        Ok(())
    }

    /// Power on a workstation in `slot` (>= 1). It network-boots from the
    /// fileserver as node 15 + slot.
    pub fn power_on_workstation(&mut self, slot: usize) -> Result<(), SessionError> {
        if slot == FILESERVER || slot >= self.slots() {
            return Err(SessionError::NoSuchNode(slot));
        }
        let nv = self.boot_nvram(slot);
        let mut m = Machine::new(Config {
            rom: self.workstation_rom.clone().unwrap_or_else(|| self.rom.clone()),
            patch_800k_floppy: false,
            ram_kb: self.ram_kb,
            nvram: nv,
            straps: STRAPS_NORMAL,
            host_time: (self.clock)(),
            disk_controller: false,
        });
        m.board.parallel.online = self.printers[slot];
        self.lan.power_on(slot, m);
        self.consoles[slot] = Console::default();
        Ok(())
    }

    pub fn power_off(&mut self, slot: usize) -> Option<Machine> {
        if let Some(c) = self.consoles.get_mut(slot) {
            *c = Console::default();
        }
        let m = self.lan.power_off(slot);
        if let (Some(m), Some(Some(nv))) = (&m, self.nvram.get_mut(slot)) {
            *nv = m.board.nvram.words;
        }
        m
    }

    /// The NVRAM `slot` powers on with: its own, else the built-in image.
    pub fn boot_nvram(&self, slot: usize) -> [u16; 16] {
        self.nvram.get(slot).copied().flatten().unwrap_or_else(|| presets::for_slot(slot, self.ram_kb))
    }

    /// What `slot`'s NVRAM holds now: the running machine's chip, else what
    /// it will power on with.
    pub fn nvram_words(&self, slot: usize) -> [u16; 16] {
        self.machine(slot).map(|m| m.board.nvram.words).unwrap_or_else(|_| self.boot_nvram(slot))
    }

    /// Give `slot` its own NVRAM (`None`: back to the built-in image). A
    /// running machine's chip is reprogrammed in place; the ROM reads it at
    /// the next power-on.
    pub fn set_nvram(&mut self, slot: usize, words: Option<[u16; 16]>) -> Result<(), SessionError> {
        if slot >= self.slots() {
            return Err(SessionError::NoSuchNode(slot));
        }
        self.nvram[slot] = words;
        let nv = self.boot_nvram(slot);
        if let Ok(m) = self.machine_mut(slot) {
            m.board.nvram.words = nv;
        }
        Ok(())
    }

    pub fn machine(&self, slot: usize) -> Result<&Machine, SessionError> {
        self.lan.nodes.get(slot).and_then(|m| m.as_ref()).ok_or(SessionError::NoSuchNode(slot))
    }
    pub fn machine_mut(&mut self, slot: usize) -> Result<&mut Machine, SessionError> {
        self.lan.nodes.get_mut(slot).and_then(|m| m.as_mut()).ok_or(SessionError::NoSuchNode(slot))
    }

    /// Current console text of a node.
    pub fn screen(&self, slot: usize) -> Result<String, SessionError> {
        Ok(self.machine(slot)?.board.screen_text())
    }

    /// Everything shown on a node's console since it powered on.
    pub fn transcript(&self, slot: usize) -> String {
        self.consoles.get(slot).map(|c| c.transcript.join("\n")).unwrap_or_default()
    }

    /// Type text on a node's keyboard (`\r` = Enter, `\x1b` = Esc).
    pub fn type_text(&mut self, slot: usize, text: &str) -> Result<(), SessionError> {
        let m = self.machine_mut(slot)?;
        for k in crate::devices::keyboard::type_text(text) {
            m.board.key(k);
        }
        Ok(())
    }

    /// Send one raw key code (key number; +0x80 = release).
    pub fn key(&mut self, slot: usize, code: u8) -> Result<(), SessionError> {
        self.machine_mut(slot)?.board.key(code);
        Ok(())
    }

    /// Move a node's trackball (counts; +x right, +y down).
    pub fn trackball(&mut self, slot: usize, dx: i32, dy: i32) -> Result<(), SessionError> {
        let m = self.machine_mut(slot)?;
        m.board.counters[1].motion(dx);
        m.board.counters[0].motion(dy);
        Ok(())
    }

    /// Wait for `Login:` on a node.
    pub fn wait_login(&mut self, slot: usize) -> Result<(), SessionError> {
        let t = self.boot_timeout;
        self.wait_for(slot, |s| s.lines().any(|l| l.trim_start().starts_with("Login:")), "Login:", t)
    }

    pub fn login(&mut self, slot: usize, user: &str, password: &str) -> Result<(), SessionError> {
        self.type_text(slot, &format!("{user}\r"))?;
        self.wait_for(slot, |s| s.lines().any(|l| l.contains("Password:")), "Password:", 30.0)?;
        self.type_text(slot, &format!("{password}\r"))?;
        self.wait_for(slot, |s| s.lines().rfind(|l| !l.trim().is_empty()).is_some_and(is_prompt), "$ prompt", 60.0)
    }

    /// Run a shell command on a node and return what it printed (the lines
    /// between the echoed command and the next `$` prompt).
    pub fn run(&mut self, slot: usize, command: &str) -> Result<String, SessionError> {
        self.machine(slot)?;
        let start = self.consoles[slot].transcript.len().saturating_sub(1);
        self.type_text(slot, &format!("{command}\r"))?;
        let deadline = self.now() + (self.command_timeout * CPU_HZ as f64) as u64;
        loop {
            self.step();
            if let Some(out) = output_after(&self.consoles[slot].transcript[start.min(self.consoles[slot].transcript.len())..], command) {
                if self.settled(slot) {
                    let t = &self.consoles[slot].transcript;
                    if let Some(out2) = output_after(&t[start.min(t.len())..], command) {
                        return Ok(out2);
                    }
                    return Ok(out);
                }
            }
            if self.now() > deadline {
                return Err(SessionError::Timeout { node: slot, waiting_for: format!("prompt after {command:?}"), seconds: self.command_timeout, screen: self.screen(slot).unwrap_or_default() });
            }
        }
    }

    /// Insert (or with `None` eject) the fileserver's diskette.
    pub fn insert_floppy(&mut self, image: Option<Vec<u8>>) -> Result<(), SessionError> {
        let d = match image {
            Some(i) => Some(Disk::floppy_from_image(i).ok_or_else(|| SessionError::BadImage("unknown floppy image size".into()))?),
            None => None,
        };
        self.machine_mut(FILESERVER)?.board.insert_floppy(d);
        Ok(())
    }

    /// The fileserver's hard-disk image, including anything QNX wrote.
    pub fn hard_disk(&self) -> Option<Vec<u8>> {
        self.machine(FILESERVER).ok()?.board.wd.drives[0].as_ref().map(|d| d.data.clone())
    }

    /// The fileserver's diskette image, including anything QNX wrote.
    pub fn floppy(&self) -> Option<Vec<u8>> {
        self.machine(FILESERVER).ok()?.board.wd.drives[3].as_ref().map(|d| d.data.clone())
    }

    /// Let emulated time pass.
    pub fn idle(&mut self, seconds: f64) {
        let end = self.now() + (seconds * CPU_HZ as f64) as u64;
        while self.now() < end {
            self.step();
        }
    }

    /// Emulated time in seconds since the first machine powered on.
    pub fn seconds(&self) -> f64 {
        self.now() as f64 / CPU_HZ as f64
    }

    fn now(&self) -> u64 {
        self.lan.nodes.iter().flatten().map(|m| m.board.cycles).max().unwrap_or(0)
    }

    pub fn wait_for(&mut self, slot: usize, done: impl Fn(&str) -> bool, what: &str, timeout: f64) -> Result<(), SessionError> {
        let deadline = self.now() + (timeout * CPU_HZ as f64) as u64;
        loop {
            if done(&self.screen(slot)?) {
                return Ok(());
            }
            if self.now() > deadline {
                return Err(SessionError::Timeout { node: slot, waiting_for: what.into(), seconds: timeout, screen: self.screen(slot)? });
            }
            self.step();
        }
    }

    /// True when a node's screen stops changing for a few frames and it has
    /// no keystrokes queued.
    fn settled(&mut self, slot: usize) -> bool {
        if self.machine(slot).map(|m| !m.board.kbd_queue.is_empty()).unwrap_or(true) {
            return false;
        }
        // The workstation's cursor blinks, so it is left out of the comparison.
        let steady = |s: String| s.lines().map(|l| l.trim_end_matches(['?', '_', ' ']).to_string()).collect::<Vec<_>>();
        let before = steady(self.screen(slot).unwrap_or_default());
        for _ in 0..6 {
            self.step();
        }
        before == steady(self.screen(slot).unwrap_or_default())
    }

    /// Run one frame of every machine, sampling each console every 0.5 ms.
    pub fn step(&mut self) {
        for _ in 0..(FRAME / SLICE) {
            self.lan.run(SLICE);
            for slot in 0..self.lan.nodes.len() {
                let Some(m) = self.lan.nodes[slot].as_ref() else { continue };
                let h = fb_hash(m);
                let c = &mut self.consoles[slot];
                if h != c.last_hash {
                    c.last_hash = h;
                    c.still = 0;
                    continue;
                }
                c.still += 1;
                if c.still == STABLE_SAMPLES {
                    let lines: Vec<String> = m.board.screen_text().lines().map(str::to_string).collect();
                    merge_scroll(&mut c.transcript, &c.last_screen, &lines);
                    c.last_screen = lines;
                }
            }
        }
        self.collect_printouts();
    }

    /// Move finished print jobs from the machines into `printouts`.
    fn collect_printouts(&mut self) {
        let idle = (PRINT_IDLE * CPU_HZ as f64) as u64;
        let seconds = self.seconds();
        for (slot, m) in self.lan.nodes.iter_mut().enumerate() {
            let Some(m) = m.as_mut() else { continue };
            let now = m.board.cycles;
            if let Some(data) = m.board.parallel.take_job(now, idle) {
                self.printouts.push(Printout { slot, data, seconds });
            }
        }
    }
}

fn fb_hash(m: &Machine) -> u64 {
    let base = m.board.frame_base() as usize;
    let fb = m.board.ram.get(base..base + 80 * 240).unwrap_or(&[]);
    fb.iter().fold(0xcbf29ce484222325u64, |h, &b| (h ^ b as u64).wrapping_mul(0x100000001b3))
}

/// The lines after the echoed `command` up to (not including) a final `$`
/// prompt line, if that prompt has appeared.
fn output_after(lines: &[String], command: &str) -> Option<String> {
    // The echoed command line may still carry the cursor.
    let bare = |l: &str| l.trim_end_matches(['?', '_', ' ']).to_string();
    let pos = lines.iter().rposition(|l| bare(l).ends_with(&bare(command.trim())))?;
    let after = &lines[pos + 1..];
    let last = after.iter().rposition(|l| !l.trim().is_empty())?;
    if !is_prompt(&after[last]) {
        return None;
    }
    Some(after[..last].join("\n"))
}

/// A single fileserver, the simple case of a [`Rig`].
pub struct Session {
    pub rig: Rig,
}

impl Session {
    /// Power on a LEXICON fileserver and wait for `Login:`.
    pub fn fileserver(rom: &[u8], hard_disk: Option<Vec<u8>>, floppy: Option<Vec<u8>>) -> Result<Self, SessionError> {
        Self::fileserver_with_clock(rom, hard_disk, floppy, utc_now)
    }
    /// The same, with the host clock to start the ICON's clock from (see `Rig::clock`).
    pub fn fileserver_with_clock(rom: &[u8], hard_disk: Option<Vec<u8>>, floppy: Option<Vec<u8>>, clock: fn() -> i64) -> Result<Self, SessionError> {
        let mut rig = Rig::new(rom, 0)?;
        rig.clock = clock;
        rig.power_on_fileserver(hard_disk, floppy)?;
        rig.wait_login(FILESERVER)?;
        Ok(Session { rig })
    }
    pub fn login(&mut self, user: &str, password: &str) -> Result<(), SessionError> {
        self.rig.login(FILESERVER, user, password)
    }
    pub fn run(&mut self, command: &str) -> Result<String, SessionError> {
        self.rig.run(FILESERVER, command)
    }
    pub fn screen(&self) -> String {
        self.rig.screen(FILESERVER).unwrap_or_default()
    }
    pub fn transcript(&self) -> String {
        self.rig.transcript(FILESERVER)
    }
    pub fn insert_floppy(&mut self, image: Option<Vec<u8>>) -> Result<(), SessionError> {
        self.rig.insert_floppy(image)
    }
    pub fn hard_disk(&self) -> Option<Vec<u8>> {
        self.rig.hard_disk()
    }
    pub fn floppy(&self) -> Option<Vec<u8>> {
        self.rig.floppy()
    }
    pub fn idle(&mut self, seconds: f64) {
        self.rig.idle(seconds)
    }
}

fn is_prompt(line: &str) -> bool {
    // Trailing cursor: an inverse cell ('?') or an underline.
    let l = line.trim_end_matches(['?', '_', ' ']);
    l == "$" || l == "#" || l == "%"
}

/// The same console row seen twice: combine them character by character,
/// taking whichever copy decoded each cell ('?' = undecodable). `None` if the
/// rows differ in a decoded character (it is a different line).
fn combine(a: &str, b: &str) -> Option<String> {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let n = a.len().max(b.len());
    let mut out = String::with_capacity(n);
    for i in 0..n {
        match (a.get(i), b.get(i)) {
            (Some(x), Some(y)) if x == y => out.push(*x),
            (Some('?'), Some(y)) => out.push(*y),
            (Some(x), Some('?')) => out.push(*x),
            (Some(x), None) => out.push(*x),
            (None, Some(y)) => out.push(*y),
            _ => return None,
        }
    }
    Some(out)
}

/// Normalise a console row for comparison: drop the cursor cell and padding.
fn norm(l: &str) -> String {
    l.trim_end_matches(['?', ' ']).to_string()
}

/// Append to `transcript` whatever is new in `now` compared with `prev`,
/// allowing for the screen having scrolled up by any number of lines. The
/// tail of `transcript` always mirrors `prev`, so lines still being drawn are
/// rewritten in place rather than appended twice.
fn merge_scroll(transcript: &mut Vec<String>, prev: &[String], now: &[String]) {
    // Row 0 never scrolls (it keeps leftover boot-logo pixels), so leave it out.
    let p: Vec<String> = prev.iter().skip(1).map(|l| norm(l)).collect();
    let n: Vec<String> = now.iter().skip(1).map(|l| norm(l)).collect();
    if p.is_empty() {
        transcript.extend(n);
        return;
    }
    // Find the scroll shift s where p[s..] best lines up with the top of n.
    let mut best: Option<(usize, usize)> = None; // (shift, matching lines)
    for shift in 0..p.len() {
        let overlap = p.len() - shift;
        let same = (0..overlap.min(n.len())).filter(|&i| p[shift + i] == n[i]).count();
        if overlap >= 3 && same * 10 >= overlap * 8 && best.is_none_or(|(_, b)| same > b) {
            best = Some((shift, same));
        }
        if same == overlap {
            break; // exact match at the smallest shift
        }
    }
    match best {
        Some((shift, _)) => {
            // Lines p[..shift] scrolled off and stay in the transcript; the
            // rest of the tail is replaced by the current screen, except that
            // a line that only lost characters to '?' (leftover logo pixels
            // near the top of the screen) keeps its cleaner earlier copy.
            let overlap = p.len() - shift;
            let keep = transcript.len().saturating_sub(overlap);
            let old: Vec<String> = transcript.split_off(keep);
            for (i, line) in n.into_iter().enumerate() {
                match old.get(i).and_then(|o| combine(o, &line)) {
                    Some(merged) => transcript.push(merged),
                    None => transcript.push(line),
                }
            }
        }
        None => {
            // The screen was redrawn: keep what was there, then add the new screen.
            transcript.extend(n);
        }
    }
}
