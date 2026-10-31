//! The ICON main board: memory map, I/O decode and device wiring.
//!
//! Map (see icon_icon2_research/ICON_firmware.md §2–3):
//! - 00000–7FFFF  DRAM (384 KB / 512 KB / 1 MB fitted)
//! - D8000–D8FFF  COM9026 buffer RAM, even bytes only
//! - E0000–F7FFF  empty "FUTURE" EPROM sockets (reads FFh)
//! - F8000–FFFFF  boot ROM (5053-1/-2 ISS 2, 32 KB)
//!
//! With the CEMCORP 1 MB upgrade card (`ram_kb` 1024) the map above 7FFFF
//! follows the 80186 chip selects instead; see [`Board::card_read8`].
//!
//! I/O: F801/F803 CRTC, F880/F882 COM9026, F900–F906 palette, F920 keyboard,
//! F981–F987 PPI A, FA01 TMS5220, FA88 beeper/LED, FAA0–FAAE disk task
//! file, FAC1–FAC7 PPI B, FAE1–FAE7 serial.

use crate::cpu::{CpuBus, I80186};
use crate::cpu::ChipSelects;
use crate::devices::parallel::Parallel;
use crate::devices::{arcnet::Com9026, counter::MwCounter, crtc::Crtc, nvram::Nvram, ppi8255::Ppi8255, serial::Sio8274, wd1002::{Disk, Wd1002}, rtc::Msm5832, tms5220::Tms5220};

/// TMS5220C clock. It comes from an RC oscillator on the board; 640 kHz
/// (8 kHz output) is the usual value and has not been measured on an ICON.
pub const SPEECH_CLOCK: u32 = 640_000;
use std::collections::VecDeque;

/// CPU clock: 14.318 MHz crystal / 2.
pub const CPU_HZ: u32 = 7_159_090;
const FRAME_HZ: u32 = 60;

/// Value of PPI A port A bits 0–4 (board straps). 1Fh = normal boot.
pub const STRAPS_NORMAL: u8 = 0x1F;

pub struct Config {
    pub rom: Vec<u8>,
    /// Patch the ROM floppy loader for 10-sector (800K) disks. The real
    /// fileservers had a later ROM (Rel. 2.11) that could boot them; the
    /// 5053 ISS 2 loader only handles 8 sectors per track.
    pub patch_800k_floppy: bool,
    /// 128–512 for the main board alone; 1024 fits the CEMCORP 1 MB card.
    pub ram_kb: u32,
    pub nvram: [u16; 16],
    pub straps: u8,
    /// Host wall-clock time (Unix seconds) at power-on, for the RTC.
    pub host_time: i64,
    /// A WD1002-05 disk controller is fitted (fileservers). Workstations
    /// have none, so its ports read FFh.
    pub disk_controller: bool,
}

pub struct Board {
    pub ram: Vec<u8>,
    pub rom: Vec<u8>,
    pub ppi_a: Ppi8255,
    pub ppi_b: Ppi8255,
    pub nvram: Nvram,
    /// Microwire counter chips, indexed by the select field (PPI A port B
    /// bits 7..6): 0 = trackball Y, 1 = trackball X, 3 = baud-rate. Select 2
    /// is the NVRAM.
    pub counters: [MwCounter; 4],
    pub crtc: Crtc,
    pub sio: Sio8274,
    pub wd: Wd1002,
    pub has_wd: bool,
    pub rtc: Msm5832,
    /// FA80-FA8E addressable latch outputs (bit 0 of each write).
    latch_fa8x: [bool; 8],
    pub arcnet: Com9026,
    pub palette: [u8; 4],
    pub straps: u8,
    /// FA88 output (beeper / trouble LED) and a log of its edges.
    pub fa88: u8,
    pub fa88_edges: u64,
    /// Keyboard: raw key codes waiting to be sent, and the byte on the F920
    /// latch (the bus carries the complement of the code). PA7 is high while
    /// a byte is pending; any write to F920 acknowledges it.
    pub kbd_queue: VecDeque<u8>,
    kbd_latch: u8,
    kbd_pending: bool,
    kbd_next_at: u64,
    /// Cycle counter, used for video timing.
    pub cycles: u64,
    /// Speaker: PPI A port C bit 3, toggled directly by software. Level
    /// changes are timestamped so a front end can render audio.
    speaker: bool,
    speaker_edges: Vec<(u64, bool)>,
    audio_cursor: u64,
    audio_dc: f32,
    audio_prev: f32,
    /// TMS5220C speech chip (port FA01, interrupt on PPI A PC4), fitted when
    /// NVRAM word 0 bit 8 ("no speech") is clear: workstations, not fileservers.
    pub speech: Option<Tms5220>,
    /// Speech samples not yet taken by `take_audio`, the CPU cycle of the
    /// first one, and the cycle the next one is due.
    speech_samples: Vec<i16>,
    speech_base: f64,
    speech_next: f64,
    /// Debug: log writes to this linear address range as (cycle, addr, value).
    pub watch: Option<(u32, u32)>,
    pub watch_log: Vec<(u64, u32, u8)>,
    /// Cached RAM address of the running kernel's console font table.
    font_at: std::cell::Cell<Option<usize>>,
    rom_font_at: std::cell::Cell<Option<usize>>,
    /// Address of the instruction being executed (set by `Machine::step`).
    pub cur_pc: u32,
    pub watch_pcs: Vec<u32>,
    /// When set, every I/O access is appended here: (cycle, port, value, is_write).
    pub io_log: Option<Vec<(u64, u16, u8, bool)>>,
    /// Unhandled I/O accesses seen, for discovering missing hardware.
    pub unknown_io: std::collections::BTreeMap<u16, u32>,
    /// The 1 MB upgrade card is fitted (`ram_kb` 1024).
    pub mem_card: bool,
    /// The parallel printer port and the printer on it.
    pub parallel: Parallel,
    /// Last word written to the card's port CF80 (None until written).
    pub card_latch: Option<u16>,
    /// The CPU's chip-select registers, copied by `Machine::step` when the
    /// card is fitted (the card sits in the CPU socket and sees the outputs).
    pub chip_selects: ChipSelects,
}

pub struct Machine {
    pub cpu: I80186,
    pub board: Board,
    /// Registers at the first jump from the boot ROM into loaded code
    /// (debugging aid: ES:BX is the boot parameter block).
    pub boot_entry: Option<crate::cpu::Regs>,
    /// Debug: the 0x40 bytes at ES:BX when `boot_entry` was captured.
    pub boot_params: Vec<u8>,
    /// Debug: when Some, log `int 5Ch` kernel calls as (cycle, fn, result).
    pub int5c_log: Option<Vec<(u64, u16, u16)>>,
    int5c_pending: Vec<(u16, u16, u16)>,
}

impl Machine {
    pub fn new(cfg: Config) -> Self {
        assert_eq!(cfg.rom.len(), 0x8000, "expected a 32 KB interleaved ICON ROM");
        let mut cfg = cfg;
        if cfg.patch_800k_floppy {
            patch_800k_floppy(&mut cfg.rom);
        }
        let board = Board {
            ram: vec![0; (cfg.ram_kb as usize) * 1024],
            rom: cfg.rom,
            ppi_a: Ppi8255::default(),
            ppi_b: Ppi8255::default(),
            nvram: Nvram::new(cfg.nvram),
            counters: Default::default(),
            crtc: Crtc::default(),
            sio: Sio8274::default(),
            wd: Wd1002::default(),
            has_wd: cfg.disk_controller,
            rtc: Msm5832::new(cfg.host_time),
            latch_fa8x: [false; 8],
            arcnet: Com9026::default(),
            palette: [0; 4],
            // At power-up the keyboard reports CAPS LOCK and CUR LOCK released.
            kbd_queue: VecDeque::from([0xAF, 0xBF]),
            kbd_latch: 0xFF,
            kbd_pending: false,
            kbd_next_at: 0,
            straps: cfg.straps & 0x1F,
            fa88: 0,
            fa88_edges: 0,
            cycles: 0,
            io_log: None,
            watch: None,
            watch_log: Vec::new(),
            cur_pc: 0,
            font_at: std::cell::Cell::new(None),
            rom_font_at: std::cell::Cell::new(None),
            watch_pcs: Vec::new(),
            speaker: false,
            speaker_edges: Vec::new(),
            audio_cursor: 0,
            audio_dc: 0.0,
            audio_prev: 0.0,
            speech: (cfg.nvram[0] & 0x0100 == 0).then(Tms5220::default),
            speech_samples: Vec::new(),
            speech_base: 0.0,
            speech_next: 0.0,
            unknown_io: Default::default(),
            mem_card: cfg.ram_kb > 512,
            parallel: Parallel::default(),
            card_latch: None,
            chip_selects: ChipSelects::default(),
        };
        let mut cpu = I80186::new();
        cpu.reset();
        Self { cpu, board, boot_entry: None, boot_params: Vec::new(), int5c_log: None, int5c_pending: Vec::new() }
    }

    /// Execute one instruction; returns cycles used.
    pub fn step(&mut self) -> u32 {
        let from_rom = self.cpu.pc() >= 0xF8000;
        self.board.cur_pc = self.cpu.pc();
        if let Some(log) = self.int5c_log.as_mut() {
            let r = self.cpu.regs();
            if let Some(&(cs, ip, func)) = self.int5c_pending.last() {
                if r.cs == cs && r.ip == ip {
                    self.int5c_pending.pop();
                    log.push((self.board.cycles, func, r.ax));
                }
            }
            let pc = self.cpu.pc();
            use crate::cpu::CpuBus;
            if self.board.mem_read8(pc) == 0xCD && self.board.mem_read8(pc + 1) == 0x5C && self.int5c_pending.len() < 64 {
                self.int5c_pending.push((r.cs, r.ip.wrapping_add(2), r.ax));
            }
        }
        let c = self.cpu.step(&mut self.board);
        if self.board.mem_card {
            self.board.chip_selects = *self.cpu.chip_selects();
        }
        if from_rom && self.boot_entry.is_none() && self.cpu.pc() < 0xE0000 && self.board.cycles > 1_000_000 {
            let r = self.cpu.regs();
            let base = ((r.es as usize) << 4) + r.bx as usize;
            self.boot_params = (0..0x40).map(|i| self.board.ram.get(base + i).copied().unwrap_or(0xFF)).collect();
            self.boot_entry = Some(r);
            // Debug: RAM as the ROM hands over to the loaded image.
            if let Some(path) = std::env::var_os("ICON_DUMP_AT_ENTRY") {
                let _ = std::fs::write(format!("{}.{:02x}", path.to_string_lossy(), self.board.arcnet.node_id), &self.board.ram);
            }
        }
        self.board.cycles += c as u64;
        self.board.arcnet.tick(self.board.cycles);
        self.board.wd.tick(c);
        self.board.service_keyboard();
        self.board.run_speech();
        // INT0 is shared by the keyboard (PA7) and disk (PA5); the kernel's
        // handler polls PPI A port A. INT2 is shared by the COM9026 (PC5) and
        // the speech chip (PC4), and the printer (PPI B PC0).
        self.cpu.set_int_line(0, self.board.kbd_pending || self.board.wd.irq);
        self.cpu.set_int_line(2, self.board.arcnet.irq() || self.board.speech_irq() || self.board.parallel.intr);
        c
    }

    /// Run for at least `cycles` CPU clocks.
    pub fn run(&mut self, cycles: u64) {
        let end = self.board.cycles + cycles;
        while self.board.cycles < end {
            self.step();
        }
    }
}

/// Change the WD1002 floppy loader (ROM file 0x170) from 80x2x8 to 80x2x10:
/// its block limit (0x500 -> 0x640), sectors per cylinder (16 -> 20) and the
/// per-track sector bound (9 -> 11).
fn patch_800k_floppy(rom: &mut [u8]) {
    let patches: [(usize, &[u8], &[u8]); 4] = [
        (0x1F1, &[0x3D, 0x00, 0x05], &[0x3D, 0x40, 0x06]),
        (0x1F8, &[0xBB, 0x10, 0x00], &[0xBB, 0x14, 0x00]),
        (0x201, &[0xBB, 0x08, 0x00], &[0xBB, 0x0A, 0x00]),
        (0x22C, &[0x83, 0x7E, 0x04, 0x09], &[0x83, 0x7E, 0x04, 0x0B]),
    ];
    for (off, old, new) in patches {
        if &rom[off..off + old.len()] == old {
            rom[off..off + new.len()].copy_from_slice(new);
        }
    }
}

impl Board {
    pub fn insert_floppy(&mut self, disk: Option<Disk>) {
        self.wd.drives[3] = disk;
    }

    pub fn attach_hard_disk(&mut self, disk: Option<Disk>) {
        self.wd.drives[0] = disk;
    }

    /// Vertical sync, derived from elapsed cycles and the CRTC frame length.
    fn vsync(&self) -> bool {
        let frame = (CPU_HZ / FRAME_HZ) as u64;
        let lines = self.crtc.lines_per_frame() as u64;
        let line = (self.cycles % frame) * lines / frame;
        line < 3
    }

    /// Take the speech samples generated since the last call (8 kHz, at most
    /// the last ~8 seconds if nothing has been collecting audio). For tests.
    pub fn take_speech_samples(&mut self) -> Vec<i16> {
        self.run_speech();
        let per = CPU_HZ as f64 * 80.0 / SPEECH_CLOCK as f64;
        self.speech_base += self.speech_samples.len() as f64 * per;
        std::mem::take(&mut self.speech_samples)
    }

    fn speech_irq(&self) -> bool {
        self.speech.as_ref().is_some_and(|s| s.irq)
    }

    /// Bring the speech chip up to the current cycle, one sample per 80 of its clocks.
    fn run_speech(&mut self) {
        let Some(chip) = self.speech.as_mut() else { return };
        let per = CPU_HZ as f64 * 80.0 / SPEECH_CLOCK as f64;
        if self.speech_samples.is_empty() {
            self.speech_base = self.speech_next;
        }
        while self.speech_next <= self.cycles as f64 {
            // Nobody is taking audio (headless): keep only the recent past.
            if self.speech_samples.len() >= 1 << 16 {
                self.speech_samples.drain(..1 << 15);
                self.speech_base += (1 << 15) as f64 * per;
            }
            self.speech_samples.push(chip.sample());
            self.speech_next += per;
        }
    }

    /// Render the speaker output from the last call up to now as mono
    /// samples at `rate` Hz (box-filtered square wave, DC removed).
    pub fn take_audio(&mut self, rate: u32) -> Vec<f32> {
        let per_sample = CPU_HZ as f64 / rate as f64;
        let n = ((self.cycles - self.audio_cursor) as f64 / per_sample) as usize;
        let mut out = Vec::with_capacity(n);
        let mut edges = std::mem::take(&mut self.speaker_edges).into_iter().peekable();
        // Level at the start of this block: the first edge's opposite, or now.
        let mut level = match edges.peek() {
            Some(&(_, l)) => !l,
            None => self.speaker,
        };
        for i in 0..n {
            let start = self.audio_cursor as f64 + i as f64 * per_sample;
            let end = start + per_sample;
            let mut high = 0.0;
            let mut t = start;
            while let Some(&(c, l)) = edges.peek() {
                let c = c as f64;
                if c >= end {
                    break;
                }
                if level {
                    high += c.max(t) - t;
                }
                t = c.max(t);
                level = l;
                edges.next();
            }
            if level {
                high += end - t;
            }
            let x = (high / per_sample) as f32 * 2.0 - 1.0;
            // One-pole DC blocker so an idle speaker is silent.
            let y = x - self.audio_prev + 0.995 * self.audio_dc;
            self.audio_prev = x;
            self.audio_dc = y;
            out.push(y * 0.25);
        }
        self.mix_speech(&mut out, per_sample);
        self.audio_cursor += (n as f64 * per_sample) as u64;
        self.speaker_edges = edges.collect(); // edges after this block's end
        out
    }

    /// Add the speech chip's samples (8 kHz) to a block of `take_audio`
    /// output, holding each speech sample for its duration.
    fn mix_speech(&mut self, out: &mut [f32], per_sample: f64) {
        if self.speech.is_none() {
            return;
        }
        self.run_speech();
        let per = CPU_HZ as f64 * 80.0 / SPEECH_CLOCK as f64;
        let mut used = 0;
        for (i, o) in out.iter_mut().enumerate() {
            let t = self.audio_cursor as f64 + i as f64 * per_sample;
            if t < self.speech_base {
                continue;
            }
            let k = ((t - self.speech_base) / per) as usize;
            if let Some(&v) = self.speech_samples.get(k) {
                *o += v as f32 / 32768.0 * 0.8;
                used = k;
            }
        }
        // Keep the sample still playing at the end of this block.
        let drop = used.min(self.speech_samples.len());
        self.speech_samples.drain(..drop);
        self.speech_base += drop as f64 * per;
    }

    /// Queue a key press or release (raw code: key number, bit 7 = release).
    pub fn key(&mut self, code: u8) {
        self.kbd_queue.push_back(code);
    }

    fn service_keyboard(&mut self) {
        // The keyboard MCU sends a byte at most every ~2 ms.
        if !self.kbd_pending && self.cycles >= self.kbd_next_at {
            if let Some(code) = self.kbd_queue.pop_front() {
                self.kbd_latch = !code;
                self.kbd_pending = true;
                self.kbd_next_at = self.cycles + 15_000;
            }
        }
    }

    /// Microwire device on PPI A port B: bits 7..6 select (00 trackball Y,
    /// 01 trackball X, 10 NVRAM, 11 baud-rate counter), bit 5 is an active-low
    /// enable. Returns the selected device while enabled.
    fn microwire_select(&self) -> Option<u8> {
        let pb = self.ppi_a.port_b_out();
        (pb & 0x20 == 0).then_some(pb >> 6)
    }

    /// Start of the visible page: NVRAM word 0 bits 0-1 pick the bank, and
    /// PPI A port B bit 0 picks page 1 (base) or page 2 (base - 32 KB).
    pub fn frame_base(&self) -> u32 {
        let base = match self.nvram.words[0] & 3 {
            0 => 0x3ABA0,
            1 => 0x5ABA0,
            2 => 0x7ABA0,
            _ => 0x1ABA0,
        };
        if self.ppi_a.port_b_out() & 1 != 0 { base } else { base - 0x8000 }
    }

    /// Decode the text console back into characters.
    ///
    /// The QNX console draws 8x10 glyphs on rows 9 lines apart (each glyph's
    /// bottom line overlaps the next row's top). Glyphs come from the running
    /// kernel's font table when it can be found in RAM (it differs from the
    /// ROM font in a dozen characters), otherwise from the ROM font at file
    /// 0x2CF4. Glyph bytes are bottom row first, LSB = leftmost pixel.
    /// Inverse cells (the cursor) decode too; unknown cells become '?'.
    pub fn screen_text(&self) -> String {
        let rom_font = self.rom_font();
        let rom_glyph = |c: u8| &self.rom[rom_font + (c - 0x20) as usize * 10..][..10];
        let kernel = self.kernel_font();
        // Glyph c from the font table starting at `table` (bottom row first),
        // returned top row first.
        let glyph_at = |table: Option<usize>, c: u8| -> [u8; 10] {
            let g: &[u8] = match table {
                Some(k) => self.ram.get(k + c as usize * 10..k + c as usize * 10 + 10).unwrap_or(&[0; 10]),
                None => rom_glyph(c),
            };
            let mut top_first = [0u8; 10];
            for k in 0..10 {
                top_first[k] = g[9 - k];
            }
            top_first
        };
        // Each table pairs a 10-line lookup (needed for '_', whose pixels
        // are on the 10th line) with a top-9-line one, so an overlapping 10th
        // line from the next row doesn't matter.
        type Tables = (HashMap<[u8; 10], char>, HashMap<[u8; 9], char>);
        use std::collections::HashMap;
        let build = |t: Option<usize>| -> Tables {
            let (mut t10, mut t9) = (HashMap::new(), HashMap::new());
            for c in (0x20u8..0x7F).rev() {
                let g = glyph_at(t, c);
                let mut top = [0u8; 9];
                top.copy_from_slice(&g[..9]);
                t9.insert(top, c as char);
                t9.entry(top.map(|b| !b)).or_insert(c as char);
                if c != b' ' && g != [0; 10] {
                    t10.insert(g, c as char);
                }
            }
            // A blank cell is a space in either polarity (Rel 2.11 leaves the
            // screen inverted), whatever unused glyphs the table also has.
            t9.insert([0; 9], ' ');
            t9.insert([0xFF; 9], ' ');
            (t10, t9)
        };
        // The kernel's table comes first; the ROM font is the fallback, since
        // the 4.10 workstation kernel keeps a table like the 4.01 one in RAM
        // but draws its console with the ROM's glyphs.
        let tables: Vec<Tables> = match kernel {
            Some(k) => vec![build(Some(k)), build(None)],
            None => vec![build(None)],
        };
        let base = self.frame_base() as usize;
        let raw = |line: usize, col: usize| self.ram.get(base + line * 80 + col).copied().unwrap_or(0);
        // The 4.10 workstation kernel draws glyphs one pixel left of the cell
        // (its leftmost column lands in the previous byte), so cells are also
        // read shifted by one pixel; the screen decides.
        let byte = |shift: u32, line: usize, col: usize| -> u8 {
            match shift {
                0 => raw(line, col),
                _ => (raw(line, col) << 1) | if col < 79 { raw(line, col + 1) >> 7 } else { 0 },
            }
        };
        let decode = |shift: u32, line: usize, col: usize| -> Option<char> {
            let ten: [u8; 10] = std::array::from_fn(|k| byte(shift, line + k, col));
            let mut nine = [0u8; 9];
            nine.copy_from_slice(&ten[..9]);
            tables.iter().find_map(|(t10, t9)| t10.get(&ten).or_else(|| t9.get(&nine)).copied())
        };
        // Rows are found top to bottom: wherever pixels appear, the next 9
        // line offsets and both pixel shifts are tried, and the one decoding
        // the most characters (less the cells it can't) is taken. Console
        // rows sit on a fixed 9-line grid, but the workstation's login screen
        // puts its banner and prompt on different grids.
        let row = |shift: u32, line: usize| -> (i32, String) {
            let mut score = 0;
            let text = (0..80)
                // A cell the row's shift can't read may still read at the other.
                .map(|c| match decode(shift, line, c).or_else(|| decode(1 - shift, line, c)) {
                    Some(ch) => {
                        score += (ch != ' ') as i32;
                        ch
                    }
                    None => {
                        score -= 1;
                        '?'
                    }
                })
                .collect();
            (score, text)
        };
        let blank = |line: usize| (0..80).all(|c| raw(line, c) == 0);
        let mut out = String::new();
        let mut line = 0;
        while line <= 231 {
            if (line..line + 9).all(blank) {
                out.push('\n');
                line += 9;
                continue;
            }
            let (_, o, text) = (0..9usize)
                .filter(|o| line + o <= 231)
                .flat_map(|o| (0..2u32).map(move |s| (s, o)))
                .map(|(s, o)| {
                    let (score, text) = row(s, line + o);
                    (score, o, text)
                })
                .max_by_key(|(score, o, _)| (*score, std::cmp::Reverse(*o)))
                .unwrap();
            out.push_str(text.trim_end());
            out.push('\n');
            line += o + 9;
        }
        out
    }

    /// Offset of the boot ROM's 8x10 font (characters 20h-7Eh, 10 bytes each,
    /// bottom row first): 2CF4h in 5053 ISS 2, 301Ah in Rel 2.11. Found by
    /// its 'H' glyph and cached.
    fn rom_font(&self) -> usize {
        const H: [u8; 10] = [0x00, 0x00, 0x44, 0x44, 0x44, 0x7C, 0x44, 0x44, 0x44, 0x00];
        if let Some(f) = self.rom_font_at.get() {
            return f;
        }
        let f = self.rom.windows(10).position(|w| w == H).and_then(|p| p.checked_sub((b'H' - 0x20) as usize * 10)).unwrap_or(0x2CF4);
        self.rom_font_at.set(Some(f));
        f
    }

    /// Locate the kernel's console font: a table of 10-byte glyphs indexed
    /// from character 0 in which most capital letters match the ROM font
    /// (kernels differ in a few glyphs, so no single letter is trusted). The
    /// address is cached and re-checked cheaply on later calls.
    fn kernel_font(&self) -> Option<usize> {
        let rom_font = self.rom_font();
        let rom_glyph = |c: u8| &self.rom[rom_font + (c - 0x20) as usize * 10..][..10];
        let score = |base: usize| -> usize {
            (b'A'..=b'Z')
                .filter(|&c| self.ram.get(base + c as usize * 10..base + c as usize * 10 + 10) == Some(rom_glyph(c)))
                .count()
        };
        if let Some(b) = self.font_at.get() {
            if score(b) >= 20 {
                return Some(b);
            }
        }
        for &anchor in b"HETNS" {
            let g = rom_glyph(anchor);
            let mut i = 0;
            while let Some(p) = self.ram[i..].windows(10).position(|w| w == g) {
                let at = i + p;
                if let Some(base) = at.checked_sub(anchor as usize * 10) {
                    if score(base) >= 20 {
                        self.font_at.set(Some(base));
                        return Some(base);
                    }
                }
                i = at + 1;
            }
        }
        None
    }

    /// Render the display as 640x240 RGBA. 1 bpp: pixel 0 -> F900, pixel 1
    /// -> F906. 2 bpp (320 wide, doubled): value v -> F900 + 2v. LSB = left.
    pub fn render_rgba(&self) -> Vec<u8> {
        // CGA RGBI as the ICON2's ICON emulator renders it (12-bit RGB).
        const RGBI: [u16; 16] = [
            0x000, 0x00A, 0x090, 0x0BA, 0xB00, 0x909, 0x990, 0x999,
            0x333, 0x00F, 0x0F0, 0x0FF, 0xF00, 0xF0F, 0xFF0, 0xFFF,
        ];
        let rgb = |i: u8| {
            let c = RGBI[(i & 15) as usize];
            [((c >> 8) & 15) as u8 * 17, ((c >> 4) & 15) as u8 * 17, (c & 15) as u8 * 17, 255]
        };
        let one_bpp = self.ppi_a.port_b_out() & 4 != 0;
        let base = self.frame_base() as usize;
        let mut out = Vec::with_capacity(640 * 240 * 4);
        for y in 0..240 {
            for x in 0..80 {
                let b = self.ram.get(base + y * 80 + x).copied().unwrap_or(0);
                if one_bpp {
                    for bit in 0..8 {
                        let reg = if b >> bit & 1 != 0 { 3 } else { 0 };
                        out.extend_from_slice(&rgb(self.palette[reg]));
                    }
                } else {
                    for px in 0..4 {
                        let px = rgb(self.palette[((b >> (px * 2)) & 3) as usize]);
                        out.extend_from_slice(&px);
                        out.extend_from_slice(&px);
                    }
                }
            }
        }
        out
    }

    fn ppi_a_inputs(&self) -> [u8; 3] {
        let mut pa = self.straps;
        if self.wd.irq {
            pa |= 0x20;
        }
        if self.kbd_pending {
            pa |= 0x80;
        }
        let mut pc = 0u8;
        if self.speech_irq() {
            pc |= 0x10;
        }
        if self.arcnet.irq() {
            pc |= 0x20;
        }
        let mw_do = match self.microwire_select() {
            Some(2) => self.nvram.data_out(),
            Some(n) => self.counters[n as usize].data_out(),
            None => false,
        };
        if mw_do {
            pc |= 0x40;
        }
        if self.vsync() {
            pc |= 0x80;
        }
        [pa, 0xFF, pc]
    }

    /// Propagate PPI A/B output pins to the NVRAM and ARCNET chips.
    fn update_pins(&mut self) {
        let pb = self.ppi_a.port_b_out();
        let pc = self.ppi_a.port_c_out();
        let spk = pc & 0x08 != 0;
        if spk != self.speaker {
            self.speaker = spk;
            if self.speaker_edges.len() < 1 << 16 {
                self.speaker_edges.push((self.cycles, spk));
            }
        }
        let sel = self.microwire_select();
        let (sk, di) = (pc & 0x02 != 0, pc & 0x01 != 0);
        self.nvram.set_pins(sel == Some(2), sk, di);
        for (i, c) in self.counters.iter_mut().enumerate() {
            if i != 2 {
                c.set_pins(sel == Some(i as u8), sk, di);
            }
        }
        self.arcnet.set_reset(pb & 0x02 != 0, self.ppi_b.port_a_out(), self.cycles);
        // Printer STROBE is PPI A PC2; the data is PPI B port B, in mode 1.
        let inte = self.ppi_b.port_c_out() & 0x04 != 0;
        self.parallel.set_strobe(pc & 0x04 != 0, self.ppi_b.port_b_out(), inte, self.cycles);
    }

    /// What drives the bus at `addr` with the 1 MB card fitted.
    ///
    /// INFERRED from the Rel 2.11 ROM (file 0x1523) and the 4.10 kernels
    /// (RAM to F7000 from NVRAM, network RAM from MMCS/MPCS) until the PALs
    /// (U30/U31/U33) are read: the card supplies RAM everywhere the main
    /// board does not answer. The main board answers in the UCS block (ROM in
    /// its top 32 KB, empty sockets below) and in the MCS3 block (COM9026
    /// RAM). Until the ROM writes port CF80 the card stays off above 7FFFF,
    /// since at reset UCS covers only FFC00 up and the ROM runs from F8000.
    fn card_map(&self, addr: u32) -> Mapped {
        let cs = &self.chip_selects;
        if cs.mmcs != 0 && cs.mpcs != 0 {
            // Mid-range block: base MMCS[15:9], total size from MPCS[14:8]
            // (one-hot, 8 KB units), four equal MCS blocks.
            let base = ((cs.mmcs & 0xFE00) as u32) << 4;
            let block = (((cs.mpcs >> 8) & 0x7F) as u32) << 11;
            let mcs3 = base + 3 * block;
            if block != 0 && addr >= mcs3 && addr < mcs3 + block {
                return Mapped::Net(addr - mcs3);
            }
        }
        if (addr as usize) < 0x80000 {
            return Mapped::Ram;
        }
        let ucs = if self.card_latch.is_some() { ((cs.umcs & 0xFFC0) as u32) << 4 } else { 0x80000 };
        if addr >= ucs {
            if addr >= 0xF8000 { Mapped::Rom } else { Mapped::None }
        } else {
            Mapped::Ram
        }
    }

    fn card_read8(&mut self, addr: u32) -> u8 {
        match self.card_map(addr) {
            Mapped::Ram => self.ram[addr as usize],
            Mapped::Rom => self.rom[(addr & 0x7FFF) as usize],
            Mapped::Net(o) if o < 0x1000 && o & 1 == 0 => self.arcnet.ram[(o >> 1) as usize],
            _ => 0xFF,
        }
    }

    fn card_write8(&mut self, addr: u32, val: u8) {
        match self.card_map(addr) {
            Mapped::Ram => self.ram[addr as usize] = val,
            Mapped::Net(o) if o < 0x1000 && o & 1 == 0 => self.arcnet.ram[(o >> 1) as usize] = val,
            _ => {}
        }
    }

    fn note_unknown(&mut self, port: u16) {
        *self.unknown_io.entry(port).or_insert(0) += 1;
    }
}

enum Mapped {
    Ram,
    Rom,
    /// Offset into the MCS3 block (COM9026 RAM on even bytes of the first 4 KB).
    Net(u32),
    None,
}

impl CpuBus for Board {
    fn mem_read8(&mut self, addr: u32) -> u8 {
        let addr = addr & 0xF_FFFF;
        if self.mem_card {
            return self.card_read8(addr);
        }
        match addr {
            a if (a as usize) < self.ram.len() => self.ram[a as usize],
            0xD8000..=0xD8FFF => {
                if addr & 1 == 0 {
                    self.arcnet.ram[((addr - 0xD8000) >> 1) as usize]
                } else {
                    0xFF
                }
            }
            0xF8000..=0xFFFFF => self.rom[(addr - 0xF8000) as usize],
            _ => 0xFF,
        }
    }

    fn mem_write8(&mut self, addr: u32, val: u8) {
        let addr = addr & 0xF_FFFF;
        if let Some((lo, hi)) = self.watch {
            if addr >= lo && addr <= hi && self.watch_log.len() < 10_000 {
                self.watch_log.push((self.cycles, addr, val));
                self.watch_pcs.push(self.cur_pc);
            }
        }
        if self.mem_card {
            return self.card_write8(addr, val);
        }
        match addr {
            a if (a as usize) < self.ram.len() => self.ram[a as usize] = val,
            0xD8000..=0xD8FFF if addr & 1 == 0 => {
                self.arcnet.ram[((addr - 0xD8000) >> 1) as usize] = val
            }
            _ => {}
        }
    }

    fn io_read8(&mut self, port: u16) -> u8 {
        let v = self.io_read8_inner(port);
        if let Some(log) = self.io_log.as_mut() {
            log.push((self.cycles, port, v, false));
        }
        v
    }

    fn io_write8(&mut self, port: u16, val: u8) {
        if let Some(log) = self.io_log.as_mut() {
            log.push((self.cycles, port, val, true));
        }
        self.io_write8_inner(port, val)
    }
}

impl Board {
    fn io_read8_inner(&mut self, port: u16) -> u8 {
        match port {
            0xF803 => self.crtc.read_data(),
            0xF920 => self.kbd_latch,
            0xFAA0..=0xFAAE if port & 1 == 0 && self.has_wd => self.wd.read(((port - 0xFAA0) >> 1) as u8),
            // RTC data, valid while READ (FA82 = 0) is asserted.
            0xFA80 => {
                let reg = self.ppi_b.latch[1] & 0x0F;
                let secs = (self.cycles / CPU_HZ as u64) as i64;
                0xF0 | self.rtc.read(reg, secs)
            }
            0xFAE1 => self.sio.ch[0].read_data(),
            0xFAE3 => self.sio.ch[1].read_data(),
            0xFAE5 => self.sio.ch[0].read_control(),
            0xFAE7 => self.sio.ch[1].read_control(),
            0xF880 => self.arcnet.read_status(),
            0xFA01 if self.speech.is_some() => {
                self.run_speech();
                self.speech.as_mut().unwrap().read_status()
            }
            0xF981 | 0xF983 | 0xF985 | 0xF987 => {
                let inputs = self.ppi_a_inputs();
                self.ppi_a.read(((port - 0xF981) >> 1) as u8, inputs)
            }
            0xFAC1 | 0xFAC3 | 0xFAC5 | 0xFAC7 => {
                let reg = ((port - 0xFAC1) >> 1) as u8;
                // Port C inputs: the printer's BUSY on PC6 (low = ready).
                let pc_in = if self.parallel.online { 0xBF } else { 0xFF };
                let v = self.ppi_b.read(reg, [0xFF, 0xFF, pc_in]);
                if reg == 2 && self.ppi_b.control & 0x84 == 0x84 {
                    // Group B mode 1: PC0 = INTR_B, PC1 = OBF_B (high: the
                    // printer has taken the byte), PC2 = INTE_B.
                    (v & !0x03) | self.parallel.intr as u8 | 0x02
                } else {
                    v
                }
            }
            _ => {
                self.note_unknown(port);
                0xFF
            }
        }
    }

    fn io_write8_inner(&mut self, port: u16, val: u8) {
        match port {
            0xCF80 if self.mem_card => self.card_latch = Some(self.card_latch.unwrap_or(0) & 0xFF00 | val as u16),
            0xCF81 if self.mem_card => self.card_latch = Some(self.card_latch.unwrap_or(0) & 0x00FF | (val as u16) << 8),
            0xF801 => self.crtc.write_addr(val),
            0xF803 => self.crtc.write_data(val),
            0xF880 => self.arcnet.write_mask(val),
            0xFA01 if self.speech.is_some() => {
                self.run_speech();
                self.speech.as_mut().unwrap().write(val)
            }
            0xF882 => self.arcnet.command(val),
            0xF920 => self.kbd_pending = false,
            0xFAA0..=0xFAAE if port & 1 == 0 && self.has_wd => self.wd.write(((port - 0xFAA0) >> 1) as u8, val),
            0xFAE1 => self.sio.ch[0].write_data(val),
            0xFAE3 => self.sio.ch[1].write_data(val),
            0xFAE5 => self.sio.ch[0].write_control(val),
            0xFAE7 => self.sio.ch[1].write_control(val),
            0xF900 | 0xF902 | 0xF904 | 0xF906 => {
                self.palette[((port - 0xF900) >> 1) as usize] = val & 0x0F
            }
            0xF981 | 0xF983 | 0xF985 | 0xF987 => {
                self.ppi_a.write(((port - 0xF981) >> 1) as u8, val);
                self.update_pins();
            }
            0xFA80..=0xFA8E if port & 1 == 0 && port != 0xFA88 => {
                let i = ((port - 0xFA80) >> 1) as usize;
                let bit = val & 1 != 0;
                // FA84 falling edge with FA80 set: RTC write strobe.
                if i == 2 && self.latch_fa8x[2] && !bit && self.latch_fa8x[0] {
                    let b = self.ppi_b.latch[1];
                    let secs = (self.cycles / CPU_HZ as u64) as i64;
                    self.rtc.write(b & 0x0F, b >> 4, secs);
                }
                self.latch_fa8x[i] = bit;
            }
            0xFA88 => {
                if val != self.fa88 {
                    self.fa88_edges += 1;
                }
                self.fa88 = val;
            }
            0xFAC1 | 0xFAC3 | 0xFAC5 | 0xFAC7 => {
                let reg = ((port - 0xFAC1) >> 1) as u8;
                if reg == 1 {
                    // Writing the mode 1 output port clears INTR_B.
                    self.parallel.intr = false;
                }
                self.ppi_b.write(reg, val);
                self.update_pins();
            }
            _ => self.note_unknown(port),
        }
    }
}
