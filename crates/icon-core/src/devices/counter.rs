//! Microwire dual 16-bit up/down counter (COP452-type), used on the ICON
//! board for the trackball X and Y axes and the baud-rate generator.
//!
//! Protocol, from the ROM self-test (file 0x1742–0x1880) and the kernel
//! driver (docs/input_video.md §3.1):
//! - Each instruction is framed by a pulse on the enable line (PPI A port B
//!   bit 5 high then low); it is 6 bits MSB first with a leading 1, sampled
//!   on the rising edge of SK.
//! - 20h/21h load register A/B from the next 16 data bits.
//! - 22h/23h read register A/B: D15 is on DO as soon as the instruction is
//!   complete, and each further SK rising edge presents the next bit.
//! - 24h/25h counter A/B <- register A/B; 26h/27h register A/B <- counter.
//! - 3Fh resets; the remaining start-up/mode commands are accepted and ignored.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Command { bits: u8, count: u8 },
    Load { reg: usize, data: u16, count: u8 },
    Read { word: u16 },
    Done,
}

#[derive(Clone, Debug)]
pub struct MwCounter {
    pub register: [u16; 2],
    pub counter: [u16; 2],
    enabled: bool,
    sk: bool,
    do_bit: bool,
    state: State,
}

impl Default for MwCounter {
    fn default() -> Self {
        Self {
            register: [0; 2],
            counter: [0; 2],
            enabled: false,
            sk: false,
            do_bit: false,
            state: State::Command { bits: 0, count: 0 },
        }
    }
}

impl MwCounter {
    pub fn data_out(&self) -> bool {
        self.do_bit
    }

    /// Update the pins. `enabled` is true while this chip is selected and the
    /// active-low enable is low; a new instruction starts at each enable.
    pub fn set_pins(&mut self, enabled: bool, sk: bool, di: bool) {
        if enabled && !self.enabled {
            self.state = State::Command { bits: 0, count: 0 };
        }
        self.enabled = enabled;
        let rising = sk && !self.sk;
        self.sk = sk;
        if enabled && rising {
            self.clock(di);
        }
    }

    fn clock(&mut self, di: bool) {
        self.state = match self.state {
            State::Command { bits, count } => {
                if count == 0 && !di {
                    State::Command { bits: 0, count: 0 } // wait for the leading 1
                } else {
                    let bits = (bits << 1) | di as u8;
                    if count + 1 < 6 { State::Command { bits, count: count + 1 } } else { self.execute(bits) }
                }
            }
            State::Load { reg, data, count } => {
                let data = (data << 1) | di as u16;
                if count + 1 < 16 {
                    State::Load { reg, data, count: count + 1 }
                } else {
                    self.register[reg] = data;
                    State::Done
                }
            }
            State::Read { word } => {
                let word = word << 1;
                self.do_bit = word & 0x8000 != 0;
                State::Read { word }
            }
            State::Done => State::Done,
        };
    }

    fn execute(&mut self, cmd: u8) -> State {
        match cmd & 0x3F {
            0x20 | 0x21 => State::Load { reg: (cmd & 1) as usize, data: 0, count: 0 },
            0x22 | 0x23 => {
                let word = self.register[(cmd & 1) as usize];
                self.do_bit = word & 0x8000 != 0;
                State::Read { word }
            }
            0x24 | 0x25 => {
                let i = (cmd & 1) as usize;
                self.counter[i] = self.register[i];
                State::Done
            }
            0x26 | 0x27 => {
                let i = (cmd & 1) as usize;
                self.register[i] = self.counter[i];
                State::Done
            }
            0x3F => {
                *self = Self { enabled: self.enabled, sk: self.sk, ..Self::default() };
                State::Done
            }
            _ => State::Done,
        }
    }

    /// Apply trackball motion as the ICON2's emulator does: +d decrements
    /// counter B, -d decrements counter A, so (A - B) follows the position.
    pub fn motion(&mut self, d: i32) {
        if d > 0 {
            self.counter[1] = self.counter[1].wrapping_sub(d as u16);
        } else if d < 0 {
            self.counter[0] = self.counter[0].wrapping_sub((-d) as u16);
        }
    }
}
