// Intel 8086/80186 instruction core.
//
// Rust port of MAME's src/devices/cpu/i86/i86.cpp, i86inline.h and the
// instruction part of i186.cpp.
// Original code: license BSD-3-Clause, copyright-holders: Carl
// (based on earlier work by Bryan McPhail, Oliver Bergmann, Fabrice Frances
// and David Hedley). See THIRD_PARTY.md at the repository root.
//
// Only the 80186 flavour is implemented (16-bit bus, 80186 timing table,
// 80186 opcodes). Debugger and save-state machinery are not ported.

#![allow(clippy::upper_case_acronyms)]

/// Segment register indices.
pub(crate) const ES: usize = 0;
pub(crate) const CS: usize = 1;
pub(crate) const SS: usize = 2;
pub(crate) const DS: usize = 3;

/// Word register indices.
pub(crate) const AX: usize = 0;
pub(crate) const CX: usize = 1;
pub(crate) const DX: usize = 2;
pub(crate) const BX: usize = 3;
pub(crate) const SP: usize = 4;
pub(crate) const BP: usize = 5;
pub(crate) const SI: usize = 6;
pub(crate) const DI: usize = 7;

/// Byte register indices (low/high halves of AX..BX).
const AL: usize = 0;
const AH: usize = 1;
const CL: usize = 2;

/// ModRM register field -> byte register index (AL, CL, DL, BL, AH, CH, DH, BH).
const BREG: [usize; 8] = [0, 2, 4, 6, 1, 3, 5, 7];

const NMI_IRQ: u32 = 0x02;

/// The 8086 wraps a word access at offset FFFF to offset 0000 of the same
/// segment (the second byte comes from seg:0000). MAME does not model this;
/// the SingleStepTests hardware captures show it, so we do.
const SEGMENT_WRAP: bool = true;

// ---------------------------------------------------------------------------
// Timing table (80186, from the Intel datasheet via MAME). Indices follow
// MAME's enum in i86.h. The 80186 table already includes EA calculation.
// ---------------------------------------------------------------------------
mod t {
    pub const EXCEPTION: usize = 0;
    pub const IRET: usize = 1;
    pub const INT3: usize = 2;
    pub const INT_IMM: usize = 3;
    pub const INTO_NT: usize = 4;
    pub const INTO_T: usize = 5;
    pub const OVERRIDE: usize = 6;
    pub const FLAG_OPS: usize = 7;
    pub const LAHF: usize = 8;
    pub const SAHF: usize = 9;
    pub const AAA: usize = 10;
    pub const AAS: usize = 11;
    pub const AAM: usize = 12;
    pub const AAD: usize = 13;
    pub const DAA: usize = 14;
    pub const DAS: usize = 15;
    pub const CBW: usize = 16;
    pub const CWD: usize = 17;
    pub const HLT: usize = 18;
    pub const LOAD_PTR: usize = 19;
    pub const LEA: usize = 20;
    pub const NOP: usize = 21;
    pub const WAIT: usize = 22;
    pub const XLAT: usize = 23;
    pub const JMP_SHORT: usize = 24;
    pub const JMP_NEAR: usize = 25;
    pub const JMP_FAR: usize = 26;
    pub const JMP_R16: usize = 27;
    pub const JMP_M16: usize = 28;
    pub const JMP_M32: usize = 29;
    pub const CALL_NEAR: usize = 30;
    pub const CALL_FAR: usize = 31;
    pub const CALL_R16: usize = 32;
    pub const CALL_M16: usize = 33;
    pub const CALL_M32: usize = 34;
    pub const RET_NEAR: usize = 35;
    pub const RET_FAR: usize = 36;
    pub const RET_NEAR_IMM: usize = 37;
    pub const RET_FAR_IMM: usize = 38;
    pub const JCC_NT: usize = 39;
    pub const JCC_T: usize = 40;
    pub const JCXZ_NT: usize = 41;
    pub const JCXZ_T: usize = 42;
    pub const LOOP_NT: usize = 43;
    pub const LOOP_T: usize = 44;
    pub const LOOPE_NT: usize = 45;
    pub const LOOPE_T: usize = 46;
    pub const IN_IMM8: usize = 47;
    pub const IN_IMM16: usize = 48;
    pub const IN_DX8: usize = 49;
    pub const IN_DX16: usize = 50;
    pub const OUT_IMM8: usize = 51;
    pub const OUT_IMM16: usize = 52;
    pub const OUT_DX8: usize = 53;
    pub const OUT_DX16: usize = 54;
    pub const MOV_RR8: usize = 55;
    pub const MOV_RM8: usize = 56;
    pub const MOV_MR8: usize = 57;
    pub const MOV_RI8: usize = 58;
    pub const MOV_MI8: usize = 59;
    pub const MOV_RR16: usize = 60;
    pub const MOV_RM16: usize = 61;
    pub const MOV_MR16: usize = 62;
    pub const MOV_RI16: usize = 63;
    pub const MOV_MI16: usize = 64;
    pub const MOV_AM8: usize = 65;
    pub const MOV_AM16: usize = 66;
    pub const MOV_MA8: usize = 67;
    pub const MOV_MA16: usize = 68;
    pub const MOV_SR: usize = 69;
    pub const MOV_SM: usize = 70;
    pub const MOV_RS: usize = 71;
    pub const MOV_MS: usize = 72;
    pub const XCHG_RR8: usize = 73;
    pub const XCHG_RM8: usize = 74;
    pub const XCHG_RR16: usize = 75;
    pub const XCHG_RM16: usize = 76;
    pub const XCHG_AR16: usize = 77;
    pub const PUSH_R16: usize = 78;
    pub const PUSH_M16: usize = 79;
    pub const PUSH_SEG: usize = 80;
    pub const PUSHF: usize = 81;
    pub const POP_R16: usize = 82;
    pub const POP_M16: usize = 83;
    pub const POP_SEG: usize = 84;
    pub const POPF: usize = 85;
    pub const ALU_RR8: usize = 86;
    pub const ALU_RM8: usize = 87;
    pub const ALU_MR8: usize = 88;
    pub const ALU_RI8: usize = 89;
    pub const ALU_MI8: usize = 90;
    pub const ALU_MI8_RO: usize = 91;
    pub const ALU_RR16: usize = 92;
    pub const ALU_RM16: usize = 93;
    pub const ALU_MR16: usize = 94;
    pub const ALU_RI16: usize = 95;
    pub const ALU_MI16: usize = 96;
    pub const ALU_MI16_RO: usize = 97;
    pub const ALU_R16I8: usize = 98;
    pub const ALU_M16I8: usize = 99;
    pub const ALU_M16I8_RO: usize = 100;
    pub const MUL_R8: usize = 101;
    pub const MUL_R16: usize = 102;
    pub const MUL_M8: usize = 103;
    pub const MUL_M16: usize = 104;
    pub const IMUL_R8: usize = 105;
    pub const IMUL_R16: usize = 106;
    pub const IMUL_M8: usize = 107;
    pub const IMUL_M16: usize = 108;
    pub const DIV_R8: usize = 109;
    pub const DIV_R16: usize = 110;
    pub const DIV_M8: usize = 111;
    pub const DIV_M16: usize = 112;
    pub const IDIV_R8: usize = 113;
    pub const IDIV_R16: usize = 114;
    pub const IDIV_M8: usize = 115;
    pub const IDIV_M16: usize = 116;
    pub const INCDEC_R8: usize = 117;
    pub const INCDEC_R16: usize = 118;
    pub const INCDEC_M8: usize = 119;
    pub const INCDEC_M16: usize = 120;
    pub const NEGNOT_R8: usize = 121;
    pub const NEGNOT_R16: usize = 122;
    pub const NEGNOT_M8: usize = 123;
    pub const NEGNOT_M16: usize = 124;
    pub const ROT_REG_1: usize = 125;
    pub const ROT_REG_BASE: usize = 126;
    pub const ROT_REG_BIT: usize = 127;
    pub const ROT_M8_1: usize = 128;
    pub const ROT_M8_BASE: usize = 129;
    pub const ROT_M16_BASE: usize = 132;
    pub const CMPS8: usize = 134;
    pub const CMPS16: usize = 137;
    pub const SCAS8: usize = 140;
    pub const SCAS16: usize = 143;
    pub const LODS8: usize = 146;
    pub const LODS16: usize = 149;
    pub const STOS8: usize = 152;
    pub const STOS16: usize = 155;
    pub const MOVS8: usize = 158;
    pub const MOVS16: usize = 161;
    pub const PUSH_IMM: usize = 176;
    pub const PUSHA: usize = 177;
    pub const POPA: usize = 178;
    pub const IMUL_RRI8: usize = 179;
    pub const IMUL_RMI8: usize = 180;
    pub const IMUL_RRI16: usize = 181;
    pub const IMUL_RMI16: usize = 182;
    pub const ENTER0: usize = 183;
    pub const ENTER1: usize = 184;
    pub const ENTER_BASE: usize = 185;
    pub const ENTER_COUNT: usize = 186;
    pub const LEAVE: usize = 187;
    pub const BOUND: usize = 188;

    /// m_i80186_timing from MAME's i186.cpp.
    pub static TIMING: [u8; 189] = [
        45, 28, 0, 2, 4, 3, 2, 2, 2, 3, 8, 7, 19, 15, 4, 4, 2, 4, 2, 18, 6, 2, 6, 11, 14, 14, 14,
        11, 17, 26, 15, 23, 13, 19, 38, 16, 22, 18, 25, 4, 13, 5, 15, 6, 16, 6, 16, 10, 10, 8, 8,
        9, 9, 7, 7, 2, 9, 12, 3, 12, 2, 9, 12, 4, 13, 8, 8, 9, 9, 2, 11, 2, 11, 4, 17, 4, 17, 3,
        10, 16, 9, 9, 10, 20, 8, 8, 3, 10, 10, 4, 16, 10, 3, 10, 10, 4, 16, 10, 4, 16, 10, 26, 35,
        32, 41, 25, 34, 31, 40, 29, 38, 35, 44, 44, 53, 50, 59, 3, 3, 15, 15, 3, 3, 10, 10, 2, 5,
        1, 15, 17, 1, 15, 17, 1, 22, 5, 22, 22, 5, 22, 15, 5, 15, 15, 5, 15, 12, 6, 11, 12, 6, 11,
        10, 6, 9, 10, 6, 9, 14, 8, 8, 14, 8, 8, 14, 8, 8, 14, 8, 8, 14, 8, 8, 14, 8, 8, 14, 68,
        83, 22, 29, 25, 32, 15, 25, 4, 16, 8, 33,
    ];
}

static PARITY: [bool; 256] = {
    let mut p = [false; 256];
    let mut i = 0;
    while i < 256 {
        p[i] = (i as u8).count_ones().is_multiple_of(2);
        i += 1;
    }
    p
};

/// What the instruction core needs from the outside world. Implemented by
/// the 80186 wrapper, which intercepts the peripheral control block.
pub(crate) trait Host {
    /// Opcode/immediate fetch (20-bit linear address).
    fn fetch(&mut self, addr: u32) -> u8;
    fn read_byte(&mut self, addr: u32) -> u8;
    fn read_word(&mut self, addr: u32) -> u16;
    fn write_byte(&mut self, addr: u32, data: u8);
    fn write_word(&mut self, addr: u32, data: u16);
    fn read_port_byte(&mut self, port: u16) -> u8;
    fn read_port_word(&mut self, port: u16) -> u16;
    fn write_port_byte(&mut self, port: u16, data: u8);
    /// OUT with AL: the 80186 drives both AH and AL onto the internal bus.
    fn write_port_byte_al(&mut self, port: u16, ax: u16);
    fn write_port_word(&mut self, port: u16, data: u16);
    /// Level of the INTR input (from the on-chip interrupt controller).
    fn intr(&self) -> bool;
    /// Interrupt acknowledge: returns the vector number.
    fn irq_ack(&mut self) -> u8;
    /// Relocation register ET bit: ESC opcodes trap to INT 7.
    fn esc_trap(&self) -> bool;
}

/// Register/flag state of the 8086/80186 execution unit.
#[derive(Clone, Debug)]
pub(crate) struct Core {
    pub regs: [u16; 8],
    pub sregs: [u16; 4],
    pub ip: u16,
    pub prev_ip: u16,

    sign_val: i32,
    aux_val: u32,
    over_val: u32,
    zero_val: u32,
    carry_val: u32,
    parity_val: u32,
    tf: bool,
    if_: bool,
    df: bool,
    iopl: u8,
    nt: bool,
    mf: bool,

    pending_irq: u32,
    nmi_state: bool,
    no_interrupt: u8,
    fire_trap: u8,
    pub test_state: bool,

    prefix_seg: usize,
    seg_prefix: bool,
    seg_prefix_next: bool,
    /// A LOCK prefix was just executed: the next opcode belongs to the same
    /// instruction (faults restart at the LOCK).
    lock_next: bool,

    ea_base: u32,
    eo: u16,
    modrm: u8,
    dst: u32,
    src: u32,

    pub halt: bool,
    icount: i32,
}

impl Default for Core {
    fn default() -> Self {
        Self::new()
    }
}

impl Core {
    pub fn new() -> Self {
        let mut c = Core {
            regs: [0; 8],
            sregs: [0; 4],
            ip: 0,
            prev_ip: 0,
            sign_val: 0,
            aux_val: 0,
            over_val: 0,
            zero_val: 1,
            carry_val: 0,
            parity_val: 1,
            tf: false,
            if_: false,
            df: false,
            iopl: 3,
            nt: true,
            mf: true,
            pending_irq: 0,
            nmi_state: false,
            no_interrupt: 0,
            fire_trap: 0,
            test_state: true,
            prefix_seg: 0,
            seg_prefix: false,
            seg_prefix_next: false,
            lock_next: false,
            ea_base: 0,
            eo: 0,
            modrm: 0,
            dst: 0,
            src: 0,
            halt: false,
            icount: 0,
        };
        c.reset();
        c
    }

    /// i8086_common_cpu_device::device_reset
    pub fn reset(&mut self) {
        self.zero_val = 1;
        self.parity_val = 1;
        self.sregs = [0, 0xffff, 0, 0];
        self.ip = 0;
        self.prev_ip = 0;
        self.sign_val = 0;
        self.aux_val = 0;
        self.over_val = 0;
        self.carry_val = 0;
        self.tf = false;
        self.if_ = false;
        self.df = false;
        self.iopl = 3;
        self.nt = true;
        self.mf = true;
        self.pending_irq = 0;
        self.no_interrupt = 0;
        self.fire_trap = 0;
        self.prefix_seg = 0;
        self.seg_prefix = false;
        self.seg_prefix_next = false;
        self.lock_next = false;
        self.ea_base = 0;
        self.eo = 0;
        self.modrm = 0;
        self.dst = 0;
        self.src = 0;
        self.halt = false;
    }

    pub fn set_nmi(&mut self, state: bool) {
        if !self.nmi_state && state {
            self.pending_irq |= NMI_IRQ;
        }
        self.nmi_state = state;
    }

    // -----------------------------------------------------------------------
    // Register helpers
    // -----------------------------------------------------------------------

    #[inline]
    fn rb(&self, b: usize) -> u8 {
        let w = self.regs[b >> 1];
        if b & 1 != 0 {
            (w >> 8) as u8
        } else {
            w as u8
        }
    }

    #[inline]
    fn wb(&mut self, b: usize, v: u8) {
        let w = &mut self.regs[b >> 1];
        if b & 1 != 0 {
            *w = (*w & 0x00ff) | ((v as u16) << 8);
        } else {
            *w = (*w & 0xff00) | v as u16;
        }
    }

    #[inline]
    fn clk(&mut self, op: usize) {
        self.icount -= t::TIMING[op] as i32;
    }

    #[inline]
    fn clkm(&mut self, op_reg: usize, op_mem: usize) {
        let op = if self.modrm >= 0xc0 { op_reg } else { op_mem };
        self.icount -= t::TIMING[op] as i32;
    }

    // -----------------------------------------------------------------------
    // Flags
    // -----------------------------------------------------------------------

    #[inline]
    fn cf(&self) -> bool {
        self.carry_val != 0
    }
    #[inline]
    fn sf(&self) -> bool {
        self.sign_val < 0
    }
    #[inline]
    fn zf(&self) -> bool {
        self.zero_val == 0
    }
    #[inline]
    fn pf(&self) -> bool {
        PARITY[(self.parity_val & 0xff) as usize]
    }
    #[inline]
    fn af(&self) -> bool {
        self.aux_val != 0
    }
    #[inline]
    fn of(&self) -> bool {
        self.over_val != 0
    }

    #[inline]
    fn set_cfb(&mut self, x: u32) {
        self.carry_val = x & 0x100;
    }
    #[inline]
    fn set_cfw(&mut self, x: u32) {
        self.carry_val = x & 0x10000;
    }
    #[inline]
    fn set_af(&mut self, x: u32, y: u32, z: u32) {
        self.aux_val = (x ^ (y ^ z)) & 0x10;
    }
    #[inline]
    fn set_szpf_byte(&mut self, x: u32) {
        let v = x as u8 as i8 as i32;
        self.sign_val = v;
        self.zero_val = v as u32;
        self.parity_val = v as u32;
    }
    #[inline]
    fn set_szpf_word(&mut self, x: u32) {
        let v = x as u16 as i16 as i32;
        self.sign_val = v;
        self.zero_val = v as u32;
        self.parity_val = v as u32;
    }
    #[inline]
    fn set_ofw_add(&mut self, x: u32, y: u32, z: u32) {
        self.over_val = (x ^ y) & (x ^ z) & 0x8000;
    }
    #[inline]
    fn set_ofb_add(&mut self, x: u32, y: u32, z: u32) {
        self.over_val = (x ^ y) & (x ^ z) & 0x80;
    }
    #[inline]
    fn set_ofw_sub(&mut self, x: u32, y: u32, z: u32) {
        self.over_val = (z ^ y) & (z ^ x) & 0x8000;
    }
    #[inline]
    fn set_ofb_sub(&mut self, x: u32, y: u32, z: u32) {
        self.over_val = (z ^ y) & (z ^ x) & 0x80;
    }

    pub fn compress_flags(&self) -> u16 {
        (self.cf() as u16)
            | (1 << 1)
            | ((self.pf() as u16) << 2)
            | ((self.af() as u16) << 4)
            | ((self.zf() as u16) << 6)
            | ((self.sf() as u16) << 7)
            | ((self.tf as u16) << 8)
            | ((self.if_ as u16) << 9)
            | ((self.df as u16) << 10)
            | ((self.of() as u16) << 11)
            | ((self.iopl as u16) << 12)
            | ((self.nt as u16) << 14)
            | ((self.mf as u16) << 15)
    }

    pub fn expand_flags(&mut self, f: u16) {
        let f = f as u32;
        self.carry_val = f & 1;
        self.parity_val = (f & 4 == 0) as u32;
        self.aux_val = f & 16;
        self.zero_val = (f & 64 == 0) as u32;
        self.sign_val = if f & 128 != 0 { -1 } else { 0 };
        self.tf = f & 0x100 != 0;
        self.if_ = f & 0x200 != 0;
        self.df = f & 0x400 != 0;
        self.over_val = f & 0x800;
        self.iopl = ((f >> 12) & 3) as u8;
        self.nt = f & 0x4000 != 0;
        self.mf = f & 0x8000 != 0;
    }

    // -----------------------------------------------------------------------
    // Memory access
    // -----------------------------------------------------------------------

    #[inline]
    fn fetch<H: Host>(&mut self, h: &mut H) -> u8 {
        let addr = (((self.sregs[CS] as u32) << 4) + self.ip as u32) & 0xfffff;
        self.ip = self.ip.wrapping_add(1);
        h.fetch(addr)
    }

    #[inline]
    fn fetch_word<H: Host>(&mut self, h: &mut H) -> u16 {
        let lo = self.fetch(h) as u16;
        let hi = self.fetch(h) as u16;
        lo | (hi << 8)
    }

    /// Segment base, honouring a segment-override prefix for DS/SS-relative
    /// accesses (calc_addr in MAME).
    #[inline]
    fn seg_base(&self, seg: usize, allow_override: bool) -> u32 {
        let s = if self.seg_prefix && allow_override && (seg == DS || seg == SS) {
            self.prefix_seg
        } else {
            seg
        };
        (self.sregs[s] as u32) << 4
    }

    #[inline]
    fn rd8<H: Host>(&mut self, h: &mut H, base: u32, off: u16) -> u8 {
        h.read_byte((base + off as u32) & 0xfffff)
    }

    #[inline]
    fn wr8<H: Host>(&mut self, h: &mut H, base: u32, off: u16, v: u8) {
        h.write_byte((base + off as u32) & 0xfffff, v)
    }

    #[inline]
    fn rd16<H: Host>(&mut self, h: &mut H, base: u32, off: u16) -> u16 {
        if SEGMENT_WRAP && off == 0xffff {
            let lo = h.read_byte((base + 0xffff) & 0xfffff) as u16;
            let hi = h.read_byte(base & 0xfffff) as u16;
            lo | (hi << 8)
        } else {
            h.read_word((base + off as u32) & 0xfffff)
        }
    }

    #[inline]
    fn wr16<H: Host>(&mut self, h: &mut H, base: u32, off: u16, v: u16) {
        if SEGMENT_WRAP && off == 0xffff {
            h.write_byte((base + 0xffff) & 0xfffff, v as u8);
            h.write_byte(base & 0xfffff, (v >> 8) as u8);
        } else {
            h.write_word((base + off as u32) & 0xfffff, v)
        }
    }

    #[inline]
    fn get_mem_b<H: Host>(&mut self, h: &mut H, seg: usize, off: u16) -> u8 {
        let b = self.seg_base(seg, true);
        self.rd8(h, b, off)
    }
    #[inline]
    fn get_mem_w<H: Host>(&mut self, h: &mut H, seg: usize, off: u16) -> u16 {
        let b = self.seg_base(seg, true);
        self.rd16(h, b, off)
    }
    #[inline]
    fn put_mem_b<H: Host>(&mut self, h: &mut H, seg: usize, off: u16, v: u8) {
        let b = self.seg_base(seg, true);
        self.wr8(h, b, off, v)
    }
    #[inline]
    fn put_mem_w<H: Host>(&mut self, h: &mut H, seg: usize, off: u16, v: u16) {
        let b = self.seg_base(seg, true);
        self.wr16(h, b, off, v)
    }

    #[inline]
    fn push<H: Host>(&mut self, h: &mut H, data: u16) {
        let b = self.seg_base(SS, false);
        let sp = self.regs[SP].wrapping_sub(2);
        self.wr16(h, b, sp, data);
        self.regs[SP] = sp;
    }

    #[inline]
    fn pop<H: Host>(&mut self, h: &mut H) -> u16 {
        let b = self.seg_base(SS, false);
        let sp = self.regs[SP];
        let v = self.rd16(h, b, sp);
        self.regs[SP] = sp.wrapping_add(2);
        v
    }

    /// Effective address decode (get_ea). For register forms (mod=3) the
    /// previous EA is left unchanged, as in MAME.
    fn get_ea<H: Host>(&mut self, h: &mut H) {
        let r = &self.regs;
        let modrm = self.modrm & 0xc7;
        let (off, seg): (u16, usize) = match modrm & 7 {
            0 => (r[BX].wrapping_add(r[SI]), DS),
            1 => (r[BX].wrapping_add(r[DI]), DS),
            2 => (r[BP].wrapping_add(r[SI]), SS),
            3 => (r[BP].wrapping_add(r[DI]), SS),
            4 => (r[SI], DS),
            5 => (r[DI], DS),
            6 => (r[BP], SS),
            _ => (r[BX], DS),
        };
        let (off, seg) = match modrm >> 6 {
            0 => {
                if modrm == 0x06 {
                    (self.fetch_word(h), DS)
                } else {
                    (off, seg)
                }
            }
            1 => {
                let d = self.fetch(h) as i8 as u16;
                (off.wrapping_add(d), seg)
            }
            2 => {
                let d = self.fetch_word(h);
                (off.wrapping_add(d), seg)
            }
            _ => return,
        };
        self.eo = off;
        self.ea_base = self.seg_base(seg, true);
    }

    #[inline]
    fn reg_byte(&self) -> u8 {
        self.rb(BREG[((self.modrm >> 3) & 7) as usize])
    }
    #[inline]
    fn set_reg_byte(&mut self, v: u8) {
        self.wb(BREG[((self.modrm >> 3) & 7) as usize], v)
    }
    #[inline]
    fn reg_word(&self) -> u16 {
        self.regs[((self.modrm >> 3) & 7) as usize]
    }
    #[inline]
    fn set_reg_word(&mut self, v: u16) {
        self.regs[((self.modrm >> 3) & 7) as usize] = v;
    }

    #[inline]
    fn get_rm_byte<H: Host>(&mut self, h: &mut H) -> u8 {
        if self.modrm >= 0xc0 {
            self.rb(BREG[(self.modrm & 7) as usize])
        } else {
            self.get_ea(h);
            let (b, o) = (self.ea_base, self.eo);
            self.rd8(h, b, o)
        }
    }

    #[inline]
    fn get_rm_word<H: Host>(&mut self, h: &mut H) -> u16 {
        if self.modrm >= 0xc0 {
            self.regs[(self.modrm & 7) as usize]
        } else {
            self.get_ea(h);
            let (b, o) = (self.ea_base, self.eo);
            self.rd16(h, b, o)
        }
    }

    #[inline]
    fn get_next_rm_word<H: Host>(&mut self, h: &mut H) -> u16 {
        let (b, o) = (self.ea_base, self.eo.wrapping_add(2));
        self.rd16(h, b, o)
    }

    #[inline]
    fn putback_rm_byte<H: Host>(&mut self, h: &mut H, v: u8) {
        if self.modrm >= 0xc0 {
            self.wb(BREG[(self.modrm & 7) as usize], v)
        } else {
            let (b, o) = (self.ea_base, self.eo);
            self.wr8(h, b, o, v)
        }
    }

    #[inline]
    fn putback_rm_word<H: Host>(&mut self, h: &mut H, v: u16) {
        if self.modrm >= 0xc0 {
            self.regs[(self.modrm & 7) as usize] = v
        } else {
            let (b, o) = (self.ea_base, self.eo);
            self.wr16(h, b, o, v)
        }
    }

    #[inline]
    fn put_rm_byte<H: Host>(&mut self, h: &mut H, v: u8) {
        if self.modrm >= 0xc0 {
            self.wb(BREG[(self.modrm & 7) as usize], v)
        } else {
            self.get_ea(h);
            let (b, o) = (self.ea_base, self.eo);
            self.wr8(h, b, o, v)
        }
    }

    #[inline]
    fn put_rm_word<H: Host>(&mut self, h: &mut H, v: u16) {
        if self.modrm >= 0xc0 {
            self.regs[(self.modrm & 7) as usize] = v
        } else {
            self.get_ea(h);
            let (b, o) = (self.ea_base, self.eo);
            self.wr16(h, b, o, v)
        }
    }

    fn put_imm_rm_byte<H: Host>(&mut self, h: &mut H) {
        if self.modrm >= 0xc0 {
            let v = self.fetch(h);
            self.wb(BREG[(self.modrm & 7) as usize], v)
        } else {
            self.get_ea(h);
            let v = self.fetch(h);
            let (b, o) = (self.ea_base, self.eo);
            self.wr8(h, b, o, v)
        }
    }

    fn put_imm_rm_word<H: Host>(&mut self, h: &mut H) {
        if self.modrm >= 0xc0 {
            let v = self.fetch_word(h);
            self.regs[(self.modrm & 7) as usize] = v
        } else {
            self.get_ea(h);
            let v = self.fetch_word(h);
            let (b, o) = (self.ea_base, self.eo);
            self.wr16(h, b, o, v)
        }
    }

    fn def_br8<H: Host>(&mut self, h: &mut H) {
        self.modrm = self.fetch(h);
        self.src = self.reg_byte() as u32;
        self.dst = self.get_rm_byte(h) as u32;
    }
    fn def_wr16<H: Host>(&mut self, h: &mut H) {
        self.modrm = self.fetch(h);
        self.src = self.reg_word() as u32;
        self.dst = self.get_rm_word(h) as u32;
    }
    fn def_r8b<H: Host>(&mut self, h: &mut H) {
        self.modrm = self.fetch(h);
        self.dst = self.reg_byte() as u32;
        self.src = self.get_rm_byte(h) as u32;
    }
    fn def_r16w<H: Host>(&mut self, h: &mut H) {
        self.modrm = self.fetch(h);
        self.dst = self.reg_word() as u32;
        self.src = self.get_rm_word(h) as u32;
    }
    fn def_ald8<H: Host>(&mut self, h: &mut H) {
        self.src = self.fetch(h) as u32;
        self.dst = self.rb(AL) as u32;
    }
    fn def_axd16<H: Host>(&mut self, h: &mut H) {
        self.src = self.fetch_word(h) as u32;
        self.dst = self.regs[AX] as u32;
    }

    // -----------------------------------------------------------------------
    // ALU
    // -----------------------------------------------------------------------

    fn addb(&mut self, c: u32) -> u32 {
        let res = self.dst.wrapping_add(self.src).wrapping_add(c);
        self.set_ofb_add(res, self.src, self.dst);
        self.set_af(res, self.src, self.dst);
        self.set_szpf_byte(res);
        self.dst = res & 0xff;
        res
    }
    fn addx(&mut self, c: u32) -> u32 {
        let res = self.dst.wrapping_add(self.src).wrapping_add(c);
        self.set_ofw_add(res, self.src, self.dst);
        self.set_af(res, self.src, self.dst);
        self.set_szpf_word(res);
        self.dst = res & 0xffff;
        res
    }
    fn subb(&mut self, b: u32) -> u32 {
        let res = self.dst.wrapping_sub(self.src).wrapping_sub(b);
        self.set_ofb_sub(res, self.src, self.dst);
        self.set_af(res, self.src, self.dst);
        self.set_szpf_byte(res);
        self.dst = res & 0xff;
        res
    }
    fn subx(&mut self, b: u32) -> u32 {
        let res = self.dst.wrapping_sub(self.src).wrapping_sub(b);
        self.set_ofw_sub(res, self.src, self.dst);
        self.set_af(res, self.src, self.dst);
        self.set_szpf_word(res);
        self.dst = res & 0xffff;
        res
    }
    fn orb(&mut self) {
        self.dst |= self.src;
        self.carry_val = 0;
        self.over_val = 0;
        self.aux_val = 0;
        self.set_szpf_byte(self.dst);
    }
    fn orw(&mut self) {
        self.dst |= self.src;
        self.carry_val = 0;
        self.over_val = 0;
        self.aux_val = 0;
        self.set_szpf_word(self.dst);
    }
    fn andb(&mut self) {
        self.dst &= self.src;
        self.carry_val = 0;
        self.over_val = 0;
        self.aux_val = 0;
        self.set_szpf_byte(self.dst);
    }
    fn andx(&mut self) {
        self.dst &= self.src;
        self.carry_val = 0;
        self.over_val = 0;
        self.aux_val = 0;
        self.set_szpf_word(self.dst);
    }
    fn xorb(&mut self) {
        self.dst ^= self.src;
        self.carry_val = 0;
        self.over_val = 0;
        self.aux_val = 0;
        self.set_szpf_byte(self.dst);
    }
    fn xorw(&mut self) {
        self.dst ^= self.src;
        self.carry_val = 0;
        self.over_val = 0;
        self.aux_val = 0;
        self.set_szpf_word(self.dst);
    }

    /// The eight ALU operations selected by bits 5..3 of an opcode/ModRM.
    fn alu_byte(&mut self, op: u8) -> bool {
        // returns true if result must be written back
        match op & 7 {
            0 => {
                let r = self.addb(0);
                self.set_cfb(r);
            }
            1 => self.orb(),
            2 => {
                let c = self.cf() as u32;
                let r = self.addb(c);
                self.set_cfb(r);
            }
            3 => {
                let c = self.cf() as u32;
                let r = self.subb(c);
                self.set_cfb(r);
            }
            4 => self.andb(),
            5 => {
                let r = self.subb(0);
                self.set_cfb(r);
            }
            6 => self.xorb(),
            _ => {
                let r = self.subb(0);
                self.set_cfb(r);
                return false;
            }
        }
        true
    }

    fn alu_word(&mut self, op: u8) -> bool {
        match op & 7 {
            0 => {
                let r = self.addx(0);
                self.set_cfw(r);
            }
            1 => self.orw(),
            2 => {
                let c = self.cf() as u32;
                let r = self.addx(c);
                self.set_cfw(r);
            }
            3 => {
                let c = self.cf() as u32;
                let r = self.subx(c);
                self.set_cfw(r);
            }
            4 => self.andx(),
            5 => {
                let r = self.subx(0);
                self.set_cfw(r);
            }
            6 => self.xorw(),
            _ => {
                let r = self.subx(0);
                self.set_cfw(r);
                return false;
            }
        }
        true
    }

    fn rol_byte(&mut self) {
        self.carry_val = self.dst & 0x80;
        self.dst = (self.dst << 1) | self.cf() as u32;
    }
    fn rol_word(&mut self) {
        self.carry_val = self.dst & 0x8000;
        self.dst = (self.dst << 1) | self.cf() as u32;
    }
    fn ror_byte(&mut self) {
        self.carry_val = self.dst & 1;
        self.dst = (self.dst >> 1) | if self.cf() { 0x80 } else { 0 };
    }
    fn ror_word(&mut self) {
        self.carry_val = self.dst & 1;
        self.dst = (self.dst >> 1) + if self.cf() { 0x8000 } else { 0 };
    }
    fn rolc_byte(&mut self) {
        self.dst = (self.dst << 1) | self.cf() as u32;
        self.set_cfb(self.dst);
    }
    fn rolc_word(&mut self) {
        self.dst = (self.dst << 1) | self.cf() as u32;
        self.set_cfw(self.dst);
    }
    fn rorc_byte(&mut self) {
        self.dst |= if self.cf() { 0x100 } else { 0 };
        self.carry_val = self.dst & 1;
        self.dst >>= 1;
    }
    fn rorc_word(&mut self) {
        self.dst |= if self.cf() { 0x10000 } else { 0 };
        self.carry_val = self.dst & 1;
        self.dst >>= 1;
    }
    fn shra_byte<H: Host>(&mut self, h: &mut H, c: u8) {
        for _ in 0..c {
            self.carry_val = self.dst & 1;
            self.dst = ((self.dst as u8 as i8) >> 1) as i32 as u32;
        }
        self.set_szpf_byte(self.dst);
        self.putback_rm_byte(h, self.dst as u8);
    }
    fn shra_word<H: Host>(&mut self, h: &mut H, c: u8) {
        for _ in 0..c {
            self.carry_val = self.dst & 1;
            self.dst = ((self.dst as u16 as i16) >> 1) as i32 as u32;
        }
        self.set_szpf_word(self.dst);
        self.putback_rm_word(h, self.dst as u16);
    }

    /// Shift/rotate a byte by `c` (c already masked to 5 bits, may be 0).
    /// OF is set from the last single-bit step, as the bit-serial hardware
    /// does (MAME leaves OF unchanged for counts other than 1).
    fn rotshft_byte_n<H: Host>(&mut self, h: &mut H, c: u8) {
        if c == 0 {
            return;
        }
        let mut prev = self.dst;
        for _ in 0..c {
            prev = self.dst;
            match self.modrm & 0x38 {
                0x00 => self.rol_byte(),
                0x08 => self.ror_byte(),
                0x10 => self.rolc_byte(),
                0x18 => self.rorc_byte(),
                0x20 | 0x30 => {
                    self.dst <<= 1;
                    self.carry_val = self.dst & 0x100;
                }
                0x28 => {
                    self.carry_val = self.dst & 1;
                    self.dst = (self.dst & 0xff) >> 1;
                }
                _ => {
                    self.carry_val = self.dst & 1;
                    self.dst = ((self.dst as u8 as i8) >> 1) as u8 as u32;
                }
            }
        }
        self.over_val = (prev ^ self.dst) & 0x80;
        if self.modrm & 0x20 != 0 {
            self.set_szpf_byte(self.dst);
        }
        self.putback_rm_byte(h, self.dst as u8);
    }

    fn rotshft_word_n<H: Host>(&mut self, h: &mut H, c: u8) {
        if c == 0 {
            return;
        }
        let mut prev = self.dst;
        for _ in 0..c {
            prev = self.dst;
            match self.modrm & 0x38 {
                0x00 => self.rol_word(),
                0x08 => self.ror_word(),
                0x10 => self.rolc_word(),
                0x18 => self.rorc_word(),
                0x20 | 0x30 => {
                    self.dst <<= 1;
                    self.carry_val = self.dst & 0x10000;
                }
                0x28 => {
                    self.carry_val = self.dst & 1;
                    self.dst = (self.dst & 0xffff) >> 1;
                }
                _ => {
                    self.carry_val = self.dst & 1;
                    self.dst = ((self.dst as u16 as i16) >> 1) as u16 as u32;
                }
            }
        }
        self.over_val = (prev ^ self.dst) & 0x8000;
        if self.modrm & 0x20 != 0 {
            self.set_szpf_word(self.dst);
        }
        self.putback_rm_word(h, self.dst as u16);
    }

    fn inc_word_reg(&mut self, reg: usize) {
        let tmp = self.regs[reg] as u32;
        let tmp1 = tmp.wrapping_add(1);
        self.over_val = (tmp == 0x7fff) as u32;
        self.set_af(tmp1, tmp, 1);
        self.set_szpf_word(tmp1);
        self.regs[reg] = tmp1 as u16;
    }

    fn dec_word_reg(&mut self, reg: usize) {
        let tmp = self.regs[reg] as u32;
        let tmp1 = tmp.wrapping_sub(1);
        self.over_val = (tmp == 0x8000) as u32;
        self.set_af(tmp1, tmp, 1);
        self.set_szpf_word(tmp1);
        self.regs[reg] = tmp1 as u16;
    }

    fn jmp<H: Host>(&mut self, h: &mut H, cond: bool) {
        let rel = self.fetch(h) as i8 as u16;
        if cond {
            self.ip = self.ip.wrapping_add(rel);
            self.clk(t::JCC_T);
        } else {
            self.clk(t::JCC_NT);
        }
    }

    /// DAA / DAS. MAME tests the already-adjusted AL against 0x9F; the
    /// 8086 (per SingleStepTests captures) compares the original AL against
    /// 0x9F when AF was set and 0x99 otherwise, and the low-nibble step
    /// does not affect CF.
    fn adj4(&mut self, param1: i8, param2: i8) {
        let old_al = self.rb(AL);
        let old_af = self.af();
        let old_cf = self.cf();
        if old_af || (old_al & 0xf) > 9 {
            let v = old_al.wrapping_add(param1 as u8);
            self.wb(AL, v);
            self.aux_val = 1;
        } else {
            self.aux_val = 0;
        }
        let limit = if old_af { 0x9f } else { 0x99 };
        if old_cf || old_al > limit {
            let v = self.rb(AL).wrapping_add(param2 as u8);
            self.wb(AL, v);
            self.carry_val = 1;
        } else {
            self.carry_val = 0;
        }
        self.set_szpf_byte(self.rb(AL) as u32);
    }

    /// AAA / AAS
    fn adjb(&mut self, param1: i8, param2: i8) {
        if self.af() || (self.rb(AL) & 0xf) > 9 {
            let al = self.rb(AL).wrapping_add(param1 as u8);
            let ah = self.rb(AH).wrapping_add(param2 as u8);
            self.wb(AL, al);
            self.wb(AH, ah);
            self.aux_val = 1;
            self.carry_val = 1;
        } else {
            self.aux_val = 0;
            self.carry_val = 0;
        }
        let al = self.rb(AL) & 0x0f;
        self.wb(AL, al);
    }

    // -----------------------------------------------------------------------
    // String operations (one iteration)
    // -----------------------------------------------------------------------

    #[inline]
    fn step_b(&self) -> u16 {
        if self.df {
            0xffff
        } else {
            1
        }
    }
    #[inline]
    fn step_w(&self) -> u16 {
        if self.df {
            0xfffe
        } else {
            2
        }
    }

    fn i_insb<H: Host>(&mut self, h: &mut H) {
        let b = self.seg_base(ES, false);
        let di = self.regs[DI];
        let data = h.read_port_byte(self.regs[DX]);
        self.wr8(h, b, di, data);
        self.regs[DI] = di.wrapping_add(self.step_b());
        self.clk(t::IN_IMM8);
    }
    fn i_insw<H: Host>(&mut self, h: &mut H) {
        let b = self.seg_base(ES, false);
        let di = self.regs[DI];
        let data = h.read_port_word(self.regs[DX]);
        self.wr16(h, b, di, data);
        self.regs[DI] = di.wrapping_add(self.step_w());
        self.clk(t::IN_IMM16);
    }
    fn i_outsb<H: Host>(&mut self, h: &mut H) {
        let v = self.get_mem_b(h, DS, self.regs[SI]);
        h.write_port_byte(self.regs[DX], v);
        self.regs[SI] = self.regs[SI].wrapping_add(self.step_b());
        self.clk(t::OUT_IMM8);
    }
    fn i_outsw<H: Host>(&mut self, h: &mut H) {
        let v = self.get_mem_w(h, DS, self.regs[SI]);
        h.write_port_word(self.regs[DX], v);
        self.regs[SI] = self.regs[SI].wrapping_add(self.step_w());
        self.clk(t::OUT_IMM16);
    }
    fn i_movsb<H: Host>(&mut self, h: &mut H) {
        let v = self.get_mem_b(h, DS, self.regs[SI]);
        self.put_mem_b(h, ES, self.regs[DI], v);
        let s = self.step_b();
        self.regs[DI] = self.regs[DI].wrapping_add(s);
        self.regs[SI] = self.regs[SI].wrapping_add(s);
        self.clk(t::MOVS8);
    }
    fn i_movsw<H: Host>(&mut self, h: &mut H) {
        let v = self.get_mem_w(h, DS, self.regs[SI]);
        self.put_mem_w(h, ES, self.regs[DI], v);
        let s = self.step_w();
        self.regs[DI] = self.regs[DI].wrapping_add(s);
        self.regs[SI] = self.regs[SI].wrapping_add(s);
        self.clk(t::MOVS16);
    }
    fn i_cmpsb<H: Host>(&mut self, h: &mut H) {
        self.src = self.get_mem_b(h, ES, self.regs[DI]) as u32;
        self.dst = self.get_mem_b(h, DS, self.regs[SI]) as u32;
        let r = self.subb(0);
        self.set_cfb(r);
        let s = self.step_b();
        self.regs[DI] = self.regs[DI].wrapping_add(s);
        self.regs[SI] = self.regs[SI].wrapping_add(s);
        self.clk(t::CMPS8);
    }
    fn i_cmpsw<H: Host>(&mut self, h: &mut H) {
        self.src = self.get_mem_w(h, ES, self.regs[DI]) as u32;
        self.dst = self.get_mem_w(h, DS, self.regs[SI]) as u32;
        let r = self.subx(0);
        self.set_cfw(r);
        let s = self.step_w();
        self.regs[DI] = self.regs[DI].wrapping_add(s);
        self.regs[SI] = self.regs[SI].wrapping_add(s);
        self.clk(t::CMPS16);
    }
    fn i_stosb<H: Host>(&mut self, h: &mut H) {
        let al = self.rb(AL);
        self.put_mem_b(h, ES, self.regs[DI], al);
        self.regs[DI] = self.regs[DI].wrapping_add(self.step_b());
        self.clk(t::STOS8);
    }
    fn i_stosw<H: Host>(&mut self, h: &mut H) {
        let ax = self.regs[AX];
        self.put_mem_w(h, ES, self.regs[DI], ax);
        self.regs[DI] = self.regs[DI].wrapping_add(self.step_w());
        self.clk(t::STOS16);
    }
    fn i_lodsb<H: Host>(&mut self, h: &mut H) {
        let v = self.get_mem_b(h, DS, self.regs[SI]);
        self.wb(AL, v);
        self.regs[SI] = self.regs[SI].wrapping_add(self.step_b());
        self.clk(t::LODS8);
    }
    fn i_lodsw<H: Host>(&mut self, h: &mut H) {
        self.regs[AX] = self.get_mem_w(h, DS, self.regs[SI]);
        self.regs[SI] = self.regs[SI].wrapping_add(self.step_w());
        self.clk(t::LODS16);
    }
    fn i_scasb<H: Host>(&mut self, h: &mut H) {
        self.src = self.get_mem_b(h, ES, self.regs[DI]) as u32;
        self.dst = self.rb(AL) as u32;
        let r = self.subb(0);
        self.set_cfb(r);
        self.regs[DI] = self.regs[DI].wrapping_add(self.step_b());
        self.clk(t::SCAS8);
    }
    fn i_scasw<H: Host>(&mut self, h: &mut H) {
        self.src = self.get_mem_w(h, ES, self.regs[DI]) as u32;
        self.dst = self.regs[AX] as u32;
        let r = self.subx(0);
        self.set_cfw(r);
        self.regs[DI] = self.regs[DI].wrapping_add(self.step_w());
        self.clk(t::SCAS16);
    }

    /// Execute one iteration of string op `op` (0x6c..0x6f, 0xa4..0xaf).
    fn string_op<H: Host>(&mut self, h: &mut H, op: u8) {
        match op {
            0x6c => self.i_insb(h),
            0x6d => self.i_insw(h),
            0x6e => self.i_outsb(h),
            0x6f => self.i_outsw(h),
            0xa4 => self.i_movsb(h),
            0xa5 => self.i_movsw(h),
            0xa6 => self.i_cmpsb(h),
            0xa7 => self.i_cmpsw(h),
            0xaa => self.i_stosb(h),
            0xab => self.i_stosw(h),
            0xac => self.i_lodsb(h),
            0xad => self.i_lodsw(h),
            0xae => self.i_scasb(h),
            _ => self.i_scasw(h),
        }
    }

    fn i_popf<H: Host>(&mut self, h: &mut H) {
        let tmp = self.pop(h);
        self.expand_flags(tmp | 0xf000);
        self.clk(t::POPF);
        if self.tf {
            self.fire_trap = 1;
        }
    }

    /// Fetch the opcode following a REP prefix, absorbing one segment prefix.
    fn repx_op<H: Host>(&mut self, h: &mut H) -> u8 {
        let mut next = self.fetch(h);
        let seg = match next {
            0x26 => Some(ES),
            0x2e => Some(CS),
            0x36 => Some(SS),
            0x3e => Some(DS),
            _ => None,
        };
        if let Some(s) = seg {
            self.seg_prefix = true;
            self.seg_prefix_next = true;
            self.prefix_seg = s;
            next = self.fetch(h);
            self.clk(t::OVERRIDE);
        }
        next
    }

    // -----------------------------------------------------------------------
    // Interrupts
    // -----------------------------------------------------------------------

    /// Take interrupt `num` (pushes FLAGS, CS, IP; clears TF/IF).
    pub fn interrupt<H: Host>(&mut self, h: &mut H, num: u8) {
        let f = self.compress_flags();
        self.push(h, f);
        self.tf = false;
        self.if_ = false;
        self.vector_to(h, num);
    }

    /// Hardware INTR: flags are pushed before the vector is acknowledged.
    fn interrupt_ack<H: Host>(&mut self, h: &mut H) {
        let f = self.compress_flags();
        self.push(h, f);
        self.tf = false;
        self.if_ = false;
        let num = h.irq_ack();
        self.vector_to(h, num);
    }

    fn vector_to<H: Host>(&mut self, h: &mut H, num: u8) {
        // MAME never charges the EXCEPTION entry; we do, so that INT n,
        // hardware interrupts and exceptions cost a plausible ~45+ clocks.
        self.clk(t::EXCEPTION);
        let a = num as u32 * 4;
        let dest_off = h.read_word(a);
        let dest_seg = h.read_word(a + 2);
        let cs = self.sregs[CS];
        self.push(h, cs);
        let ip = self.ip;
        self.push(h, ip);
        self.ip = dest_off;
        self.prev_ip = dest_off;
        self.sregs[CS] = dest_seg;
    }

    /// Fault-style exception: return address points at the faulting
    /// instruction (including any prefixes).
    fn fault<H: Host>(&mut self, h: &mut H, num: u8) {
        self.ip = self.prev_ip;
        self.seg_prefix = false;
        self.seg_prefix_next = false;
        self.lock_next = false;
        self.interrupt(h, num);
    }

    // -----------------------------------------------------------------------
    // Main entry point
    // -----------------------------------------------------------------------

    /// Execute one instruction (including any segment-override prefixes) or
    /// service one pending interrupt. `budget` bounds how many clocks a
    /// REP-prefixed string instruction may run before it is suspended (and
    /// restarted on the next call, as MAME does when its icount runs out).
    /// Returns the clocks consumed (0 if halted and nothing happened).
    pub fn step<H: Host>(&mut self, h: &mut H, budget: i32) -> u32 {
        self.icount = budget;
        loop {
            if self.seg_prefix_next {
                self.seg_prefix = true;
                self.seg_prefix_next = false;
            } else if self.lock_next {
                // keep prev_ip and any segment override
                self.lock_next = false;
            } else {
                self.prev_ip = self.ip;
                self.seg_prefix = false;

                // Dispatch IRQ
                if self.no_interrupt == 0 {
                    if self.pending_irq & NMI_IRQ != 0 {
                        self.pending_irq &= !NMI_IRQ;
                        self.halt = false;
                        self.interrupt(h, 2);
                        return (budget - self.icount) as u32;
                    } else if self.if_ && h.intr() {
                        self.halt = false;
                        self.interrupt_ack(h);
                        return (budget - self.icount) as u32;
                    }
                }

                if self.halt {
                    return (budget - self.icount) as u32;
                }

                // No interrupt allowed between last instruction and this one
                if self.no_interrupt > 0 {
                    self.no_interrupt -= 1;
                }

                // Trap should allow one instruction to be executed
                if self.fire_trap != 0 {
                    if self.fire_trap >= 2 {
                        self.interrupt(h, 1);
                        self.fire_trap = 0;
                        return (budget - self.icount) as u32;
                    } else {
                        self.fire_trap += 1;
                    }
                }
            }

            let op = self.fetch(h);
            self.execute_op(h, op);

            // Segment/LOCK prefixes are separate "instructions" in MAME's
            // loop, during which interrupts are not taken; fold them into the
            // step of the instruction they belong to.
            if !self.seg_prefix_next && !self.lock_next {
                break;
            }
        }
        (budget - self.icount) as u32
    }

    fn execute_op<H: Host>(&mut self, h: &mut H, op: u8) {
        match op {
            // ---------------- ALU r/m <-> reg ----------------
            0x00 | 0x08 | 0x10 | 0x18 | 0x20 | 0x28 | 0x30 | 0x38 => {
                self.def_br8(h);
                if self.alu_byte(op >> 3) {
                    // ADC/SBB: MAME writes back before setting CF; same result.
                    self.putback_rm_byte(h, self.dst as u8);
                    self.clkm(t::ALU_RR8, t::ALU_MR8);
                } else {
                    self.clkm(t::ALU_RR8, t::ALU_RM8);
                }
            }
            0x01 | 0x09 | 0x11 | 0x19 | 0x21 | 0x29 | 0x31 | 0x39 => {
                self.def_wr16(h);
                if self.alu_word(op >> 3) {
                    self.putback_rm_word(h, self.dst as u16);
                    if op == 0x31 {
                        self.clkm(t::ALU_RR16, t::ALU_RM16);
                    } else {
                        self.clkm(t::ALU_RR16, t::ALU_MR16);
                    }
                } else {
                    self.clkm(t::ALU_RR16, t::ALU_RM16);
                }
            }
            0x02 | 0x0a | 0x12 | 0x1a | 0x22 | 0x2a | 0x32 | 0x3a => {
                self.def_r8b(h);
                if self.alu_byte(op >> 3) {
                    self.set_reg_byte(self.dst as u8);
                }
                self.clkm(t::ALU_RR8, t::ALU_RM8);
            }
            0x03 | 0x0b | 0x13 | 0x1b | 0x23 | 0x2b | 0x33 | 0x3b => {
                self.def_r16w(h);
                if self.alu_word(op >> 3) {
                    self.set_reg_word(self.dst as u16);
                }
                self.clkm(t::ALU_RR16, t::ALU_RM16);
            }
            0x04 | 0x0c | 0x14 | 0x1c | 0x24 | 0x2c | 0x34 | 0x3c => {
                self.def_ald8(h);
                if self.alu_byte(op >> 3) {
                    self.wb(AL, self.dst as u8);
                }
                self.clk(t::ALU_RI8);
            }
            0x05 | 0x0d | 0x15 | 0x1d | 0x25 | 0x2d | 0x35 | 0x3d => {
                self.def_axd16(h);
                if self.alu_word(op >> 3) {
                    self.regs[AX] = self.dst as u16;
                }
                self.clk(t::ALU_RI16);
            }

            // ---------------- segment push/pop ----------------
            0x06 | 0x0e | 0x16 | 0x1e => {
                let v = self.sregs[((op >> 3) & 3) as usize];
                self.push(h, v);
                self.clk(t::PUSH_SEG);
            }
            0x07 | 0x17 | 0x1f => {
                let s = ((op >> 3) & 3) as usize;
                self.sregs[s] = self.pop(h);
                self.clk(t::POP_SEG);
                if s == SS {
                    self.no_interrupt = 1;
                }
            }

            // ---------------- prefixes ----------------
            0x26 | 0x2e | 0x36 | 0x3e => {
                self.seg_prefix_next = true;
                self.prefix_seg = ((op >> 3) & 3) as usize;
                self.clk(t::OVERRIDE);
            }

            0x27 => {
                self.adj4(6, 0x60);
                self.clk(t::DAA);
            }
            0x2f => {
                self.adj4(-6, -0x60);
                self.clk(t::DAS);
            }
            0x37 => {
                // MAME adds 2 to AH when AL > 0xF9 (80286 behaviour); the
                // 8086 captures show AH is always incremented by exactly 1.
                self.adjb(6, 1);
                self.clk(t::AAA);
            }
            0x3f => {
                self.adjb(-6, -1);
                self.clk(t::AAS);
            }

            0x40..=0x47 => {
                self.inc_word_reg((op & 7) as usize);
                self.clk(t::INCDEC_R16);
            }
            0x48..=0x4f => {
                self.dec_word_reg((op & 7) as usize);
                self.clk(t::INCDEC_R16);
            }
            0x50..=0x57 => {
                let r = (op & 7) as usize;
                // PUSH SP pushes the decremented value on the 8086/80186.
                let v = if r == SP { self.regs[SP].wrapping_sub(2) } else { self.regs[r] };
                self.push(h, v);
                self.clk(t::PUSH_R16);
            }
            0x58..=0x5f => {
                let v = self.pop(h);
                self.regs[(op & 7) as usize] = v;
                self.clk(t::POP_R16);
            }

            // ---------------- 80186 additions ----------------
            0x60 => {
                // PUSHA
                let tmp = self.regs[SP];
                for r in [AX, CX, DX, BX] {
                    let v = self.regs[r];
                    self.push(h, v);
                }
                self.push(h, tmp);
                for r in [BP, SI, DI] {
                    let v = self.regs[r];
                    self.push(h, v);
                }
                self.clk(t::PUSHA);
            }
            0x61 => {
                // POPA
                self.regs[DI] = self.pop(h);
                self.regs[SI] = self.pop(h);
                self.regs[BP] = self.pop(h);
                self.pop(h);
                self.regs[BX] = self.pop(h);
                self.regs[DX] = self.pop(h);
                self.regs[CX] = self.pop(h);
                self.regs[AX] = self.pop(h);
                self.clk(t::POPA);
            }
            0x62 => {
                // BOUND
                self.modrm = self.fetch(h);
                if self.modrm >= 0xc0 {
                    // Register operand: undefined; MAME uses a stale EA.
                    // Treated as an invalid opcode, as on the 80286.
                    self.invalid_opcode(h);
                    return;
                }
                let low = self.get_rm_word(h) as i16;
                let high = self.get_next_rm_word(h) as i16;
                let idx = self.reg_word() as i16;
                self.clk(t::BOUND);
                if idx < low || idx > high {
                    // Deviation from MAME: INT 5 is a fault on the 80186; the
                    // saved CS:IP points at the BOUND instruction.
                    self.fault(h, 5);
                }
            }
            0x68 => {
                let v = self.fetch_word(h);
                self.push(h, v);
                self.clk(t::PUSH_IMM);
            }
            0x69 => {
                self.def_r16w(h);
                let tmp = self.fetch_word(h);
                let r = (self.src as u16 as i16 as i32) * (tmp as i16 as i32);
                self.dst = r as u32;
                let ov = (r >> 15) != 0 && (r >> 15) != -1;
                self.carry_val = ov as u32;
                self.over_val = ov as u32;
                self.set_reg_word(r as u16);
                self.clkm(t::IMUL_RRI16, t::IMUL_RMI16);
            }
            0x6a => {
                let v = self.fetch(h) as i8 as u16;
                self.push(h, v);
                self.clk(t::PUSH_IMM);
            }
            0x6b => {
                self.def_r16w(h);
                let src2 = self.fetch(h) as i8 as i32;
                let r = (self.src as u16 as i16 as i32) * src2;
                self.dst = r as u32;
                let ov = (r >> 15) != 0 && (r >> 15) != -1;
                self.carry_val = ov as u32;
                self.over_val = ov as u32;
                self.set_reg_word(r as u16);
                self.clkm(t::IMUL_RRI8, t::IMUL_RMI8);
            }
            0x6c..=0x6f => self.string_op(h, op),

            0x70 => self.jmp(h, self.of()),
            0x71 => self.jmp(h, !self.of()),
            0x72 => self.jmp(h, self.cf()),
            0x73 => self.jmp(h, !self.cf()),
            0x74 => self.jmp(h, self.zf()),
            0x75 => self.jmp(h, !self.zf()),
            0x76 => self.jmp(h, self.cf() || self.zf()),
            0x77 => self.jmp(h, !(self.cf() || self.zf())),
            0x78 => self.jmp(h, self.sf()),
            0x79 => self.jmp(h, !self.sf()),
            0x7a => self.jmp(h, self.pf()),
            0x7b => self.jmp(h, !self.pf()),
            // MAME tests (SF!=OF)&&!ZF here; the hardware tests SF!=OF only.
            0x7c => self.jmp(h, self.sf() != self.of()),
            0x7d => self.jmp(h, self.sf() == self.of()),
            0x7e => self.jmp(h, self.zf() || (self.sf() != self.of())),
            0x7f => self.jmp(h, (self.sf() == self.of()) && !self.zf()),

            0x80 | 0x82 => {
                self.modrm = self.fetch(h);
                self.dst = self.get_rm_byte(h) as u32;
                // MAME sign-extends the 0x82 immediate into 32 bits, which
                // breaks the carry calculation; it is a byte operand.
                self.src = self.fetch(h) as u32;
                if self.modrm >= 0xc0 {
                    self.clk(t::ALU_RI8);
                } else if self.modrm & 0x38 == 0x38 {
                    self.clk(t::ALU_MI8_RO);
                } else {
                    self.clk(t::ALU_MI8);
                }
                if self.alu_byte(self.modrm >> 3) {
                    self.putback_rm_byte(h, self.dst as u8);
                }
            }
            0x81 | 0x83 => {
                self.modrm = self.fetch(h);
                self.dst = self.get_rm_word(h) as u32;
                self.src = if op == 0x81 {
                    self.fetch_word(h) as u32
                } else {
                    self.fetch(h) as i8 as u16 as u32
                };
                let (r, mo, m) = if op == 0x81 {
                    (t::ALU_RI16, t::ALU_MI16_RO, t::ALU_MI16)
                } else {
                    (t::ALU_R16I8, t::ALU_M16I8_RO, t::ALU_M16I8)
                };
                if self.modrm >= 0xc0 {
                    self.clk(r);
                } else if self.modrm & 0x38 == 0x38 {
                    self.clk(mo);
                } else {
                    self.clk(m);
                }
                if self.alu_word(self.modrm >> 3) {
                    self.putback_rm_word(h, self.dst as u16);
                }
            }
            0x84 => {
                self.def_br8(h);
                self.andb();
                self.clkm(t::ALU_RR8, t::ALU_RM8);
            }
            0x85 => {
                self.def_wr16(h);
                self.andx();
                self.clkm(t::ALU_RR16, t::ALU_RM16);
            }
            0x86 => {
                self.def_br8(h);
                self.set_reg_byte(self.dst as u8);
                self.putback_rm_byte(h, self.src as u8);
                self.clkm(t::XCHG_RR8, t::XCHG_RM8);
            }
            0x87 => {
                self.def_wr16(h);
                self.set_reg_word(self.dst as u16);
                self.putback_rm_word(h, self.src as u16);
                self.clkm(t::XCHG_RR16, t::XCHG_RM16);
            }
            0x88 => {
                self.modrm = self.fetch(h);
                let v = self.reg_byte();
                self.put_rm_byte(h, v);
                self.clkm(t::MOV_RR8, t::MOV_MR8);
            }
            0x89 => {
                self.modrm = self.fetch(h);
                let v = self.reg_word();
                self.put_rm_word(h, v);
                self.clkm(t::MOV_RR16, t::MOV_MR16);
            }
            0x8a => {
                self.modrm = self.fetch(h);
                let v = self.get_rm_byte(h);
                self.set_reg_byte(v);
                self.clkm(t::MOV_RR8, t::MOV_RM8);
            }
            0x8b => {
                self.modrm = self.fetch(h);
                let v = self.get_rm_word(h);
                self.set_reg_word(v);
                self.clkm(t::MOV_RR16, t::MOV_RM16);
            }
            0x8c => {
                self.modrm = self.fetch(h);
                let v = self.sregs[((self.modrm & 0x18) >> 3) as usize];
                self.put_rm_word(h, v);
                self.clkm(t::MOV_RS, t::MOV_MS);
            }
            0x8d => {
                self.modrm = self.fetch(h);
                self.get_ea(h);
                self.set_reg_word(self.eo);
                self.clk(t::LEA);
            }
            0x8e => {
                // 80186 version: MOV CS / reg 4..7 trap to INT 6.
                self.modrm = self.fetch(h);
                let v = self.get_rm_word(h);
                self.clkm(t::MOV_SR, t::MOV_SM);
                match self.modrm & 0x38 {
                    0x00 => self.sregs[ES] = v,
                    0x10 => {
                        self.sregs[SS] = v;
                        self.no_interrupt = 1;
                    }
                    0x18 => self.sregs[DS] = v,
                    _ => self.invalid_opcode(h),
                }
            }
            0x8f => {
                self.modrm = self.fetch(h);
                let v = self.pop(h);
                self.put_rm_word(h, v);
                self.clkm(t::POP_R16, t::POP_M16);
            }
            0x90 => self.clk(t::NOP),
            0x91..=0x97 => {
                let r = (op & 7) as usize;
                self.regs.swap(AX, r);
                self.clk(t::XCHG_AR16);
            }
            0x98 => {
                let v = if self.rb(AL) & 0x80 != 0 { 0xff } else { 0 };
                self.wb(AH, v);
                self.clk(t::CBW);
            }
            0x99 => {
                self.regs[DX] = if self.rb(AH) & 0x80 != 0 { 0xffff } else { 0 };
                self.clk(t::CWD);
            }
            0x9a => {
                let tmp = self.fetch_word(h);
                let tmp2 = self.fetch_word(h);
                let cs = self.sregs[CS];
                self.push(h, cs);
                let ip = self.ip;
                self.push(h, ip);
                self.ip = tmp;
                self.sregs[CS] = tmp2;
                self.clk(t::CALL_FAR);
            }
            0x9b => {
                if !self.test_state {
                    // Wait for /TEST: re-execute.
                    self.ip = self.ip.wrapping_sub(1);
                    self.clk(t::WAIT);
                } else {
                    self.clk(t::WAIT);
                }
            }
            0x9c => {
                let f = self.compress_flags();
                self.push(h, f);
                self.clk(t::PUSHF);
            }
            0x9d => self.i_popf(h),
            0x9e => {
                let tmp = (self.compress_flags() & 0xff00) | (self.rb(AH) as u16 & 0xd5);
                self.expand_flags(tmp);
                self.clk(t::SAHF);
            }
            0x9f => {
                let f = self.compress_flags() as u8;
                self.wb(AH, f);
                self.clk(t::LAHF);
            }
            0xa0 => {
                let a = self.fetch_word(h);
                let v = self.get_mem_b(h, DS, a);
                self.wb(AL, v);
                self.clk(t::MOV_AM8);
            }
            0xa1 => {
                let a = self.fetch_word(h);
                self.regs[AX] = self.get_mem_w(h, DS, a);
                self.clk(t::MOV_AM16);
            }
            0xa2 => {
                let a = self.fetch_word(h);
                let v = self.rb(AL);
                self.put_mem_b(h, DS, a, v);
                self.clk(t::MOV_MA8);
            }
            0xa3 => {
                let a = self.fetch_word(h);
                let v = self.regs[AX];
                self.put_mem_w(h, DS, a, v);
                self.clk(t::MOV_MA16);
            }
            0xa4..=0xa7 | 0xaa..=0xaf => self.string_op(h, op),
            0xa8 => {
                self.def_ald8(h);
                self.andb();
                self.clk(t::ALU_RI8);
            }
            0xa9 => {
                self.def_axd16(h);
                self.andx();
                self.clk(t::ALU_RI16);
            }
            0xb0..=0xb7 => {
                let v = self.fetch(h);
                self.wb(BREG[(op & 7) as usize], v);
                self.clk(t::MOV_RI8);
            }
            0xb8..=0xbf => {
                let v = self.fetch_word(h);
                self.regs[(op & 7) as usize] = v;
                self.clk(t::MOV_RI16);
            }

            0xc0 => {
                self.modrm = self.fetch(h);
                self.src = self.get_rm_byte(h) as u32;
                self.dst = self.src;
                let c = self.fetch(h) & 0x1f;
                self.clkm(t::ROT_REG_BASE, t::ROT_M8_BASE);
                self.icount -= t::TIMING[t::ROT_REG_BIT] as i32 * c as i32;
                self.rotshft_byte_n(h, c);
            }
            0xc1 => {
                self.modrm = self.fetch(h);
                self.src = self.get_rm_word(h) as u32;
                self.dst = self.src;
                let c = self.fetch(h) & 0x1f;
                self.clkm(t::ROT_REG_BASE, t::ROT_M16_BASE);
                self.icount -= t::TIMING[t::ROT_REG_BIT] as i32 * c as i32;
                self.rotshft_word_n(h, c);
            }
            0xc2 => {
                let count = self.fetch_word(h);
                self.ip = self.pop(h);
                self.regs[SP] = self.regs[SP].wrapping_add(count);
                self.clk(t::RET_NEAR_IMM);
            }
            0xc3 => {
                self.ip = self.pop(h);
                self.clk(t::RET_NEAR);
            }
            0xc4 | 0xc5 => {
                self.modrm = self.fetch(h);
                let v = self.get_rm_word(h);
                self.set_reg_word(v);
                let s = self.get_next_rm_word(h);
                self.sregs[if op == 0xc4 { ES } else { DS }] = s;
                self.clk(t::LOAD_PTR);
            }
            0xc6 => {
                self.modrm = self.fetch(h);
                self.put_imm_rm_byte(h);
                self.clkm(t::MOV_RI8, t::MOV_MI8);
            }
            0xc7 => {
                self.modrm = self.fetch(h);
                self.put_imm_rm_word(h);
                self.clkm(t::MOV_RI16, t::MOV_MI16);
            }
            0xc8 => {
                // ENTER. Deviation from MAME, whose ENTER allocates the locals
                // before copying the display and copies from the new frame
                // pointer; this follows Intel's documented algorithm.
                let nb = self.fetch_word(h);
                let level = self.fetch(h) & 0x1f;
                let c = match level {
                    0 => t::ENTER0,
                    1 => t::ENTER1,
                    _ => t::ENTER_BASE,
                };
                self.clk(c);
                if level > 1 {
                    self.icount -= level as i32 * t::TIMING[t::ENTER_COUNT] as i32;
                }
                let bp = self.regs[BP];
                self.push(h, bp);
                let frame = self.regs[SP];
                if level > 0 {
                    let ssb = self.seg_base(SS, false);
                    let mut bp = bp;
                    for _ in 1..level {
                        bp = bp.wrapping_sub(2);
                        let v = self.rd16(h, ssb, bp);
                        self.push(h, v);
                    }
                    self.push(h, frame);
                }
                self.regs[BP] = frame;
                self.regs[SP] = self.regs[SP].wrapping_sub(nb);
            }
            0xc9 => {
                // LEAVE
                self.regs[SP] = self.regs[BP];
                self.regs[BP] = self.pop(h);
                self.clk(t::LEAVE);
            }
            0xca => {
                let count = self.fetch_word(h);
                self.ip = self.pop(h);
                self.sregs[CS] = self.pop(h);
                self.regs[SP] = self.regs[SP].wrapping_add(count);
                self.clk(t::RET_FAR_IMM);
            }
            0xcb => {
                self.ip = self.pop(h);
                self.sregs[CS] = self.pop(h);
                self.clk(t::RET_FAR);
            }
            0xcc => {
                self.interrupt(h, 3);
                self.clk(t::INT3);
            }
            0xcd => {
                let n = self.fetch(h);
                self.interrupt(h, n);
                self.clk(t::INT_IMM);
            }
            0xce => {
                if self.of() {
                    self.interrupt(h, 4);
                    self.clk(t::INTO_T);
                } else {
                    self.clk(t::INTO_NT);
                }
            }
            0xcf => {
                self.ip = self.pop(h);
                self.sregs[CS] = self.pop(h);
                self.i_popf(h);
                self.clk(t::IRET);
            }
            0xd0 => {
                self.modrm = self.fetch(h);
                self.src = self.get_rm_byte(h) as u32;
                self.dst = self.src;
                self.clkm(t::ROT_REG_1, t::ROT_M8_1);
                match self.modrm & 0x38 {
                    0x38 => {
                        self.shra_byte(h, 1);
                        self.over_val = 0;
                    }
                    0x20 | 0x28 | 0x30 => {
                        self.rotshft_byte_n(h, 1);
                        self.over_val = (self.src ^ self.dst) & 0x80;
                    }
                    _ => {
                        self.rotshft_byte_n(h, 1);
                        self.over_val = (self.src ^ self.dst) & 0x80;
                    }
                }
            }
            0xd1 => {
                self.modrm = self.fetch(h);
                self.src = self.get_rm_word(h) as u32;
                self.dst = self.src;
                self.clkm(t::ROT_REG_1, t::ROT_M8_1);
                if self.modrm & 0x38 == 0x38 {
                    self.shra_word(h, 1);
                    self.over_val = 0;
                } else {
                    self.rotshft_word_n(h, 1);
                    self.over_val = (self.src ^ self.dst) & 0x8000;
                }
            }
            0xd2 => {
                self.modrm = self.fetch(h);
                self.src = self.get_rm_byte(h) as u32;
                self.dst = self.src;
                let c = self.rb(CL) & 0x1f;
                self.clkm(t::ROT_REG_BASE, t::ROT_M16_BASE);
                self.icount -= t::TIMING[t::ROT_REG_BIT] as i32 * c as i32;
                self.rotshft_byte_n(h, c);
            }
            0xd3 => {
                self.modrm = self.fetch(h);
                self.src = self.get_rm_word(h) as u32;
                self.dst = self.src;
                let c = self.rb(CL) & 0x1f;
                self.clkm(t::ROT_REG_BASE, t::ROT_M16_BASE);
                self.icount -= t::TIMING[t::ROT_REG_BIT] as i32 * c as i32;
                self.rotshft_word_n(h, c);
            }
            0xd4 => {
                let base = self.fetch(h);
                let al = self.rb(AL);
                if let (Some(q), Some(r)) = (al.checked_div(base), al.checked_rem(base)) {
                    self.wb(AH, q);
                    self.wb(AL, r);
                    // MAME sets SZP from AX; the hardware uses AL.
                    self.set_szpf_byte(r as u32);
                    self.clk(t::AAM);
                } else {
                    // The 8086 leaves SZP as for a zero result (hardware
                    // captures); MAME leaves them unchanged.
                    self.set_szpf_byte(0);
                    self.interrupt(h, 0);
                }
            }
            0xd5 => {
                let base = self.fetch(h);
                let al = self.rb(AH).wrapping_mul(base).wrapping_add(self.rb(AL));
                self.wb(AL, al);
                self.wb(AH, 0);
                self.set_szpf_byte(al as u32);
                self.clk(t::AAD);
            }
            0xd6 => {
                // SALC
                let v = if self.cf() { 0xff } else { 0 };
                self.wb(AL, v);
                self.clk(t::ALU_RR8);
            }
            0xd7 => {
                let off = self.regs[BX].wrapping_add(self.rb(AL) as u16);
                let v = self.get_mem_b(h, DS, off);
                self.wb(AL, v);
                self.clk(t::XLAT);
            }
            0xd8..=0xdf => {
                if h.esc_trap() {
                    self.fault(h, 7);
                } else {
                    self.modrm = self.fetch(h);
                    self.get_rm_byte(h);
                    self.clk(t::NOP);
                    // MAME forces AX=FFFF for FSTSW AX (DF E0) to fake an
                    // absent 80C187; without a coprocessor interface the
                    // ESC is a plain NOP and AX is left alone.
                }
            }
            0xe0 => {
                let disp = self.fetch(h) as i8 as u16;
                self.regs[CX] = self.regs[CX].wrapping_sub(1);
                if !self.zf() && self.regs[CX] != 0 {
                    self.ip = self.ip.wrapping_add(disp);
                    self.clk(t::LOOP_T);
                } else {
                    self.clk(t::LOOP_NT);
                }
            }
            0xe1 => {
                let disp = self.fetch(h) as i8 as u16;
                self.regs[CX] = self.regs[CX].wrapping_sub(1);
                if self.zf() && self.regs[CX] != 0 {
                    self.ip = self.ip.wrapping_add(disp);
                    self.clk(t::LOOPE_T);
                } else {
                    self.clk(t::LOOPE_NT);
                }
            }
            0xe2 => {
                let disp = self.fetch(h) as i8 as u16;
                self.regs[CX] = self.regs[CX].wrapping_sub(1);
                if self.regs[CX] != 0 {
                    self.ip = self.ip.wrapping_add(disp);
                    self.clk(t::LOOP_T);
                } else {
                    self.clk(t::LOOP_NT);
                }
            }
            0xe3 => {
                let disp = self.fetch(h) as i8 as u16;
                if self.regs[CX] == 0 {
                    self.ip = self.ip.wrapping_add(disp);
                    self.clk(t::JCXZ_T);
                } else {
                    self.clk(t::JCXZ_NT);
                }
            }
            0xe4 => {
                let port = self.fetch(h) as u16;
                let v = h.read_port_byte(port);
                self.wb(AL, v);
                self.clk(t::IN_IMM8);
            }
            0xe5 => {
                let port = self.fetch(h) as u16;
                self.regs[AX] = h.read_port_word(port);
                self.clk(t::IN_IMM16);
            }
            0xe6 => {
                let port = self.fetch(h) as u16;
                h.write_port_byte_al(port, self.regs[AX]);
                self.clk(t::OUT_IMM8);
            }
            0xe7 => {
                let port = self.fetch(h) as u16;
                h.write_port_word(port, self.regs[AX]);
                self.clk(t::OUT_IMM16);
            }
            0xe8 => {
                let tmp = self.fetch_word(h);
                let ip = self.ip;
                self.push(h, ip);
                self.ip = self.ip.wrapping_add(tmp);
                self.clk(t::CALL_NEAR);
            }
            0xe9 => {
                let off = self.fetch_word(h);
                self.ip = self.ip.wrapping_add(off);
                self.clk(t::JMP_NEAR);
            }
            0xea => {
                let tmp = self.fetch_word(h);
                let tmp1 = self.fetch_word(h);
                self.sregs[CS] = tmp1;
                self.ip = tmp;
                self.clk(t::JMP_FAR);
            }
            0xeb => {
                let rel = self.fetch(h) as i8 as u16;
                self.clk(t::JMP_SHORT);
                self.ip = self.ip.wrapping_add(rel);
            }
            0xec => {
                let v = h.read_port_byte(self.regs[DX]);
                self.wb(AL, v);
                self.clk(t::IN_DX8);
            }
            0xed => {
                self.regs[AX] = h.read_port_word(self.regs[DX]);
                self.clk(t::IN_DX16);
            }
            0xee => {
                h.write_port_byte_al(self.regs[DX], self.regs[AX]);
                self.clk(t::OUT_DX8);
            }
            0xef => {
                h.write_port_word(self.regs[DX], self.regs[AX]);
                self.clk(t::OUT_DX16);
            }
            0xf0 | 0xf1 => {
                // LOCK (0xf1 is an alias). No bus lock output is modelled.
                self.lock_next = true;
                self.clk(t::NOP);
            }
            0xf2 | 0xf3 => self.rep(h, op == 0xf3),
            0xf4 => {
                self.halt = true;
                self.clk(t::HLT);
            }
            0xf5 => {
                self.carry_val = (self.carry_val == 0) as u32;
                self.clk(t::FLAG_OPS);
            }
            0xf6 => self.op_f6(h),
            0xf7 => self.op_f7(h),
            0xf8 => {
                self.carry_val = 0;
                self.clk(t::FLAG_OPS);
            }
            0xf9 => {
                self.carry_val = 1;
                self.clk(t::FLAG_OPS);
            }
            0xfa => {
                self.if_ = false;
                self.clk(t::FLAG_OPS);
            }
            0xfb => {
                self.if_ = true;
                self.no_interrupt = 1;
                self.clk(t::FLAG_OPS);
            }
            0xfc => {
                self.df = false;
                self.clk(t::FLAG_OPS);
            }
            0xfd => {
                self.df = true;
                self.clk(t::FLAG_OPS);
            }
            0xfe => {
                self.modrm = self.fetch(h);
                let tmp = self.get_rm_byte(h) as u32;
                match self.modrm & 0x38 {
                    0x00 => {
                        let tmp1 = tmp.wrapping_add(1);
                        self.over_val = (tmp == 0x7f) as u32;
                        self.set_af(tmp1, tmp, 1);
                        self.set_szpf_byte(tmp1);
                        self.putback_rm_byte(h, tmp1 as u8);
                        self.clkm(t::INCDEC_R8, t::INCDEC_M8);
                    }
                    0x08 => {
                        let tmp1 = tmp.wrapping_sub(1);
                        self.over_val = (tmp == 0x80) as u32;
                        self.set_af(tmp1, tmp, 1);
                        self.set_szpf_byte(tmp1);
                        self.putback_rm_byte(h, tmp1 as u8);
                        self.clkm(t::INCDEC_R8, t::INCDEC_M8);
                    }
                    _ => {
                        // MAME: "FE Pre with unimplemented mod" (no effect)
                    }
                }
            }
            0xff => self.op_ff(h),

            // 0x0f (POP CS on the 8086), 0x63..0x67 (Jcc aliases on the
            // 8086) are not decoded by the 80186: invalid opcode, INT 6.
            _ => self.invalid_opcode(h),
        }
    }

    fn invalid_opcode<H: Host>(&mut self, h: &mut H) {
        self.icount -= 10;
        // Deviation from MAME: the saved CS:IP points at the invalid opcode
        // (as documented for the 80186 "unused opcode" exception).
        self.fault(h, 6);
    }

    fn rep<H: Host>(&mut self, h: &mut H, repe: bool) {
        let next = self.repx_op(h);
        let mut c = self.regs[CX];
        match next {
            0x6c..=0x6f | 0xa4..=0xa7 | 0xaa..=0xaf => {
                let cmp = matches!(next, 0xa6 | 0xa7 | 0xae | 0xaf);
                self.clk(t::OVERRIDE);
                if c != 0 {
                    loop {
                        self.string_op(h, next);
                        c = c.wrapping_sub(1);
                        if c == 0 || self.icount <= 0 {
                            break;
                        }
                        if cmp && (self.zf() != repe) {
                            break;
                        }
                    }
                }
                self.regs[CX] = c;
                self.seg_prefix = false;
                self.seg_prefix_next = false;
                if c != 0 {
                    let terminated = cmp && (self.zf() != repe);
                    if !terminated {
                        self.ip = self.prev_ip;
                    }
                }
            }
            _ => {
                // Not a string op: the REP is ignored and the following
                // opcode executes as a normal instruction (with any segment
                // prefix). MAME rewinds IP and runs it in the next loop
                // iteration; doing it here keeps one instruction per step.
                if self.seg_prefix_next {
                    self.seg_prefix = true;
                    self.seg_prefix_next = false;
                }
                self.execute_op(h, next);
            }
        }
    }

    fn op_f6<H: Host>(&mut self, h: &mut H) {
        self.modrm = self.fetch(h);
        let tmp = self.get_rm_byte(h) as u32;
        match self.modrm & 0x38 {
            0x00 | 0x08 => {
                let v = tmp & self.fetch(h) as u32;
                self.carry_val = 0;
                self.over_val = 0;
                self.set_szpf_byte(v);
                self.clkm(t::ALU_RI8, t::ALU_MI8_RO);
            }
            0x10 => {
                self.putback_rm_byte(h, !tmp as u8);
                self.clkm(t::NEGNOT_R8, t::NEGNOT_M8);
            }
            0x18 => {
                self.dst = 0;
                self.src = tmp;
                let r = self.subb(0);
                self.set_cfb(r);
                self.putback_rm_byte(h, self.dst as u8);
                self.clkm(t::NEGNOT_R8, t::NEGNOT_M8);
            }
            0x20 => {
                let r = self.rb(AL) as u32 * tmp;
                self.regs[AX] = r as u16;
                let f = (self.rb(AH) != 0) as u32;
                self.carry_val = f;
                self.over_val = f;
                self.zero_val = self.regs[AX] as u32;
                self.clkm(t::MUL_R8, t::MUL_M8);
            }
            0x28 => {
                let r = (self.rb(AL) as i8 as i16) * (tmp as u8 as i8 as i16);
                self.regs[AX] = r as u16;
                // MAME uses AH != 0; overflow means AX != sign-extended AL.
                let f = (r != r as i8 as i16) as u32;
                self.carry_val = f;
                self.over_val = f;
                self.zero_val = self.regs[AX] as u32;
                self.clkm(t::IMUL_R8, t::IMUL_M8);
            }
            0x30 => {
                let ax = self.regs[AX] as u32;
                match (ax.checked_div(tmp), ax.checked_rem(tmp)) {
                    (Some(q), Some(r)) if q <= 0xff => {
                        self.wb(AL, q as u8);
                        self.wb(AH, r as u8);
                        self.zero_val = q & 0xff;
                    }
                    _ => self.interrupt(h, 0),
                }
                self.clkm(t::DIV_R8, t::DIV_M8);
            }
            _ => {
                if tmp != 0 {
                    let a = self.regs[AX] as i16 as i32;
                    let d = tmp as u8 as i8 as i32;
                    let q = a / d;
                    let r = a % d;
                    let lower = if self.mf { -0x7f } else { -0x80 };
                    if q > 0x7f || q < lower {
                        self.interrupt(h, 0);
                    } else {
                        self.wb(AL, q as u8);
                        self.wb(AH, r as u8);
                        self.zero_val = (q as u32) & 0xff;
                    }
                } else {
                    self.interrupt(h, 0);
                }
                self.clkm(t::IDIV_R8, t::IDIV_M8);
            }
        }
    }

    fn op_f7<H: Host>(&mut self, h: &mut H) {
        self.modrm = self.fetch(h);
        let tmp = self.get_rm_word(h) as u32;
        match self.modrm & 0x38 {
            0x00 | 0x08 => {
                let v = tmp & self.fetch_word(h) as u32;
                self.carry_val = 0;
                self.over_val = 0;
                self.set_szpf_word(v);
                self.clkm(t::ALU_RI16, t::ALU_MI16_RO);
            }
            0x10 => {
                self.putback_rm_word(h, !tmp as u16);
                self.clkm(t::NEGNOT_R16, t::NEGNOT_M16);
            }
            0x18 => {
                self.dst = 0;
                self.src = tmp;
                let r = self.subx(0);
                self.set_cfw(r);
                self.putback_rm_word(h, self.dst as u16);
                self.clkm(t::NEGNOT_R16, t::NEGNOT_M16);
            }
            0x20 => {
                let r = self.regs[AX] as u32 * tmp;
                self.regs[AX] = r as u16;
                self.regs[DX] = (r >> 16) as u16;
                let f = (self.regs[DX] != 0) as u32;
                self.carry_val = f;
                self.over_val = f;
                self.zero_val = (self.regs[AX] | self.regs[DX]) as u32;
                self.clkm(t::MUL_R16, t::MUL_M16);
            }
            0x28 => {
                let r = (self.regs[AX] as i16 as i32) * (tmp as u16 as i16 as i32);
                self.regs[AX] = r as u16;
                self.regs[DX] = (r >> 16) as u16;
                let f = (r != r as i16 as i32) as u32;
                self.carry_val = f;
                self.over_val = f;
                self.zero_val = (self.regs[AX] | self.regs[DX]) as u32;
                self.clkm(t::IMUL_R16, t::IMUL_M16);
            }
            0x30 => {
                let n = ((self.regs[DX] as u32) << 16) | self.regs[AX] as u32;
                match (n.checked_div(tmp), n.checked_rem(tmp)) {
                    (Some(q), Some(r)) if q <= 0xffff => {
                        self.regs[AX] = q as u16;
                        self.regs[DX] = r as u16;
                        self.zero_val = q;
                    }
                    _ => self.interrupt(h, 0),
                }
                self.clkm(t::DIV_R16, t::DIV_M16);
            }
            _ => {
                if tmp != 0 {
                    let n = (((self.regs[DX] as u32) << 16) | self.regs[AX] as u32) as i32 as i64;
                    let d = tmp as u16 as i16 as i64;
                    let q = n / d;
                    let r = n % d;
                    let lower = if self.mf { -0x7fff } else { -0x8000 };
                    if q > 0x7fff || q < lower {
                        self.interrupt(h, 0);
                    } else {
                        self.regs[AX] = q as u16;
                        self.regs[DX] = r as u16;
                        self.zero_val = (q as u32) & 0xffff;
                    }
                } else {
                    self.interrupt(h, 0);
                }
                self.clkm(t::IDIV_R16, t::IDIV_M16);
            }
        }
    }

    fn op_ff<H: Host>(&mut self, h: &mut H) {
        self.modrm = self.fetch(h);
        let tmp = self.get_rm_word(h) as u32;
        match self.modrm & 0x38 {
            0x00 => {
                let tmp1 = tmp.wrapping_add(1);
                self.over_val = (tmp == 0x7fff) as u32;
                self.set_af(tmp1, tmp, 1);
                self.set_szpf_word(tmp1);
                self.putback_rm_word(h, tmp1 as u16);
                self.clkm(t::INCDEC_R16, t::INCDEC_M16);
            }
            0x08 => {
                let tmp1 = tmp.wrapping_sub(1);
                self.over_val = (tmp == 0x8000) as u32;
                self.set_af(tmp1, tmp, 1);
                self.set_szpf_word(tmp1);
                self.putback_rm_word(h, tmp1 as u16);
                self.clkm(t::INCDEC_R16, t::INCDEC_M16);
            }
            0x10 => {
                let ip = self.ip;
                self.push(h, ip);
                self.ip = tmp as u16;
                self.clkm(t::CALL_R16, t::CALL_M16);
            }
            0x18 => {
                let old_cs = self.sregs[CS];
                self.sregs[CS] = self.get_next_rm_word(h);
                self.push(h, old_cs);
                let ip = self.ip;
                self.push(h, ip);
                self.ip = tmp as u16;
                self.clk(t::CALL_M32);
            }
            0x20 => {
                self.ip = tmp as u16;
                self.clkm(t::JMP_R16, t::JMP_M16);
            }
            0x28 => {
                self.ip = tmp as u16;
                self.sregs[CS] = self.get_next_rm_word(h);
                self.clk(t::JMP_M32);
            }
            0x30 => {
                // PUSH SP (FF F4) pushes the decremented SP, like opcode 54.
                let v = if self.modrm == 0xf4 { (tmp as u16).wrapping_sub(2) } else { tmp as u16 };
                self.push(h, v);
                self.clkm(t::PUSH_R16, t::PUSH_M16);
            }
            _ => {
                // MAME: "FF Pre with unimplemented mod"
                self.icount -= 10;
            }
        }
    }
}
