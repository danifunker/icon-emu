//! NMC9306 / 93C06-style Microwire serial EEPROM: 16 words x 16 bits.
//!
//! The ICON bit-bangs it through PPI A: CS = port B bit 7, DI = port C
//! bit 0, SK = port C bit 1, DO = port C bit 6 (see ICON_firmware.md §5).
//! Commands are a start bit, a 2-bit opcode and 6 address bits (only the low
//! 4 are used), MSB first, sampled on the rising edge of SK.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Idle,
    /// A 0 has been clocked in; the next 1 is the start bit (as MAME's nmc9306).
    Armed,
    /// Collecting opcode + address bits after the start bit.
    Command { bits: u8, count: u8 },
    /// Shifting out data, MSB first.
    Reading { word: u16, remaining: u8 },
    /// Collecting 16 data bits for WRITE.
    Writing { addr: u8, data: u16, count: u8 },
    /// Command done; waiting for CS to drop.
    Done,
}

#[derive(Clone, Debug)]
pub struct Nvram {
    pub words: [u16; 16],
    write_enabled: bool,
    cs: bool,
    sk: bool,
    do_bit: bool,
    state: State,
    /// Set whenever the contents change, so a front end can persist them.
    pub dirty: bool,
    /// Debug: every completed command (opcode, address, data).
    pub log: Vec<(u8, u8, u16)>,
    /// Debug: reads aborted by CS dropping before all 16 bits were clocked.
    pub aborted_reads: u32,
}

impl Nvram {
    pub fn new(words: [u16; 16]) -> Self {
        Self {
            words,
            write_enabled: false,
            cs: false,
            sk: false,
            do_bit: true,
            state: State::Idle,
            dirty: false,
            log: Vec::new(),
            aborted_reads: 0,
        }
    }

    /// Data-out pin. When idle with CS high it reads 1 (ready).
    pub fn data_out(&self) -> bool {
        self.do_bit
    }

    /// Update the input pins. Call on every write to the PPI.
    pub fn set_pins(&mut self, cs: bool, sk: bool, di: bool) {
        if !cs {
            if self.cs {
                if let State::Reading { remaining, .. } = self.state {
                    if remaining > 1 {
                        self.aborted_reads += 1;
                    }
                }
                self.state = State::Idle;
                self.do_bit = true;
            }
            self.cs = false;
            self.sk = sk;
            return;
        }
        self.cs = true;
        let rising = sk && !self.sk;
        self.sk = sk;
        if rising {
            self.clock(di);
        }
    }

    fn clock(&mut self, di: bool) {
        self.state = match self.state {
            State::Idle => {
                if di { State::Idle } else { State::Armed }
            }
            State::Armed => {
                if di { State::Command { bits: 0, count: 0 } } else { State::Armed }
            }
            State::Command { bits, count } => {
                let bits = (bits << 1) | di as u8;
                let count = count + 1;
                if count < 8 {
                    State::Command { bits, count }
                } else {
                    self.execute(bits)
                }
            }
            State::Reading { word, remaining } => {
                self.do_bit = word & 0x8000 != 0;
                if remaining > 1 {
                    State::Reading { word: word << 1, remaining: remaining - 1 }
                } else {
                    State::Done
                }
            }
            State::Writing { addr, data, count } => {
                let data = (data << 1) | di as u16;
                let count = count + 1;
                if count < 16 {
                    State::Writing { addr, data, count }
                } else {
                    if self.write_enabled {
                        self.words[addr as usize] = data;
                        self.dirty = true;
                    }
                    self.do_bit = true; // programming completes instantly
                    State::Done
                }
            }
            State::Done => State::Done,
        };
    }

    fn execute(&mut self, bits: u8) -> State {
        let op = bits >> 6;
        let addr = bits & 0x0F;
        if self.log.len() < 100_000 {
            self.log.push((op, addr, self.words[addr as usize]));
        }
        match op {
            0b10 => {
                // READ: a dummy 0 is presented first, then D15..D0.
                self.do_bit = false;
                State::Reading { word: self.words[addr as usize], remaining: 16 }
            }
            0b01 => State::Writing { addr, data: 0, count: 0 },
            0b11 => {
                if self.write_enabled {
                    self.words[addr as usize] = 0xFFFF;
                    self.dirty = true;
                }
                self.do_bit = true;
                State::Done
            }
            _ => {
                match (bits >> 4) & 3 {
                    0b11 => self.write_enabled = true,  // EWEN
                    0b00 => self.write_enabled = false, // EWDS
                    _ => {}                             // ERAL/WRAL: not used by the ROM
                }
                State::Done
            }
        }
    }
}

/// Known-good NVRAM images, reconstructed from the 4.x kernel's NVRAM
/// routines (docs/nvram_rtc_disk_install.md §1.5). Word 0 bits: 12-15 loader
/// disables (net, floppy, hd, rom), 11 ?, 8 no speech, 7 no keyboard,
/// 5 no RTC, 3 no 1797 FDC, 2 workstation, 0-1 extra 128 KB banks.
/// Words 1-4 serial number, 5-9 CRTC R0-R9, 10 node/boot server,
/// 11 palette, 12 boot-file selector, 13 monitor, 14-15 unused.
pub mod presets {
    /// ICON fileserver, node 1: floppy then hard disk, 512 KB.
    pub fn fileserver() -> [u16; 16] {
        [
            0x998A, 0x5173, 0x3C01, 0x5173, 0x3C01, 0x2838, 0x442C, 0x0041,
            0x3D3C, 0x0300, 0x0101, 0xF4F4, 0x0000, 0xFFFF, 0xFFFF, 0xFFFF,
        ]
    }

    /// ICON workstation, node 16, boots from server 1 over the network, 512 KB.
    pub fn workstation() -> [u16; 16] {
        [
            0xE82E, 0x5173, 0x3C02, 0x5173, 0x3C02, 0x2838, 0x442C, 0x0041,
            0x3D3C, 0x0300, 0x1001, 0xF4F4, 0x0001, 0xFFFF, 0xFFFF, 0xFFFF,
        ]
    }
}
