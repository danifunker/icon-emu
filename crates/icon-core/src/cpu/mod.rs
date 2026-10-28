//! Intel 80186 CPU core.
//!
//! The instruction core and on-chip peripherals (interrupt controller,
//! timers, DMA, chip-select registers) are a Rust port of MAME's i86/i186
//! cores (BSD-3-Clause, copyright-holders: Carl; peripheral code by Phill
//! Harvey-Smith, Aaron Giles, Paul Leaman). See THIRD_PARTY.md.

mod i86;
mod i186;

pub use i186::{ChipSelects, Regs, I80186};

/// Everything outside the CPU package: memory and the I/O space.
///
/// The 80186 peripheral control block (default I/O FF00–FFFF) is handled
/// inside [`I80186`] and never reaches the bus.
pub trait CpuBus {
    fn mem_read8(&mut self, addr: u32) -> u8;
    fn mem_write8(&mut self, addr: u32, val: u8);
    fn io_read8(&mut self, port: u16) -> u8;
    fn io_write8(&mut self, port: u16, val: u8);

    fn mem_read16(&mut self, addr: u32) -> u16 {
        let lo = self.mem_read8(addr & 0xF_FFFF) as u16;
        let hi = self.mem_read8(addr.wrapping_add(1) & 0xF_FFFF) as u16;
        lo | (hi << 8)
    }
    fn mem_write16(&mut self, addr: u32, val: u16) {
        self.mem_write8(addr & 0xF_FFFF, val as u8);
        self.mem_write8(addr.wrapping_add(1) & 0xF_FFFF, (val >> 8) as u8);
    }
    fn io_read16(&mut self, port: u16) -> u16 {
        self.io_read8(port) as u16 | ((self.io_read8(port.wrapping_add(1)) as u16) << 8)
    }
    fn io_write16(&mut self, port: u16, val: u16) {
        self.io_write8(port, val as u8);
        self.io_write8(port.wrapping_add(1), (val >> 8) as u8);
    }

    /// Interrupt-acknowledge cycle for INT0/INT1 in cascade mode. The ICON
    /// does not use cascade mode, so the default is fine.
    fn inta(&mut self, _line: u8) -> u8 {
        0
    }
}
