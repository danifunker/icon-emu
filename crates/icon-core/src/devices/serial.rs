//! Intel 8274 / NEC uPD7201 dual serial controller (Z80-SIO register model),
//! minimal: register pointer, status, and the reset/latch behaviour the ROM's
//! channel test checks (file 0x19C0–0x1A90). No data is transmitted yet.

const RR0_RX_AVAILABLE: u8 = 0x01;
const RR0_TX_EMPTY: u8 = 0x04;
const RR0_TX_UNDERRUN: u8 = 0x40;

#[derive(Clone, Debug, Default)]
pub struct Channel {
    pointer: u8,
    pub wr: [u8; 8],
    rr0: u8,
    rx: Option<u8>,
}

impl Channel {
    fn reset(&mut self) {
        *self = Channel { rr0: RR0_TX_EMPTY | RR0_TX_UNDERRUN, ..Default::default() };
    }

    pub fn write_control(&mut self, v: u8) {
        let reg = self.pointer;
        self.pointer = 0;
        if reg != 0 {
            self.wr[reg as usize] = v;
            return;
        }
        self.wr[0] = v;
        self.pointer = v & 7;
        if (v >> 3) & 7 == 3 {
            self.reset(); // channel reset
        }
        if v >> 6 == 3 {
            self.rr0 &= !RR0_TX_UNDERRUN; // reset Tx underrun/EOM latch
        }
    }

    pub fn read_control(&mut self) -> u8 {
        let reg = self.pointer;
        self.pointer = 0;
        match reg {
            0 => self.rr0 | if self.rx.is_some() { RR0_RX_AVAILABLE } else { 0 },
            1 => 0x01, // all sent
            2 => self.wr[2],
            _ => 0,
        }
    }

    pub fn write_data(&mut self, _v: u8) {}

    pub fn read_data(&mut self) -> u8 {
        self.rx.take().unwrap_or(0)
    }
}

#[derive(Clone, Debug)]
pub struct Sio8274 {
    pub ch: [Channel; 2],
}

impl Default for Sio8274 {
    fn default() -> Self {
        let mut s = Self { ch: Default::default() };
        s.ch[0].reset();
        s.ch[1].reset();
        s
    }
}
