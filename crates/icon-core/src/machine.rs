//! The ICON main board: memory map, I/O decode and device wiring.
//!
//! Map (see icon_icon2_research/ICON_firmware.md §2–3):
//! - 00000–7FFFF  DRAM (384 KB / 512 KB / 1 MB fitted)
//! - D8000–D8FFF  COM9026 buffer RAM, even bytes only
//! - E0000–F7FFF  empty "FUTURE" EPROM sockets (reads FFh)
//! - F8000–FFFFF  boot ROM (5053-1/-2 ISS 2, 32 KB)
//!
//! I/O: F801/F803 CRTC, F880/F882 COM9026, F900–F906 palette, F920 keyboard,
//! F981–F987 PPI A, FA01 TMS5220, FA88 beeper/LED, FAA0–FAAE disk task
//! file, FAC1–FAC7 PPI B, FAE1–FAE7 serial.

use crate::cpu::{CpuBus, I80186};
use crate::devices::{arcnet::Com9026, counter::MwCounter, crtc::Crtc, nvram::Nvram, ppi8255::Ppi8255, serial::Sio8274};
use std::collections::VecDeque;

/// CPU clock: 14.318 MHz crystal / 2.
pub const CPU_HZ: u32 = 7_159_090;
const FRAME_HZ: u32 = 60;

/// Value of PPI A port A bits 0–4 (board straps). 1Fh = normal boot.
pub const STRAPS_NORMAL: u8 = 0x1F;

pub struct Config {
    pub rom: Vec<u8>,
    pub ram_kb: u32,
    pub nvram: [u16; 16],
    pub straps: u8,
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
    /// Cycle counter, used for video timing.
    pub cycles: u64,
    /// Unhandled I/O accesses seen, for discovering missing hardware.
    pub unknown_io: std::collections::BTreeMap<u16, u32>,
}

pub struct Machine {
    pub cpu: I80186,
    pub board: Board,
}

impl Machine {
    pub fn new(cfg: Config) -> Self {
        assert_eq!(cfg.rom.len(), 0x8000, "expected a 32 KB interleaved ICON ROM");
        let board = Board {
            ram: vec![0; (cfg.ram_kb as usize) * 1024],
            rom: cfg.rom,
            ppi_a: Ppi8255::default(),
            ppi_b: Ppi8255::default(),
            nvram: Nvram::new(cfg.nvram),
            counters: Default::default(),
            crtc: Crtc::default(),
            sio: Sio8274::default(),
            arcnet: Com9026::default(),
            palette: [0; 4],
            // At power-up the keyboard reports CAPS LOCK and CUR LOCK released.
            kbd_queue: VecDeque::from([0xAF, 0xBF]),
            kbd_latch: 0xFF,
            kbd_pending: false,
            straps: cfg.straps & 0x1F,
            fa88: 0,
            fa88_edges: 0,
            cycles: 0,
            unknown_io: Default::default(),
        };
        let mut cpu = I80186::new();
        cpu.reset();
        Self { cpu, board }
    }

    /// Execute one instruction; returns cycles used.
    pub fn step(&mut self) -> u32 {
        let c = self.cpu.step(&mut self.board);
        self.board.cycles += c as u64;
        self.board.arcnet.tick(self.board.cycles);
        self.board.service_keyboard();
        // INT0 is shared by the keyboard (PA7) and disk (PA5); the kernel's
        // handler polls PPI A port A. The COM9026 (PC5) is on INT2.
        self.cpu.set_int_line(0, self.board.kbd_pending);
        self.cpu.set_int_line(2, self.board.arcnet.irq());
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

impl Board {
    /// Vertical sync, derived from elapsed cycles and the CRTC frame length.
    fn vsync(&self) -> bool {
        let frame = (CPU_HZ / FRAME_HZ) as u64;
        let lines = self.crtc.lines_per_frame() as u64;
        let line = (self.cycles % frame) * lines / frame;
        line < 3
    }

    /// Queue a key press or release (raw code: key number, bit 7 = release).
    pub fn key(&mut self, code: u8) {
        self.kbd_queue.push_back(code);
    }

    fn service_keyboard(&mut self) {
        if !self.kbd_pending {
            if let Some(code) = self.kbd_queue.pop_front() {
                self.kbd_latch = !code;
                self.kbd_pending = true;
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
        let mut pa = self.straps; // bit 5 = disk IRQ (idle low)
        if self.kbd_pending {
            pa |= 0x80;
        }
        let mut pc = 0u8;
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
        let sel = self.microwire_select();
        let (sk, di) = (pc & 0x02 != 0, pc & 0x01 != 0);
        self.nvram.set_pins(sel == Some(2), sk, di);
        for (i, c) in self.counters.iter_mut().enumerate() {
            if i != 2 {
                c.set_pins(sel == Some(i as u8), sk, di);
            }
        }
        self.arcnet.set_reset(pb & 0x02 != 0, self.ppi_b.port_a_out(), self.cycles);
    }

    fn note_unknown(&mut self, port: u16) {
        *self.unknown_io.entry(port).or_insert(0) += 1;
    }
}

impl CpuBus for Board {
    fn mem_read8(&mut self, addr: u32) -> u8 {
        let addr = addr & 0xF_FFFF;
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
        match addr {
            a if (a as usize) < self.ram.len() => self.ram[a as usize] = val,
            0xD8000..=0xD8FFF if addr & 1 == 0 => {
                self.arcnet.ram[((addr - 0xD8000) >> 1) as usize] = val
            }
            _ => {}
        }
    }

    fn io_read8(&mut self, port: u16) -> u8 {
        match port {
            0xF803 => self.crtc.read_data(),
            0xF920 => self.kbd_latch,
            0xFAE1 => self.sio.ch[0].read_data(),
            0xFAE3 => self.sio.ch[1].read_data(),
            0xFAE5 => self.sio.ch[0].read_control(),
            0xFAE7 => self.sio.ch[1].read_control(),
            0xF880 => self.arcnet.read_status(),
            0xF981 | 0xF983 | 0xF985 | 0xF987 => {
                let inputs = self.ppi_a_inputs();
                self.ppi_a.read(((port - 0xF981) >> 1) as u8, inputs)
            }
            0xFAC1 | 0xFAC3 | 0xFAC5 | 0xFAC7 => {
                self.ppi_b.read(((port - 0xFAC1) >> 1) as u8, [0xFF, 0xFF, 0xFF])
            }
            _ => {
                self.note_unknown(port);
                0xFF
            }
        }
    }

    fn io_write8(&mut self, port: u16, val: u8) {
        match port {
            0xF801 => self.crtc.write_addr(val),
            0xF803 => self.crtc.write_data(val),
            0xF880 => self.arcnet.write_mask(val),
            0xF882 => self.arcnet.command(val),
            0xF920 => self.kbd_pending = false,
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
            0xFA88 => {
                if val != self.fa88 {
                    self.fa88_edges += 1;
                }
                self.fa88 = val;
            }
            0xFAC1 | 0xFAC3 | 0xFAC5 | 0xFAC7 => {
                self.ppi_b.write(((port - 0xFAC1) >> 1) as u8, val);
                self.update_pins();
            }
            _ => self.note_unknown(port),
        }
    }
}
