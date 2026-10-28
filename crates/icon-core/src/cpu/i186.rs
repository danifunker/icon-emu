// Intel 80186: CPU wrapper and on-chip peripheral control block (interrupt
// controller, timers, DMA, chip-select registers, relocation register).
//
// Rust port of MAME's src/devices/cpu/i86/i186.cpp.
// Original code: license BSD-3-Clause, copyright-holders: Carl.
// Peripheral code from the rmnimbus driver by Phill Harvey-Smith, which is
// based on the Leland sound driver by Aaron Giles and Paul Leaman.
// See THIRD_PARTY.md at the repository root.
//
// Differences from MAME's structure: MAME drives the timers from emu_timer
// callbacks; here every timer is a counter clocked from the instruction
// stream (one count per 4 CPU clocks, as on real hardware). The iRMX (slave)
// interrupt-controller mode is not implemented; only master mode is.

use super::i86::{Core, Host, AX, BP, BX, CS, CX, DI, DS, DX, ES, SI, SP, SS};
use super::CpuBus;

// External interrupt control register bits
const EXTINT_CTRL_LTM: u16 = 0x10;
const EXTINT_CTRL_CASCADE: u16 = 0x20;
const EXTINT_CTRL_SFNM: u16 = 0x40;

// DMA control register bits
const DEST_MIO: u16 = 0x8000;
const DEST_DECREMENT: u16 = 0x4000;
const DEST_INCREMENT: u16 = 0x2000;
const DEST_INCDEC_MASK: u16 = DEST_DECREMENT | DEST_INCREMENT;
const SRC_MIO: u16 = 0x1000;
const SRC_DECREMENT: u16 = 0x0800;
const SRC_INCREMENT: u16 = 0x0400;
const SRC_INCDEC_MASK: u16 = SRC_DECREMENT | SRC_INCREMENT;
const TERMINATE_ON_ZERO: u16 = 0x0200;
const INTERRUPT_ON_ZERO: u16 = 0x0100;
const SYNC_MASK: u16 = 0x00c0;
const CHANNEL_PRIORITY: u16 = 0x0020;
const TIMER_DRQ: u16 = 0x0010;
const CHG_NOCHG: u16 = 0x0004;
const ST_STOP: u16 = 0x0002;
const BYTE_WORD: u16 = 0x0001;

/// Relocation register bits
const RELREG_ESC_TRAP: u16 = 0x8000;
const RELREG_MEM: u16 = 0x1000;

/// Clocks charged for one DMA transfer (two bus cycles).
const DMA_TRANSFER_CLOCKS: u32 = 8;
/// Clocks that pass per `step` while the CPU is halted.
const HALT_CLOCKS: u32 = 4;
/// Upper bound on clocks a REP string instruction runs per `step` before it
/// is suspended and resumed on the next call (MAME's icount timeslice).
const REP_BUDGET: i32 = 256;

/// Chip-select control registers (values as read back by software).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChipSelects {
    pub umcs: u16,
    pub lmcs: u16,
    pub pacs: u16,
    pub mmcs: u16,
    pub mpcs: u16,
}

/// Programmer-visible registers, for debuggers and tracing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Regs {
    pub ax: u16,
    pub bx: u16,
    pub cx: u16,
    pub dx: u16,
    pub si: u16,
    pub di: u16,
    pub bp: u16,
    pub sp: u16,
    pub cs: u16,
    pub ds: u16,
    pub es: u16,
    pub ss: u16,
    pub ip: u16,
    pub flags: u16,
}

#[derive(Clone, Copy, Debug, Default)]
struct TimerState {
    control: u16,
    max_a: u16,
    max_b: u16,
    count: u16,
    /// Equivalent of MAME's int_timer being enabled: the timer is counting
    /// the internal clock.
    armed: bool,
    tmrin: bool,
    out: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct DmaState {
    drq_state: bool,
    source: u32,
    dest: u32,
    count: u16,
    control: u16,
}

#[derive(Clone, Copy, Debug, Default)]
struct IntrState {
    vector: u8,
    pending: bool,
    ack_mask: u16,
    priority_mask: u16,
    in_service: u16,
    request: u16,
    status: u16,
    poll_status: u16,
    timer: u16,
    dma: [u16; 2],
    ext: [u16; 4],
    ext_state: u8,
}

/// Peripheral control block state.
#[derive(Clone, Debug, Default)]
struct Pcb {
    timer: [TimerState; 3],
    dma: [DmaState; 2],
    intr: IntrState,
    mem: ChipSelects,
    last_dma: bool,
    dma_latency: i32,
    reloc: u16,
    /// CPU clocks not yet converted into timer counts (clock / 4).
    clk_frac: u32,
}

impl Pcb {
    fn reset(&mut self) {
        self.intr = IntrState {
            priority_mask: 0x0007,
            timer: 0x000f,
            dma: [0x000f; 2],
            ext: [0x000f; 4],
            ..IntrState::default()
        };
        self.reloc = 0x20ff;
        self.mem.umcs = 0xfffb;
        for d in &mut self.dma {
            d.drq_state = false;
            d.control = 0;
        }
        for t in &mut self.timer {
            t.control = 0;
            t.max_a = 0;
            t.max_b = 0;
            t.count = 0;
            t.armed = false;
            t.out = true;
        }
        self.dma_latency = 0;
        self.clk_frac = 0;
    }

    #[inline]
    fn io_hit(&self, port: u16) -> bool {
        self.reloc & RELREG_MEM == 0 && (port >> 8) == (self.reloc & 0xff)
    }

    #[inline]
    fn mem_hit(&self, addr: u32) -> bool {
        self.reloc & RELREG_MEM != 0 && ((addr >> 8) & 0xfff) == (self.reloc & 0xfff) as u32
    }

    // -----------------------------------------------------------------------
    // Interrupt controller (master mode)
    // -----------------------------------------------------------------------

    /// INTA cycle / poll register read: returns the vector.
    fn int_callback<B: CpuBus>(&mut self, bus: &mut B) -> u8 {
        self.intr.pending = false;

        if self.intr.ack_mask & 0xf0 != 0 {
            let mut i = 0;
            while i < 4 {
                if self.intr.ack_mask & (1 << (i + 4)) != 0 {
                    break;
                }
                i += 1;
            }
            if i < 4 && self.intr.ext[i] & EXTINT_CTRL_LTM == 0 {
                self.intr.request &= !self.intr.ack_mask;
            }
        } else {
            self.intr.request &= !self.intr.ack_mask;
        }

        self.intr.in_service |= self.intr.ack_mask;

        if self.intr.ack_mask == 0x0001 {
            match self.intr.poll_status & 0x1f {
                0x08 => self.intr.status &= !0x01,
                0x12 => self.intr.status &= !0x02,
                0x13 => self.intr.status &= !0x04,
                _ => {}
            }
        }

        let vector = match self.intr.poll_status & 0x1f {
            0x0c if self.intr.ext[0] & EXTINT_CTRL_CASCADE != 0 => bus.inta(0),
            0x0d if self.intr.ext[1] & EXTINT_CTRL_CASCADE != 0 => bus.inta(1),
            v => v as u8,
        };
        self.intr.ack_mask = 0;
        self.intr.poll_status &= !0x8000;
        vector
    }

    fn update_interrupt_state(&mut self) {
        let mut priority: u16 = 0;
        while priority <= self.intr.priority_mask {
            // note: by checking 4 bits, we also verify that the mask is off
            if self.intr.timer & 0x0f == priority {
                // if we're already servicing something at this level, don't generate anything new
                if self.intr.in_service & 0x01 != 0 {
                    return;
                }
                if self.intr.status & 0x07 != 0 {
                    let v = if self.intr.status & 1 != 0 {
                        0x08
                    } else if self.intr.status & 2 != 0 {
                        0x12
                    } else {
                        0x13
                    };
                    self.intr.ack_mask = 0x01;
                    self.generate_int(v);
                    return;
                }
            }

            for int_num in 0..2 {
                if self.intr.dma[int_num] & 0x0f == priority {
                    let irq = 0x04 << int_num;
                    if self.intr.in_service & irq != 0 {
                        return;
                    }
                    if self.intr.request & irq != 0 {
                        self.intr.ack_mask = irq;
                        self.generate_int(0x0a + int_num as u16);
                        return;
                    }
                }
            }

            for int_num in 0..4 {
                if self.intr.ext[int_num] & 0x0f == priority {
                    let bit = 0x10 << int_num;
                    let sfnm = self.intr.ext[int_num] & EXTINT_CTRL_SFNM != 0;
                    if self.intr.in_service & bit != 0 && !sfnm {
                        return;
                    }
                    if self.intr.request & bit != 0 {
                        if int_num >= 2 && self.intr.ext[int_num - 2] & EXTINT_CTRL_CASCADE != 0 {
                            // INT2/INT3 are INTA0/INTA1 in cascade mode
                            self.intr.request &= !bit;
                            continue;
                        }
                        self.intr.ack_mask = bit;
                        self.generate_int(0x0c + int_num as u16);
                        return;
                    } else if self.intr.in_service & bit != 0 && sfnm {
                        return;
                    }
                }
            }
            priority += 1;
        }
        self.intr.pending = false;
    }

    fn generate_int(&mut self, vector: u16) {
        self.intr.poll_status = 0x8000 | vector;
        self.intr.pending = true;
    }

    fn handle_eoi(&mut self, data: u16) {
        if data & 0x8000 == 0 {
            // specific EOI
            match data & 0x1f {
                0x08 | 0x12 | 0x13 => self.intr.in_service &= !0x01,
                0x0a => self.intr.in_service &= !0x04,
                0x0b => self.intr.in_service &= !0x08,
                0x0c => self.intr.in_service &= !0x10,
                0x0d => self.intr.in_service &= !0x20,
                0x0e => self.intr.in_service &= !0x40,
                0x0f => self.intr.in_service &= !0x80,
                _ => {}
            }
        } else {
            // non-specific EOI: clear the highest-priority in-service bit
            let mut handled = false;
            let mut priority = 0;
            while priority <= 7 && !handled {
                if self.intr.timer & 0x07 == priority && self.intr.in_service & 0x01 != 0 {
                    self.intr.in_service &= !0x01;
                    handled = true;
                }
                for int_num in 0..2 {
                    if handled {
                        break;
                    }
                    if self.intr.dma[int_num] & 0x07 == priority
                        && self.intr.in_service & (0x04 << int_num) != 0
                    {
                        self.intr.in_service &= !(0x04 << int_num);
                        handled = true;
                    }
                }
                for int_num in 0..4 {
                    if handled {
                        break;
                    }
                    if self.intr.ext[int_num] & 0x07 == priority
                        && self.intr.in_service & (0x10 << int_num) != 0
                    {
                        self.intr.in_service &= !(0x10 << int_num);
                        handled = true;
                    }
                }
                priority += 1;
            }
        }
        self.update_interrupt_state();
    }

    fn external_int(&mut self, int_num: usize, state: bool) {
        let cur = self.intr.ext_state & (1 << int_num) != 0;
        if cur == state {
            return;
        }
        if !state {
            self.intr.request &= !(0x10 << int_num);
            self.intr.ack_mask &= !(0x10 << int_num);
            self.intr.ext_state &= !(1 << int_num);
        } else {
            self.intr.request |= 0x10 << int_num;
            self.intr.ext_state |= 1 << int_num;
        }
        self.update_interrupt_state();
    }

    // -----------------------------------------------------------------------
    // Timers
    // -----------------------------------------------------------------------

    /// Is this timer clocked by the internal (CPU/4) clock?
    #[inline]
    fn internally_clocked(which: usize, control: u16) -> bool {
        control & 0x0004 == 0 && !(which != 2 && control & 0x0008 != 0)
    }

    fn timer_elapsed<B: CpuBus>(&mut self, bus: &mut B, which: usize) {
        // set the max count bit
        self.timer[which].control |= 0x0020;

        // request an interrupt
        if self.timer[which].control & 0x2000 != 0 {
            self.intr.status |= 0x01 << which;
            self.update_interrupt_state();
        }

        if which == 2 {
            if self.dma_latency == 0 {
                for ch in 0..2 {
                    if self.dma[ch].control & (TIMER_DRQ | ST_STOP) == (TIMER_DRQ | ST_STOP) {
                        self.drq_callback(bus, ch);
                    }
                }
            }
            for t in 0..2 {
                if self.timer[t].control & 0x800c == 0x8008 {
                    self.inc_timer(bus, t);
                }
            }
        } else {
            let c = self.timer[which].control;
            self.timer[which].out = if c & 2 == 0 { true } else { c & 0x1000 != 0 };
        }

        let t = &mut self.timer[which];
        // if we're continuous or altcounting, reset
        if t.control & 1 != 0 || (t.control & 2 != 0 && t.control & 0x1000 == 0) {
            if t.control & 2 != 0 && t.control & 0x1000 == 0 {
                t.control |= 0x1000;
            } else {
                t.control &= !0x1000;
            }
            self.restart_timer(which);
        } else {
            t.armed = false;
            t.control &= !0x9000;
        }
        self.timer[which].count = 0;
    }

    fn restart_timer(&mut self, which: usize) {
        let t = &mut self.timer[which];
        // Only run timer 0,1 when not incremented via timer 2 pre-scaler
        if which != 2 && t.control & 0x800c == 0x8008 {
            return;
        }
        if t.control & 4 == 0 {
            t.armed = true;
        }
    }

    fn inc_timer<B: CpuBus>(&mut self, bus: &mut B, which: usize) {
        let t = &mut self.timer[which];
        t.count = t.count.wrapping_add(1);
        let max = if t.control & 2 != 0 && t.control & 0x1000 != 0 {
            t.max_b
        } else {
            t.max_a
        };
        // A max count of 0 means 65536.
        if t.count == max {
            self.timer_elapsed(bus, which);
        }
    }

    /// Advance the timers by `cycles` CPU clocks.
    fn clock_timers<B: CpuBus>(&mut self, bus: &mut B, cycles: u32) {
        self.clk_frac += cycles;
        while self.clk_frac >= 4 {
            self.clk_frac -= 4;
            for which in 0..3 {
                let t = &self.timer[which];
                if t.armed && t.control & 0x8000 != 0 && Self::internally_clocked(which, t.control)
                {
                    self.inc_timer(bus, which);
                }
            }
        }
    }

    fn internal_timer_update(
        &mut self,
        which: usize,
        new_count: Option<u16>,
        new_max_a: Option<u16>,
        new_max_b: Option<u16>,
        new_control: Option<u16>,
    ) {
        let mut update_int_timer = false;
        let t = &mut self.timer[which];

        if let Some(c) = new_count {
            if t.control & 0x8000 != 0 {
                update_int_timer = true;
            }
            t.count = c;
        }
        if let Some(m) = new_max_a {
            if m != t.max_a {
                if t.control & 0x8000 != 0 {
                    update_int_timer = true;
                }
                t.max_a = m;
            }
        }
        if let Some(m) = new_max_b {
            if m != t.max_b {
                if t.control & 0x8000 != 0 {
                    update_int_timer = true;
                }
                t.max_b = m;
            }
        }
        if let Some(nc) = new_control {
            let resbits: u16 = if which == 2 { 0x1fde } else { 0x1fc0 };
            // merge back in the bits we don't modify
            let mut nc = (nc & !resbits) | (t.control & resbits);
            // handle the /INH bit
            if nc & 0x4000 == 0 {
                nc = (nc & !0x8000) | (t.control & 0x8000);
            }
            nc &= !0x4000;

            let diff = nc ^ t.control;
            if diff != 0 {
                if diff & 0x8000 != 0 && nc & 0x8000 == 0 {
                    // going off
                    update_int_timer = true;
                } else if diff & 0x8000 != 0 && nc & 0x8000 != 0 && nc & 0xc == 0 {
                    // going on with internal clock
                    update_int_timer = true;
                }
                if diff & 0x8000 == 0 && diff & 0x2000 != 0 {
                    update_int_timer = true;
                }
                // a change of clock source must re-evaluate arming
                if diff & 0x001c != 0 {
                    update_int_timer = true;
                }
            }
            // RIU is cleared whenever ALT = 0
            if nc & 0x0002 == 0 {
                nc &= !0x1000;
            }
            t.control = nc;
        }

        if update_int_timer {
            let c = t.control;
            t.armed = c & 0x8000 != 0
                && Self::internally_clocked(which, c)
                && (c & 0x10 == 0 || t.armed);
        }
    }

    fn external_tmrin<B: CpuBus>(&mut self, bus: &mut B, which: usize, state: bool) {
        let rising = state && !self.timer[which].tmrin;
        self.timer[which].tmrin = state;
        if !rising {
            return;
        }
        let c = self.timer[which].control;
        if c & 0x8004 == 0x8004 {
            self.inc_timer(bus, which);
        } else if c & 0x8014 == 0x8010 {
            // retrigger
            self.timer[which].count = 0;
            self.restart_timer(which);
        }
    }

    // -----------------------------------------------------------------------
    // DMA
    // -----------------------------------------------------------------------

    fn update_dma_control(&mut self, which: usize, new_control: u16) {
        let d = &mut self.dma[which];
        let mut nc = new_control;
        // handle the CHG bit
        if nc & CHG_NOCHG == 0 {
            nc = (nc & !ST_STOP) | (d.control & ST_STOP);
        }
        nc &= !CHG_NOCHG;
        d.control = nc;
    }

    fn drq_callback<B: CpuBus>(&mut self, bus: &mut B, which: usize) {
        let d = self.dma[which];
        if d.control & ST_STOP == 0 {
            return;
        }
        let dest_mem = d.control & DEST_MIO != 0;
        let src_mem = d.control & SRC_MIO != 0;
        let incdec: u32 = if d.control & BYTE_WORD != 0 {
            let w = if src_mem {
                bus.mem_read16(d.source & 0xfffff)
            } else {
                bus.io_read16(d.source as u16)
            };
            if dest_mem {
                bus.mem_write16(d.dest & 0xfffff, w);
            } else {
                bus.io_write16(d.dest as u16, w);
            }
            2
        } else {
            let b = if src_mem {
                bus.mem_read8(d.source & 0xfffff)
            } else {
                bus.io_read8(d.source as u16)
            };
            if dest_mem {
                bus.mem_write8(d.dest & 0xfffff, b);
            } else {
                bus.io_write8(d.dest as u16, b);
            }
            1
        };

        let d = &mut self.dma[which];
        match d.control & DEST_INCDEC_MASK {
            DEST_DECREMENT => d.dest = d.dest.wrapping_sub(incdec) & 0xfffff,
            DEST_INCREMENT => d.dest = d.dest.wrapping_add(incdec) & 0xfffff,
            _ => {}
        }
        match d.control & SRC_INCDEC_MASK {
            SRC_DECREMENT => d.source = d.source.wrapping_sub(incdec) & 0xfffff,
            SRC_INCREMENT => d.source = d.source.wrapping_add(incdec) & 0xfffff,
            _ => {}
        }
        d.count = d.count.wrapping_sub(1);

        // Terminate if count is zero, and terminate flag set
        if (d.control & TERMINATE_ON_ZERO != 0 || d.control & SYNC_MASK == 0) && d.count == 0 {
            d.control &= !ST_STOP;
        }
        // Interrupt if count is zero, and interrupt flag set
        if d.control & INTERRUPT_ON_ZERO != 0 && d.count == 0 {
            self.intr.request |= 0x04 << which;
            self.update_interrupt_state();
        }
    }

    /// Channel to service now, if any (MAME's arbitration in execute_run).
    fn dma_pending(&mut self) -> Option<usize> {
        let r0 = self.dma[0].drq_state && self.dma[0].control & ST_STOP != 0;
        let r1 = self.dma[1].drq_state && self.dma[1].control & ST_STOP != 0;
        if !(r0 || r1) || self.dma_latency != 0 {
            return None;
        }
        let mut channel = if self.last_dma { 0 } else { 1 };
        self.last_dma = !self.last_dma;
        let p0 = self.dma[0].control & CHANNEL_PRIORITY != 0;
        let p1 = self.dma[1].control & CHANNEL_PRIORITY != 0;
        if !r1 {
            channel = 0;
        } else if !r0 {
            channel = 1;
        } else if p0 && !p1 {
            channel = 0;
        } else if p1 && !p0 {
            channel = 1;
        }
        Some(channel)
    }

    // -----------------------------------------------------------------------
    // Register file
    // -----------------------------------------------------------------------

    fn port_r<B: CpuBus>(&mut self, bus: &mut B, offset: u16) -> u16 {
        match offset {
            0x10 => self.intr.vector as u16,
            0x12 => {
                if self.intr.poll_status & 0x8000 != 0 {
                    self.int_callback(bus);
                }
                self.intr.poll_status
            }
            0x13 => self.intr.poll_status,
            0x14 => {
                let mut temp = (self.intr.timer >> 3) & 0x01;
                temp |= (self.intr.dma[0] >> 1) & 0x04;
                temp |= self.intr.dma[1] & 0x08;
                temp |= (self.intr.ext[0] << 1) & 0x10;
                temp |= (self.intr.ext[1] << 2) & 0x20;
                temp |= (self.intr.ext[2] << 3) & 0x40;
                temp |= (self.intr.ext[3] << 4) & 0x80;
                temp
            }
            0x15 => self.intr.priority_mask,
            0x16 => self.intr.in_service,
            0x17 => {
                let mut temp = self.intr.request & !0x0001;
                if self.intr.status & 0x0007 != 0 {
                    temp |= 1;
                }
                temp
            }
            0x18 => self.intr.status,
            0x19 => self.intr.timer,
            0x1a => self.intr.dma[0],
            0x1b => self.intr.dma[1],
            0x1c..=0x1f => self.intr.ext[(offset - 0x1c) as usize],
            0x28 | 0x2c | 0x30 => self.timer[((offset - 0x28) / 4) as usize].count,
            0x29 | 0x2d | 0x31 => self.timer[((offset - 0x29) / 4) as usize].max_a,
            0x2a | 0x2e => self.timer[((offset - 0x2a) / 4) as usize].max_b,
            0x2b | 0x2f | 0x33 => self.timer[((offset - 0x2b) / 4) as usize].control,
            0x50 => self.mem.umcs,
            0x51 => self.mem.lmcs,
            0x52 => self.mem.pacs,
            0x53 => self.mem.mmcs,
            0x54 => self.mem.mpcs,
            0x60 | 0x68 => self.dma[((offset - 0x60) / 8) as usize].source as u16,
            0x61 | 0x69 => (self.dma[((offset - 0x61) / 8) as usize].source >> 16) as u16,
            0x62 | 0x6a => self.dma[((offset - 0x62) / 8) as usize].dest as u16,
            0x63 | 0x6b => (self.dma[((offset - 0x63) / 8) as usize].dest >> 16) as u16,
            0x64 | 0x6c => self.dma[((offset - 0x64) / 8) as usize].count,
            0x65 | 0x6d => self.dma[((offset - 0x65) / 8) as usize].control,
            0x7f => self.reloc,
            _ => 0,
        }
    }

    fn port_w<B: CpuBus>(&mut self, bus: &mut B, offset: u16, data: u16) {
        match offset {
            0x10 => self.intr.vector = (data & 0xf8) as u8,
            0x11 => {
                self.handle_eoi(data);
                self.update_interrupt_state();
            }
            0x14 => {
                self.intr.timer = (self.intr.timer & !0x08) | ((data << 3) & 0x08);
                self.intr.dma[0] = (self.intr.dma[0] & !0x08) | ((data << 1) & 0x08);
                self.intr.dma[1] = (self.intr.dma[1] & !0x08) | (data & 0x08);
                self.intr.ext[0] = (self.intr.ext[0] & !0x08) | ((data >> 1) & 0x08);
                self.intr.ext[1] = (self.intr.ext[1] & !0x08) | ((data >> 2) & 0x08);
                self.intr.ext[2] = (self.intr.ext[2] & !0x08) | ((data >> 3) & 0x08);
                self.intr.ext[3] = (self.intr.ext[3] & !0x08) | ((data >> 4) & 0x08);
                self.update_interrupt_state();
            }
            0x15 => {
                self.intr.priority_mask = data & 0x0007;
                self.update_interrupt_state();
            }
            0x16 => {
                self.intr.in_service = data & 0x00ff;
                self.update_interrupt_state();
            }
            0x17 => {
                self.intr.request = (self.intr.request & !0x000c) | (data & 0x000c);
                self.update_interrupt_state();
            }
            0x18 => {
                self.intr.status = (self.intr.status & !0x8007) | (data & 0x8007);
                self.update_interrupt_state();
            }
            0x19 => {
                self.intr.timer = data & 0x000f;
                self.update_interrupt_state();
            }
            0x1a | 0x1b => {
                self.intr.dma[(offset - 0x1a) as usize] = data & 0x000f;
                self.update_interrupt_state();
            }
            0x1c | 0x1d => {
                self.intr.ext[(offset - 0x1c) as usize] = data & 0x007f;
                self.update_interrupt_state();
            }
            0x1e | 0x1f => {
                self.intr.ext[(offset - 0x1c) as usize] = data & 0x001f;
                self.update_interrupt_state();
            }
            0x28 | 0x2c | 0x30 => {
                self.internal_timer_update(((offset - 0x28) / 4) as usize, Some(data), None, None, None)
            }
            0x29 | 0x2d | 0x31 => {
                self.internal_timer_update(((offset - 0x29) / 4) as usize, None, Some(data), None, None)
            }
            0x2a | 0x2e => {
                self.internal_timer_update(((offset - 0x2a) / 4) as usize, None, None, Some(data), None)
            }
            0x2b | 0x2f | 0x33 => {
                self.internal_timer_update(((offset - 0x2b) / 4) as usize, None, None, None, Some(data))
            }
            0x50 => self.mem.umcs = data | 0xc038,
            0x51 => self.mem.lmcs = (data & 0x3fff) | 0x0038,
            0x52 => self.mem.pacs = data | 0x0038,
            0x53 => self.mem.mmcs = data | 0x01f8,
            0x54 => self.mem.mpcs = data | 0x8038,
            0x60 | 0x68 => {
                let d = &mut self.dma[((offset - 0x60) / 8) as usize];
                d.source = (d.source & !0x0ffff) | data as u32;
                self.dma_latency = 8;
            }
            0x61 | 0x69 => {
                let d = &mut self.dma[((offset - 0x61) / 8) as usize];
                d.source = (d.source & !0xf0000) | (((data as u32) << 16) & 0xf0000);
                self.dma_latency = 8;
            }
            0x62 | 0x6a => {
                let d = &mut self.dma[((offset - 0x62) / 8) as usize];
                d.dest = (d.dest & !0x0ffff) | data as u32;
                self.dma_latency = 8;
            }
            0x63 | 0x6b => {
                let d = &mut self.dma[((offset - 0x63) / 8) as usize];
                d.dest = (d.dest & !0xf0000) | (((data as u32) << 16) & 0xf0000);
                self.dma_latency = 8;
            }
            0x64 | 0x6c => {
                self.dma[((offset - 0x64) / 8) as usize].count = data;
                self.dma_latency = 8;
            }
            0x65 | 0x6d => {
                let which = ((offset - 0x65) / 8) as usize;
                self.update_dma_control(which, data);
                if self.dma[which].control & (SYNC_MASK | ST_STOP | TIMER_DRQ) == ST_STOP {
                    // Unsynchronised transfer: MAME runs it to completion at once.
                    while self.dma[which].control & ST_STOP != 0 {
                        self.drq_callback(bus, which);
                    }
                }
            }
            0x7f => self.reloc = data,
            _ => {}
        }
    }
}

/// Glue between the instruction core and the outside world: routes
/// peripheral-control-block accesses to the PCB, everything else to the bus.
struct Sys<'a, B: CpuBus> {
    bus: &'a mut B,
    pcb: &'a mut Pcb,
}

impl<B: CpuBus> Host for Sys<'_, B> {
    #[inline]
    fn fetch(&mut self, addr: u32) -> u8 {
        self.bus.mem_read8(addr)
    }

    #[inline]
    fn read_byte(&mut self, addr: u32) -> u8 {
        if self.pcb.mem_hit(addr) {
            let v = self.pcb.port_r(self.bus, ((addr >> 1) & 0x7f) as u16);
            return if addr & 1 != 0 { (v >> 8) as u8 } else { v as u8 };
        }
        self.bus.mem_read8(addr)
    }

    #[inline]
    fn read_word(&mut self, addr: u32) -> u16 {
        if self.pcb.mem_hit(addr) {
            // Unaligned reads from the internal bus are swapped rather than split
            let v = self.pcb.port_r(self.bus, ((addr >> 1) & 0x7f) as u16);
            return if addr & 1 != 0 { v.swap_bytes() } else { v };
        }
        self.bus.mem_read16(addr)
    }

    #[inline]
    fn write_byte(&mut self, addr: u32, data: u8) {
        if self.pcb.mem_hit(addr) {
            let v = if addr & 1 != 0 { (data as u16) << 8 } else { data as u16 };
            self.pcb.port_w(self.bus, ((addr >> 1) & 0x7f) as u16, v);
            return;
        }
        self.bus.mem_write8(addr, data)
    }

    #[inline]
    fn write_word(&mut self, addr: u32, data: u16) {
        if self.pcb.mem_hit(addr) {
            let v = if addr & 1 != 0 { data.swap_bytes() } else { data };
            self.pcb.port_w(self.bus, ((addr >> 1) & 0x7f) as u16, v);
            return;
        }
        self.bus.mem_write16(addr, data)
    }

    fn read_port_byte(&mut self, port: u16) -> u8 {
        if self.pcb.io_hit(port) {
            let v = self.pcb.port_r(self.bus, (port >> 1) & 0x7f);
            return if port & 1 != 0 { (v >> 8) as u8 } else { v as u8 };
        }
        self.bus.io_read8(port)
    }

    fn read_port_word(&mut self, port: u16) -> u16 {
        if self.pcb.io_hit(port) {
            let v = self.pcb.port_r(self.bus, (port >> 1) & 0x7f);
            return if port & 1 != 0 { v.swap_bytes() } else { v };
        }
        self.bus.io_read16(port)
    }

    fn write_port_byte(&mut self, port: u16, data: u8) {
        if self.pcb.io_hit(port) {
            let v = if port & 1 != 0 { (data as u16) << 8 } else { data as u16 };
            self.pcb.port_w(self.bus, (port >> 1) & 0x7f, v);
        } else {
            self.bus.io_write8(port, data)
        }
    }

    fn write_port_byte_al(&mut self, port: u16, ax: u16) {
        if self.pcb.io_hit(port) {
            // Both AH and AL are written onto the internal bus
            let v = if port & 1 != 0 { ax.swap_bytes() } else { ax };
            self.pcb.port_w(self.bus, (port >> 1) & 0x7f, v);
        } else {
            self.bus.io_write8(port, ax as u8)
        }
    }

    fn write_port_word(&mut self, port: u16, data: u16) {
        if self.pcb.io_hit(port) {
            let v = if port & 1 != 0 { data.swap_bytes() } else { data };
            self.pcb.port_w(self.bus, (port >> 1) & 0x7f, v);
        } else {
            self.bus.io_write16(port, data)
        }
    }

    #[inline]
    fn intr(&self) -> bool {
        self.pcb.intr.pending
    }

    fn irq_ack(&mut self) -> u8 {
        self.pcb.int_callback(self.bus)
    }

    #[inline]
    fn esc_trap(&self) -> bool {
        self.pcb.reloc & RELREG_ESC_TRAP != 0
    }
}

/// Intel 80186 CPU with its integrated peripherals.
#[derive(Clone, Debug)]
pub struct I80186 {
    core: Core,
    pcb: Pcb,
}

impl Default for I80186 {
    fn default() -> Self {
        Self::new()
    }
}

impl I80186 {
    pub fn new() -> Self {
        let mut c = I80186 { core: Core::new(), pcb: Pcb::default() };
        c.reset();
        c
    }

    /// Hardware reset: CS=FFFF IP=0000, flags cleared, PCB at I/O FF00,
    /// UMCS=FFFB, timers/DMA stopped, interrupt sources masked.
    pub fn reset(&mut self) {
        self.core.reset();
        self.pcb.reset();
    }

    /// Execute one instruction (with its prefixes), service one interrupt,
    /// or perform one DMA transfer. Returns the CPU clocks used; the timers
    /// are advanced by the same amount. While halted, returns a small idle
    /// slice so time keeps moving.
    pub fn step<B: CpuBus>(&mut self, bus: &mut B) -> u32 {
        if let Some(ch) = self.pcb.dma_pending() {
            self.pcb.drq_callback(bus, ch);
            self.pcb.clock_timers(bus, DMA_TRANSFER_CLOCKS);
            return DMA_TRANSFER_CLOCKS;
        }
        if self.pcb.dma_latency > 0 {
            self.pcb.dma_latency -= 1;
        }
        let mut cycles = {
            let mut sys = Sys { bus: &mut *bus, pcb: &mut self.pcb };
            self.core.step(&mut sys, REP_BUDGET)
        };
        if cycles == 0 {
            cycles = HALT_CLOCKS;
        }
        self.pcb.clock_timers(bus, cycles);
        cycles
    }

    /// External interrupt pins INT0..INT3 (level), as MAME's int0_w..int3_w.
    pub fn set_int_line(&mut self, line: u8, state: bool) {
        if (line as usize) < 4 {
            self.pcb.external_int(line as usize, state);
        }
    }

    /// NMI input (edge-triggered on the rising edge).
    pub fn set_nmi(&mut self, state: bool) {
        self.core.set_nmi(state);
    }

    /// DMA request inputs DRQ0/DRQ1 (level).
    pub fn set_drq(&mut self, channel: u8, state: bool) {
        if (channel as usize) < 2 {
            self.pcb.dma[channel as usize].drq_state = state;
        }
    }

    /// Timer inputs TMRIN0/TMRIN1 (timer 2 has no input pin). Acts on the
    /// rising edge: counts in external-clock mode, retriggers in RTG mode.
    /// The bus is not needed, so a timer reaching max count here can only
    /// raise interrupts (no timer-2-driven DMA).
    pub fn set_tmrin(&mut self, timer: u8, state: bool) {
        if (timer as usize) < 2 {
            let mut nobus = NullBus;
            self.pcb.external_tmrin(&mut nobus, timer as usize, state);
        }
    }

    /// Timer output pins TMROUT0/TMROUT1 (level; the one-clock low pulse of
    /// single-max-count mode is not modelled, the pin then reads high).
    pub fn tmrout(&self, timer: u8) -> bool {
        match timer {
            0 | 1 => self.pcb.timer[timer as usize].out,
            _ => true,
        }
    }

    /// /TEST input used by WAIT (true = not waiting).
    pub fn set_test(&mut self, state: bool) {
        self.core.test_state = state;
    }

    pub fn chip_selects(&self) -> &ChipSelects {
        &self.pcb.mem
    }

    /// Relocation register (RELREG).
    pub fn relreg(&self) -> u16 {
        self.pcb.reloc
    }

    pub fn halted(&self) -> bool {
        self.core.halt
    }

    pub fn regs(&self) -> Regs {
        let c = &self.core;
        Regs {
            ax: c.regs[AX],
            bx: c.regs[BX],
            cx: c.regs[CX],
            dx: c.regs[DX],
            si: c.regs[SI],
            di: c.regs[DI],
            bp: c.regs[BP],
            sp: c.regs[SP],
            cs: c.sregs[CS],
            ds: c.sregs[DS],
            es: c.sregs[ES],
            ss: c.sregs[SS],
            ip: c.ip,
            flags: c.compress_flags(),
        }
    }

    pub fn set_regs(&mut self, r: &Regs) {
        let c = &mut self.core;
        c.regs[AX] = r.ax;
        c.regs[BX] = r.bx;
        c.regs[CX] = r.cx;
        c.regs[DX] = r.dx;
        c.regs[SI] = r.si;
        c.regs[DI] = r.di;
        c.regs[BP] = r.bp;
        c.regs[SP] = r.sp;
        c.sregs[CS] = r.cs;
        c.sregs[DS] = r.ds;
        c.sregs[ES] = r.es;
        c.sregs[SS] = r.ss;
        c.ip = r.ip;
        c.prev_ip = r.ip;
        // Bits 12-15 always read as 1 on the 8086/80186.
        c.expand_flags(r.flags | 0xf000);
    }

    /// Linear address (CS:IP) of the next instruction.
    pub fn pc(&self) -> u32 {
        (((self.core.sregs[CS] as u32) << 4) + self.core.ip as u32) & 0xfffff
    }

    /// Read a PCB register by offset (0..0x7f word index), without side
    /// effects on the bus. Intended for debuggers and tests.
    pub fn pcb_peek(&mut self, offset: u8) -> u16 {
        if offset == 0x12 {
            return self.pcb.intr.poll_status;
        }
        let mut nobus = NullBus;
        self.pcb.port_r(&mut nobus, (offset & 0x7f) as u16)
    }
}

/// Bus used where the PCB needs one but no transfer can happen.
struct NullBus;

impl CpuBus for NullBus {
    fn mem_read8(&mut self, _addr: u32) -> u8 {
        0xff
    }
    fn mem_write8(&mut self, _addr: u32, _val: u8) {}
    fn io_read8(&mut self, _port: u16) -> u8 {
        0xff
    }
    fn io_write8(&mut self, _port: u16, _val: u8) {}
}
