//! HD46505 / 6845 CRT controller: register file plus a frame/sync clock.
//!
//! Only what the ICON needs so far: the registers (all readable, which the
//! ROM's r/w test relies on) and vertical sync timing derived from R4/R5/R7/R9.

#[derive(Clone, Debug, Default)]
pub struct Crtc {
    pub addr: u8,
    pub regs: [u8; 18],
}

impl Crtc {
    pub fn write_addr(&mut self, v: u8) {
        self.addr = v & 0x1F;
    }
    pub fn write_data(&mut self, v: u8) {
        if let Some(r) = self.regs.get_mut(self.addr as usize) {
            *r = v;
        }
    }
    pub fn read_data(&self) -> u8 {
        self.regs.get(self.addr as usize).copied().unwrap_or(0)
    }

    /// Display start address (R12/R13).
    pub fn start_address(&self) -> u16 {
        ((self.regs[12] as u16 & 0x3F) << 8) | self.regs[13] as u16
    }

    /// Scan lines per frame, or a sane default when unprogrammed.
    pub fn lines_per_frame(&self) -> u32 {
        let rows = self.regs[4] as u32 + 1;
        let lines = rows * (self.regs[9] as u32 + 1) + self.regs[5] as u32;
        if lines < 100 { 262 } else { lines }
    }
}
