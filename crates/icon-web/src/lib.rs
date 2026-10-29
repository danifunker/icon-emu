//! WebAssembly front end: an ICONET segment with a LEXICON fileserver
//! (slot 0) and one or more ICON workstations (slots 1.., node 15 + slot).
//!
//! Each slot has a machine kind, and each kind its own boot ROM, so a
//! fileserver ROM and a workstation ROM can differ (later: ICON2, ICON3).

use icon_core::devices::nvram::presets;
use icon_core::devices::wd1002::Disk;
use icon_core::lan::Lan;
use icon_core::machine::{Config, Machine, CPU_HZ, STRAPS_NORMAL};
use wasm_bindgen::prelude::*;

pub const FILESERVER: usize = 0;

/// A blank 10 MB (306 x 4 x 17) hard disk image.
#[wasm_bindgen]
pub fn blank_hard_disk_10mb() -> Vec<u8> {
    let (c, h, s) = icon_core::devices::wd1002::HD_10MB;
    Disk::blank_hard(c, h, s).data
}

/// A blank 64 MB-class (1024 x 8 x 17, Micropolis 1335) hard disk image.
#[wasm_bindgen]
pub fn blank_hard_disk_64mb() -> Vec<u8> {
    let (c, h, s) = icon_core::devices::wd1002::HD_64MB;
    Disk::blank_hard(c, h, s).data
}

/// A pre-formatted 640K QNX floppy (empty, as `dinit 2` leaves it).
#[wasm_bindgen]
pub fn blank_floppy_640k() -> Vec<u8> {
    Disk::formatted_floppy_640k()
}

// ---- Files on a disk image, for the page's Files panel. Each call takes the
// image and returns the changed copy; the page decides when that is safe (a
// diskette is swapped out and back in, a hard disk only while the fileserver
// is off). `offset` is a volume's byte offset from `disk_volumes`; `now` is
// local wall-clock seconds, dated into 1991 as the emulated clock does.

use icon_core::files::{self, qnx2fs};

fn js_err(e: qnx2fs::Error) -> JsError {
    JsError::new(&e.to_string())
}

fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The QNX volumes in a disk image, as JSON `[{"label", "offset"}]`.
#[wasm_bindgen]
pub fn disk_volumes(image: &[u8]) -> String {
    let v: Vec<String> = files::volumes(image)
        .iter()
        .map(|v| format!("{{\"label\":{},\"offset\":{}}}", json_str(&v.label), v.offset))
        .collect();
    format!("[{}]", v.join(","))
}

/// A directory's entries as JSON: name, dir, size, date, attrs; plus the volume's free space.
#[wasm_bindgen]
pub fn disk_list(image: &[u8], offset: f64, path: &str) -> Result<String, JsError> {
    let mut img = image.to_vec();
    let mut vol = files::open(&mut img, offset as u64).map_err(js_err)?;
    let mut list = vol.list(path).map_err(js_err)?;
    list.sort_by(|a, b| (!a.is_dir, a.name.to_lowercase()).cmp(&(!b.is_dir, b.name.to_lowercase())));
    let entries: Vec<String> = list
        .iter()
        .map(|e| {
            format!(
                "{{\"name\":{},\"dir\":{},\"size\":{},\"date\":{},\"attrs\":{}}}",
                json_str(&e.name),
                e.is_dir,
                e.size,
                json_str(&e.modified().map(qnx2fs::format_date).unwrap_or_default()),
                json_str(&e.describe())
            )
        })
        .collect();
    Ok(format!(
        "{{\"free\":{},\"total\":{},\"entries\":[{}]}}",
        vol.free_blocks() * 512,
        vol.total_blocks() * 512,
        entries.join(",")
    ))
}

/// A file's contents; with `text`, QNX text gets LF line endings.
#[wasm_bindgen]
pub fn disk_read(image: &[u8], offset: f64, path: &str, text: bool) -> Result<Vec<u8>, JsError> {
    let mut img = image.to_vec();
    let data = files::open(&mut img, offset as u64).and_then(|mut v| v.get(path)).map_err(js_err)?;
    Ok(if text && qnx2fs::looks_like_rs_text(&data) { qnx2fs::rs_to_lf(&data) } else { data })
}

/// Write a file (its name must already be valid; see `qnx_name`). With `text`,
/// host text gets QNX's line endings. With `replace`, an existing file of that
/// name is deleted first.
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn disk_put(image: Vec<u8>, offset: f64, path: &str, data: &[u8], text: bool, replace: bool, now: f64) -> Result<Vec<u8>, JsError> {
    let mut img = image;
    {
        let mut vol = files::open(&mut img, offset as u64).map_err(js_err)?;
        vol.set_clock(icon_core::devices::rtc::icon_time(now as i64));
        if replace {
            if let Ok(e) = vol.lookup(path) {
                if !e.is_dir {
                    vol.remove(path).map_err(js_err)?;
                }
            }
        }
        let body = if text && files::looks_like_text(data) { qnx2fs::lf_to_rs(data) } else { data.to_vec() };
        vol.put(path, &body, &qnx2fs::NewEntry::default()).map_err(js_err)?;
    }
    Ok(img)
}

/// Create a directory.
#[wasm_bindgen]
pub fn disk_mkdir(image: Vec<u8>, offset: f64, path: &str, now: f64) -> Result<Vec<u8>, JsError> {
    let mut img = image;
    {
        let mut vol = files::open(&mut img, offset as u64).map_err(js_err)?;
        vol.set_clock(icon_core::devices::rtc::icon_time(now as i64));
        vol.mkdir(path, &qnx2fs::NewEntry::default()).map_err(js_err)?;
    }
    Ok(img)
}

/// Delete a file or an empty directory.
#[wasm_bindgen]
pub fn disk_remove(image: Vec<u8>, offset: f64, path: &str, now: f64) -> Result<Vec<u8>, JsError> {
    let mut img = image;
    {
        let mut vol = files::open(&mut img, offset as u64).map_err(js_err)?;
        vol.set_clock(icon_core::devices::rtc::icon_time(now as i64));
        vol.remove(path).map_err(js_err)?;
    }
    Ok(img)
}

/// The closest valid QNX 2 file name (1-16 of A-Z a-z 0-9 _ . $).
#[wasm_bindgen]
pub fn qnx_name(name: &str) -> String {
    qnx2fs::sanitize_name(name)
}

/// A hard-disk image laid out as the emulator uses it (drops the padding
/// heads some disk readers add), so partition offsets line up.
#[wasm_bindgen]
pub fn normalize_hard_disk(image: Vec<u8>) -> Vec<u8> {
    Disk::hard_auto(image).data
}

fn check_rom(rom: &[u8]) -> Result<(), JsError> {
    if rom.len() != 0x8000 {
        return Err(JsError::new("expected a 32 KB ROM image (the two 16 KB EPROMs interleaved, even bytes first)"));
    }
    Ok(())
}

/// Machine kinds a slot can hold. Only the original ICON hardware is emulated so far.
pub const LEXICON: &str = "lexicon";
pub const ICON: &str = "icon";
const NODE_KINDS: [&str; 1] = [ICON];

#[wasm_bindgen]
pub struct Network {
    lan: Lan,
    /// Kind of machine in each slot.
    kinds: Vec<String>,
    /// Boot ROM per machine kind.
    roms: std::collections::HashMap<String, Vec<u8>>,
    /// Memory fitted to every machine: 512, or 1024 for the 1 MB upgrade card.
    ram_kb: u32,
}

#[wasm_bindgen]
impl Network {
    /// A segment with the fileserver and one ICON workstation, and no ROMs yet.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Network {
        Network { lan: Lan::new(2), kinds: vec![LEXICON.into(), ICON.into()], roms: Default::default(), ram_kb: 512 }
    }

    /// Set the boot ROM for one machine kind ("lexicon", "icon"). Machines
    /// pick it up at their next power-on.
    pub fn set_rom(&mut self, kind: &str, rom: &[u8]) -> Result<(), JsError> {
        check_rom(rom)?;
        self.roms.insert(kind.to_string(), rom.to_vec());
        Ok(())
    }

    /// Memory for machines powered on from now: 512 KB, or 1024 KB (the
    /// CEMCORP 1 MB upgrade card, which needs the Rel 2.11 boot ROM).
    pub fn set_ram_kb(&mut self, kb: u32) {
        self.ram_kb = if kb == 1024 { 1024 } else { 512 };
    }

    pub fn has_rom(&self, kind: &str) -> bool {
        self.roms.contains_key(kind)
    }

    /// Number of slots (fileserver plus workstations).
    pub fn slots(&self) -> usize {
        self.lan.nodes.len()
    }

    /// The machine kind in a slot.
    pub fn kind(&self, slot: usize) -> String {
        self.kinds.get(slot).cloned().unwrap_or_default()
    }

    /// Add a powered-off node of the given kind; returns its slot number.
    pub fn add_node(&mut self, kind: &str) -> Result<usize, JsError> {
        if !NODE_KINDS.contains(&kind) {
            return Err(JsError::new(&format!("{kind} nodes are not emulated yet")));
        }
        self.lan.nodes.push(None);
        self.lan.grow();
        self.kinds.push(kind.to_string());
        Ok(self.lan.nodes.len() - 1)
    }

    /// Remove the last node if it is off and not the only workstation.
    pub fn remove_last_node(&mut self) -> bool {
        if self.lan.nodes.len() <= 2 || self.lan.nodes.last().is_some_and(|n| n.is_some()) {
            return false;
        }
        self.lan.nodes.pop();
        self.lan.grow();
        self.kinds.pop();
        true
    }

    /// Packets this node has sent or received so far (for its coax light).
    pub fn activity(&self, slot: usize) -> f64 {
        self.lan.activity.get(slot).copied().unwrap_or(0) as f64
    }

    /// Power a machine on. `host_time` is local wall-clock seconds (for the
    /// fileserver's battery clock); disks are attached separately. Fails if
    /// there is no ROM for the slot's kind.
    pub fn power_on(&mut self, slot: usize, host_time: f64) -> Result<(), JsError> {
        let Some(kind) = self.kinds.get(slot) else { return Err(JsError::new("no such node")) };
        let rom = self.roms.get(kind).ok_or_else(|| JsError::new(&format!("no {kind} ROM loaded")))?.clone();
        let fs = slot == FILESERVER;
        let mut nvram = if fs {
            presets::fileserver()
        } else {
            let mut nv = presets::workstation();
            nv[10] = (((15 + slot) as u16) << 8) | 1; // own node ID, boot server 1
            nv
        };
        presets::set_ram_kb(&mut nvram, self.ram_kb);
        let m = Machine::new(Config {
            rom,
            patch_800k_floppy: false,
            ram_kb: self.ram_kb,
            nvram,
            straps: STRAPS_NORMAL,
            host_time: host_time as i64,
            disk_controller: fs,
        });
        self.lan.power_on(slot, m);
        Ok(())
    }

    pub fn power_off(&mut self, slot: usize) {
        self.lan.power_off(slot);
    }

    pub fn is_on(&self, slot: usize) -> bool {
        self.lan.nodes.get(slot).is_some_and(|n| n.is_some())
    }

    /// Run every powered machine for one 60 Hz frame.
    pub fn run_frame(&mut self) {
        self.lan.run((CPU_HZ / 60) as u64);
    }

    fn m(&self, slot: usize) -> Option<&Machine> {
        self.lan.nodes.get(slot)?.as_ref()
    }
    fn m_mut(&mut self, slot: usize) -> Option<&mut Machine> {
        self.lan.nodes.get_mut(slot)?.as_mut()
    }

    /// The display as 640x240 RGBA (empty when off).
    pub fn screen(&self, slot: usize) -> Vec<u8> {
        self.m(slot).map(|m| m.board.render_rgba()).unwrap_or_default()
    }

    /// Queue a raw ICON key code (key number; bit 7 set = release).
    pub fn key(&mut self, slot: usize, code: u8) {
        if let Some(m) = self.m_mut(slot) {
            m.board.key(code);
        }
    }

    /// Type ASCII text on a machine's keyboard (`\r` = Enter).
    pub fn type_text(&mut self, slot: usize, text: &str) {
        if let Some(m) = self.m_mut(slot) {
            for k in icon_core::devices::keyboard::type_text(text) {
                m.board.key(k);
            }
        }
    }

    /// Key events still waiting to be sent to a machine's keyboard.
    pub fn keys_pending(&self, slot: usize) -> usize {
        self.m(slot).map(|m| m.board.kbd_queue.len()).unwrap_or(0)
    }

    /// Trackball motion in mouse counts (+x right, +y down). The ICON2's
    /// ICON emulator halves host counts; the X chip is select 01, Y is 00.
    pub fn trackball(&mut self, slot: usize, dx: i32, dy: i32) {
        if let Some(m) = self.m_mut(slot) {
            m.board.counters[1].motion(dx / 2);
            m.board.counters[0].motion(dy / 2);
        }
    }

    /// Speaker audio from all machines, mixed, since the last call.
    pub fn audio(&mut self, rate: u32) -> Vec<f32> {
        let mut mix: Vec<f32> = Vec::new();
        for m in self.lan.nodes.iter_mut().flatten() {
            let a = m.board.take_audio(rate);
            if mix.len() < a.len() {
                mix.resize(a.len(), 0.0);
            }
            for (o, x) in mix.iter_mut().zip(a) {
                *o += x;
            }
        }
        mix
    }

    /// Front-panel lights: bit 0 trouble LED, bit 1 hard disk, bit 2 floppy,
    /// bit 3 receiver armed (network ready).
    pub fn lights(&self, slot: usize) -> u8 {
        let Some(m) = self.m(slot) else { return 0 };
        let a = &m.board.wd.activity;
        (m.board.fa88 & 1)
            | ((a[0] > 0) as u8) << 1
            | ((a[3] > 0) as u8) << 2
            | ((m.board.arcnet.status & 0x80 == 0) as u8) << 3
    }

    pub fn status(&self, slot: usize) -> String {
        let Some(m) = self.m(slot) else { return "off".into() };
        let r = m.cpu.regs();
        format!("node {} · {:04X}:{:04X}", m.board.arcnet.node_id, r.cs, r.ip)
    }

    pub fn packets(&self) -> f64 {
        self.lan.packets as f64
    }

    /// Insert a floppy image into the fileserver's drive.
    pub fn insert_floppy(&mut self, data: &[u8]) -> Result<(), JsError> {
        let disk = Disk::floppy_from_image(data.to_vec())
            .ok_or_else(|| JsError::new("unrecognised floppy image size"))?;
        if let Some(m) = self.m_mut(FILESERVER) {
            m.board.insert_floppy(Some(disk));
        }
        Ok(())
    }

    pub fn eject_floppy(&mut self) {
        if let Some(m) = self.m_mut(FILESERVER) {
            m.board.insert_floppy(None);
        }
    }

    /// Current floppy contents, including what QNX wrote (empty if none).
    pub fn floppy_image(&self) -> Vec<u8> {
        self.m(FILESERVER)
            .and_then(|m| m.board.wd.drives[3].as_ref())
            .map(|d| d.data.clone())
            .unwrap_or_default()
    }

    /// Attach the fileserver's hard disk. Geometry comes from a geometry
    /// record in block 1 when present, otherwise 4 heads x 17 sectors.
    pub fn attach_hard_disk(&mut self, data: &[u8]) {
        if let Some(m) = self.m_mut(FILESERVER) {
            m.board.attach_hard_disk(Some(Disk::hard_auto(data.to_vec())));
        }
    }

    /// Current hard disk contents (empty if none).
    pub fn hard_disk_image(&self) -> Vec<u8> {
        self.m(FILESERVER)
            .and_then(|m| m.board.wd.drives[0].as_ref())
            .map(|d| d.data.clone())
            .unwrap_or_default()
    }

    /// True if the hard disk changed since the last call (then clears the flag).
    pub fn take_hard_disk_dirty(&mut self) -> bool {
        match self.m_mut(FILESERVER).and_then(|m| m.board.wd.drives[0].as_mut()) {
            Some(d) if d.dirty => {
                d.dirty = false;
                true
            }
            _ => false,
        }
    }
}
