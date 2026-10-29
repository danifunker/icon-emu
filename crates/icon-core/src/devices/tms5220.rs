//! TI TMS5220C speech synthesizer (LPC-10), as fitted to the ICON
//! workstation board ("TMS5220CNL"). Ported from MAME's
//! `src/devices/sound/tms5220.cpp` and `tms5110r.hxx` (BSD-3-Clause; see
//! THIRD_PARTY.md), keeping only what the ICON uses:
//!
//! - one port, FA01: reads return the status register (TS, BL, BE in bits
//!   7..5) and acknowledge the interrupt; writes are commands, or speech data
//!   for the FIFO after Speak External (0x60). Writes complete at once (the
//!   "instant" mode MAME uses when /RS and /WS are not modelled).
//! - no speech ROM (VSM): Speak and Read Byte read all-ones, i.e. a stop frame.
//! - the interrupt (buffer low, buffer empty, talk status falling) on PPI A
//!   port C bit 4, active high.
//!
//! The chip produces one sample per 80 clocks: 8 kHz from a 640 kHz clock.

const FIFO_SIZE: usize = 16;
const NOCOMMAND: u8 = 0xFF;
/// Interpolation-period reload values for the 5220C's rate control.
const RELOAD_TABLE: [u8; 4] = [0, 2, 4, 6];

// TMS5220 / 5220C coefficient set (MAME `tms5220_coeff`).
const KBITS: [u32; 10] = [5, 5, 4, 4, 4, 4, 4, 3, 3, 3];
const ENERGY: [i32; 16] = [0, 1, 2, 3, 4, 6, 8, 11, 16, 23, 33, 47, 63, 85, 114, 0];
const PITCH: [i32; 64] = [
    0, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, //
    30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 44, 46, 48, //
    50, 52, 53, 56, 58, 60, 62, 65, 68, 70, 72, 76, 78, 80, 84, 86, //
    91, 94, 98, 101, 105, 109, 114, 118, 122, 127, 132, 137, 142, 148, 153, 159,
];
const K: [&[i32]; 10] = [
    &[
        -501, -498, -497, -495, -493, -491, -488, -482, -478, -474, -469, -464, -459, -452, -445, -437, //
        -412, -380, -339, -288, -227, -158, -81, -1, 80, 157, 226, 287, 337, 379, 411, 436,
    ],
    &[
        -328, -303, -274, -244, -211, -175, -138, -99, -59, -18, 24, 64, 105, 143, 180, 215, //
        248, 278, 306, 331, 354, 374, 392, 408, 422, 435, 445, 455, 463, 470, 476, 506,
    ],
    &[-441, -387, -333, -279, -225, -171, -117, -63, -9, 45, 98, 152, 206, 260, 314, 368],
    &[-328, -273, -217, -161, -106, -50, 5, 61, 116, 172, 228, 283, 339, 394, 450, 506],
    &[-328, -282, -235, -189, -142, -96, -50, -3, 43, 90, 136, 182, 229, 275, 322, 368],
    &[-256, -212, -168, -123, -79, -35, 10, 54, 98, 143, 187, 232, 276, 320, 365, 409],
    &[-308, -260, -212, -164, -117, -69, -21, 27, 75, 122, 170, 218, 266, 314, 361, 409],
    &[-256, -161, -66, 29, 124, 219, 314, 409],
    &[-256, -176, -96, -15, 65, 146, 226, 307],
    &[-205, -132, -59, 14, 87, 160, 234, 307],
];
const CHIRP: [i8; 52] = [
    0x00, 0x03, 0x0f, 0x28, 0x4c, 0x6c, 0x71, 0x50, 0x25, 0x26, 0x4c, 0x44, 0x1a, 0x32, 0x3b, 0x13, //
    0x37, 0x1a, 0x25, 0x1f, 0x1d, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0,
];
const INTERP_SHIFT: [u32; 8] = [0, 3, 3, 3, 2, 2, 1, 1];

#[derive(Clone, Debug)]
pub struct Tms5220 {
    fifo: [u8; FIFO_SIZE],
    fifo_head: usize,
    fifo_tail: usize,
    fifo_count: usize,
    fifo_bits_taken: u32,

    previous_talk_status: bool,
    spen: bool,
    ddis: bool,
    talk: bool,
    talkd: bool,
    buffer_low: bool,
    buffer_empty: bool,
    /// The interrupt output, active high as the board sees it.
    pub irq: bool,
    command_register: u8,

    olde: bool,
    oldp: bool,
    new_energy: usize,
    new_pitch: usize,
    new_k: [usize; 10],
    current_energy: i32,
    current_pitch: i32,
    current_k: [i32; 10],
    previous_energy: i32,

    subcycle: u8,
    subc_reload: u8,
    pc: u8,
    ip: u8,
    inhibit: bool,
    uv_zpar: bool,
    zpar: bool,
    pitch_zero: bool,
    rate: u8,
    pitch_count: i32,
    u: [i32; 11],
    x: [i32; 10],
    rng: u16,
    excitation: i32,
}

impl Default for Tms5220 {
    fn default() -> Self {
        let mut s = Self {
            fifo: [0; FIFO_SIZE],
            fifo_head: 0,
            fifo_tail: 0,
            fifo_count: 0,
            fifo_bits_taken: 0,
            previous_talk_status: false,
            spen: false,
            ddis: false,
            talk: false,
            talkd: false,
            buffer_low: true,
            buffer_empty: true,
            irq: false,
            command_register: NOCOMMAND,
            olde: true,
            oldp: true,
            new_energy: 0,
            new_pitch: 0,
            new_k: [0; 10],
            current_energy: 0,
            current_pitch: 0,
            current_k: [0; 10],
            previous_energy: 0,
            subcycle: 0,
            subc_reload: 1,
            pc: 0,
            ip: 0,
            inhibit: true,
            uv_zpar: false,
            zpar: false,
            pitch_zero: false,
            rate: 0,
            pitch_count: 0,
            u: [0; 11],
            x: [0; 10],
            rng: 0x1FFF,
            excitation: 0,
        };
        s.reset();
        s
    }
}

impl Tms5220 {
    pub fn reset(&mut self) {
        self.fifo = [0; FIFO_SIZE];
        self.fifo_head = 0;
        self.fifo_tail = 0;
        self.fifo_count = 0;
        self.fifo_bits_taken = 0;
        self.spen = false;
        self.ddis = false;
        self.talk = false;
        self.talkd = false;
        self.previous_talk_status = false;
        self.irq = false;
        self.buffer_empty = true;
        self.buffer_low = true;
        self.command_register = NOCOMMAND;
        self.new_energy = 0;
        self.current_energy = 0;
        self.previous_energy = 0;
        self.new_pitch = 0;
        self.current_pitch = 0;
        self.zpar = false;
        self.uv_zpar = false;
        self.new_k = [0; 10];
        self.current_k = [0; 10];
        self.inhibit = true;
        self.subcycle = 0;
        self.rate = 0;
        self.pitch_count = 0;
        self.pc = 0;
        self.subc_reload = 1;
        self.olde = true;
        self.oldp = true;
        self.ip = RELOAD_TABLE[0];
        self.rng = 0x1FFF;
        self.u = [0; 11];
        self.x = [0; 10];
    }

    fn talk_status(&self) -> bool {
        self.spen || self.talkd
    }

    /// Status register read (port FA01): TS, BL, BE in bits 7, 6, 5. Clears the interrupt.
    pub fn read_status(&mut self) -> u8 {
        self.irq = false;
        (self.talk_status() as u8) << 7 | (self.buffer_low as u8) << 6 | (self.buffer_empty as u8) << 5
    }

    /// Write to port FA01: speech data during Speak External, otherwise a command.
    pub fn write(&mut self, data: u8) {
        if !self.ddis {
            return self.process_command(data);
        }
        let old_buffer_low = self.buffer_low;
        if self.fifo_count < FIFO_SIZE {
            self.fifo[self.fifo_tail] = data;
            self.fifo_tail = (self.fifo_tail + 1) % FIFO_SIZE;
            self.fifo_count += 1;
            self.update_fifo_status_and_ints();
            // Buffer low going inactive while not speaking starts speech.
            if !self.spen && old_buffer_low && !self.buffer_low {
                self.zero_new_frame();
                self.spen = true;
                self.talk = true; // MAME's FAST_START_HACK
            }
        }
    }

    /// The idle "zpar" frame loaded when speech starts.
    fn zero_new_frame(&mut self) {
        self.zpar = true;
        self.uv_zpar = true;
        self.olde = true;
        self.oldp = true;
        self.new_energy = 0;
        self.new_pitch = 0;
        self.new_k = [0, 0, 0, 0, 0xF, 0xF, 0xF, 0x7, 0x7, 0x7];
    }

    fn update_fifo_status_and_ints(&mut self) {
        if self.fifo_count <= 8 {
            if !self.buffer_low {
                self.buffer_low = true;
                self.irq = true;
            }
        } else {
            self.buffer_low = false;
        }
        if self.fifo_count == 0 {
            if !self.buffer_empty {
                self.buffer_empty = true;
                self.irq = true;
            }
            if self.ddis {
                self.talk = false;
                self.spen = false;
            }
        } else {
            self.buffer_empty = false;
        }
        if self.previous_talk_status && !self.talk_status() {
            self.irq = true;
            self.ddis = false;
            self.previous_talk_status = false;
            if self.command_register != NOCOMMAND {
                self.process_command(self.command_register);
            }
        }
        self.previous_talk_status = self.talk_status();
    }

    fn read_bits(&mut self, count: u32) -> usize {
        if !self.ddis {
            return (1 << count) - 1; // no speech ROM: the data lines float high
        }
        let mut val = 0;
        for _ in 0..count {
            val = (val << 1) | ((self.fifo[self.fifo_head] >> self.fifo_bits_taken) & 1) as usize;
            self.fifo_bits_taken += 1;
            if self.fifo_bits_taken >= 8 {
                self.fifo_count -= 1;
                self.fifo[self.fifo_head] = 0;
                self.fifo_head = (self.fifo_head + 1) % FIFO_SIZE;
                self.fifo_bits_taken = 0;
                self.update_fifo_status_and_ints();
            }
        }
        val
    }

    fn process_command(&mut self, cmd: u8) {
        self.command_register = cmd;
        match cmd & 0x70 {
            // Read byte / read and branch / load address: speech ROM commands.
            0x10 | 0x30 | 0x40 => {
                if !self.talk_status() {
                    self.command_register = NOCOMMAND;
                }
            }
            0x00 | 0x20 => {
                self.rate = cmd & 0x0F; // 5220C rate control
                self.command_register = NOCOMMAND;
            }
            0x50 => {
                // Speak (from a speech ROM, which the ICON doesn't have).
                self.spen = true;
                self.talk = true;
                self.ddis = false;
                self.zero_new_frame();
                self.command_register = NOCOMMAND;
            }
            0x60 => {
                // Speak External: clear the FIFO; speech starts once it is over half full.
                self.fifo = [0; FIFO_SIZE];
                self.fifo_head = 0;
                self.fifo_tail = 0;
                self.fifo_count = 0;
                self.fifo_bits_taken = 0;
                self.ddis = true;
                self.zero_new_frame();
                self.command_register = NOCOMMAND;
            }
            _ => {
                // 0x70: reset
                self.reset();
                self.command_register = NOCOMMAND;
            }
        }
        self.update_fifo_status_and_ints();
    }

    fn parse_frame(&mut self) {
        self.uv_zpar = false;
        self.zpar = false;
        if self.rate & 0x04 != 0 {
            let r = self.read_bits(2);
            self.ip = RELOAD_TABLE[r];
        } else {
            self.ip = RELOAD_TABLE[(self.rate & 3) as usize];
        }
        self.update_fifo_status_and_ints();
        if self.ddis && self.buffer_empty {
            return;
        }
        self.new_energy = self.read_bits(4);
        self.update_fifo_status_and_ints();
        if self.ddis && self.buffer_empty {
            return;
        }
        if self.new_energy == 0 || self.new_energy == 15 {
            return;
        }
        let repeat = self.read_bits(1) != 0;
        self.new_pitch = self.read_bits(6);
        self.uv_zpar = self.new_pitch == 0;
        self.update_fifo_status_and_ints();
        if self.ddis && self.buffer_empty || repeat {
            return;
        }
        let ks = if self.new_pitch == 0 { 4 } else { 10 };
        for i in 0..ks {
            self.new_k[i] = self.read_bits(KBITS[i]);
            self.update_fifo_status_and_ints();
            if self.ddis && self.buffer_empty {
                return;
            }
        }
    }

    /// Generate one output sample (signed, 16-bit range).
    pub fn sample(&mut self) -> i16 {
        if !self.talkd {
            self.advance_counters_idle();
            return -1;
        }
        if self.ip == 0 && self.pc == 12 && self.subcycle == 1 {
            self.ip = RELOAD_TABLE[(self.rate & 3) as usize];
            self.parse_frame();
            if self.new_energy == 0x0F {
                self.talk = false;
                self.spen = false;
                self.update_fifo_status_and_ints();
            }
            let (old_uv, new_uv) = (self.oldp, self.new_pitch == 0);
            let new_silence = self.new_energy == 0;
            self.inhibit = (!old_uv && new_uv) || (old_uv && !new_uv) || (self.olde && !new_silence) || (old_uv && new_silence);
        } else {
            let inhibit = self.inhibit && self.ip != 0;
            if self.subcycle == 2 {
                let step = |cur: i32, target: i32, ip: u8| -> i32 {
                    cur + (((target - cur) * (1 - inhibit as i32)) >> INTERP_SHIFT[ip as usize])
                };
                let zpar = self.zpar as i32;
                match self.pc {
                    0 => {
                        if self.ip == 0 {
                            self.pitch_zero = false;
                        }
                        self.current_energy = step(self.current_energy, ENERGY[self.new_energy], self.ip) * (1 - zpar);
                    }
                    1 => self.current_pitch = step(self.current_pitch, PITCH[self.new_pitch], self.ip) * (1 - zpar),
                    2..=11 => {
                        let i = (self.pc - 2) as usize;
                        let z = if i < 4 { zpar } else { self.uv_zpar as i32 };
                        self.current_k[i] = step(self.current_k[i], K[i][self.new_k[i]], self.ip) * (1 - z);
                    }
                    _ => {}
                }
            }
        }
        // Excitation: noise for unvoiced frames, the chirp for voiced ones.
        self.excitation = if self.oldp {
            if self.rng & 1 != 0 { !0x3F } else { 0x40 }
        } else {
            CHIRP[(self.pitch_count as usize).min(51)] as i32
        };
        for _ in 0..20 {
            let bit = ((self.rng >> 12) ^ (self.rng >> 3) ^ (self.rng >> 2) ^ self.rng) & 1;
            self.rng = (self.rng << 1) | bit;
        }
        let mut s = self.lattice_filter();
        while s > 16383 {
            s -= 32768;
        }
        while s < -16384 {
            s += 32768;
        }
        let out = clip_analog(s);

        self.subcycle += 1;
        if self.subcycle == 2 && self.pc == 12 {
            if self.ip == 7 && self.inhibit {
                self.pitch_zero = true;
            }
            if self.ip == 7 {
                self.olde = self.new_energy == 0;
                self.oldp = self.new_pitch == 0;
                self.talkd = self.talk;
                self.update_fifo_status_and_ints();
                if !self.talk && self.spen {
                    self.talk = true;
                }
            }
            self.subcycle = self.subc_reload;
            self.pc = 0;
            self.ip = (self.ip + 1) & 7;
        } else if self.subcycle == 3 {
            self.subcycle = self.subc_reload;
            self.pc += 1;
        }
        self.pitch_count += 1;
        if self.pitch_count >= self.current_pitch || self.pitch_zero {
            self.pitch_count = 0;
        }
        self.pitch_count &= 0x1FF;
        out
    }

    fn advance_counters_idle(&mut self) {
        self.subcycle += 1;
        if self.subcycle == 2 && self.pc == 12 {
            if self.ip == 7 {
                self.talkd = self.talk;
                self.update_fifo_status_and_ints();
                if !self.talk && self.spen {
                    self.talk = true;
                }
            }
            self.subcycle = self.subc_reload;
            self.pc = 0;
            self.ip = (self.ip + 1) & 7;
        } else if self.subcycle == 3 {
            self.subcycle = self.subc_reload;
            self.pc += 1;
        }
    }

    fn lattice_filter(&mut self) -> i32 {
        let k = self.current_k;
        self.u[10] = matrix_multiply(self.previous_energy, self.excitation << 6);
        for i in (0..10).rev() {
            self.u[i] = self.u[i + 1] - matrix_multiply(k[i], self.x[i]);
        }
        for i in (1..10).rev() {
            self.x[i] = self.x[i - 1] + matrix_multiply(k[i - 1], self.u[i - 1]);
        }
        self.x[0] = self.u[0];
        self.previous_energy = self.current_energy;
        self.u[0]
    }
}

fn matrix_multiply(mut a: i32, mut b: i32) -> i32 {
    while a > 511 {
        a -= 1024;
    }
    while a < -512 {
        a += 1024;
    }
    while b > 16383 {
        b -= 32768;
    }
    while b < -16384 {
        b += 32768;
    }
    (a * b) >> 9
}

/// The analog output: 14-bit filter result clipped to the chip's 8-bit DAC, range-extended to 16 bits.
fn clip_analog(s: i32) -> i16 {
    let c = s.clamp(-2048, 2047) & !0xF;
    ((c << 4) | ((c & 0x7F0) >> 3) | ((c & 0x400) >> 10)) as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ICON ROM's speech self-test (codes 4B-4E): reset with 0xFF bytes,
    /// Speak External with 13 data bytes, expect an interrupt and then
    /// "not talking, buffer low, buffer empty" (status & 0xE0 == 0x60).
    #[test]
    fn rom_self_test_sequence() {
        let mut t = Tms5220::default();
        for _ in 0..16 {
            t.write(0xFF);
        }
        assert_eq!(t.read_status() & 0x80, 0, "4B: talking after reset");
        t.write(0x60);
        for b in [0x65, 0x2C, 0x96, 0xAD, 0x7B, 0x6A, 0x9F, 0x66, 0xE4, 0x20, 0x8D, 0x9C, 0xF3] {
            t.write(b);
        }
        let mut irq = false;
        let mut peak = 0i32;
        for _ in 0..8000 {
            peak = peak.max((t.sample() as i32).abs());
            irq |= t.irq;
            if t.read_status() & 0xE0 == 0x60 && irq {
                break;
            }
        }
        assert!(irq, "4C: no interrupt");
        assert_eq!(t.read_status() & 0xE0, 0x60, "4D: speech did not finish");
        assert!(peak > 1000, "no audible output (peak {peak})");
    }
}
