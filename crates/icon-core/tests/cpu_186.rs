//! Hand-written tests for the 80186 core: arithmetic and flags, multiply /
//! divide, string instructions, BCD adjusts, control transfer, the 80186
//! instruction additions, and the on-chip peripherals (PCB, timers,
//! interrupt controller, DMA).

use icon_core::cpu::{CpuBus, Regs, I80186};

const CF: u16 = 0x0001;
const PF: u16 = 0x0004;
const AF: u16 = 0x0010;
const ZF: u16 = 0x0040;
const SF: u16 = 0x0080;
const IF: u16 = 0x0200;
const OF: u16 = 0x0800;
const ARITH: u16 = CF | PF | AF | ZF | SF | OF;

const CODE_SEG: u16 = 0x1000;
const STACK_SEG: u16 = 0x2000;
const DATA_SEG: u16 = 0x3000;
const EXTRA_SEG: u16 = 0x4000;
const SP0: u16 = 0x1000;

struct Bus {
    mem: Vec<u8>,
    io_writes: Vec<(u16, u8)>,
    io_reads: Vec<u16>,
    io_next: u8,
}

impl Bus {
    fn new() -> Self {
        Bus { mem: vec![0; 1 << 20], io_writes: Vec::new(), io_reads: Vec::new(), io_next: 0x10 }
    }
    fn load(&mut self, addr: u32, bytes: &[u8]) {
        self.mem[addr as usize..addr as usize + bytes.len()].copy_from_slice(bytes);
    }
    fn w16(&mut self, addr: u32, v: u16) {
        self.mem[addr as usize] = v as u8;
        self.mem[addr as usize + 1] = (v >> 8) as u8;
    }
    fn r16(&self, addr: u32) -> u16 {
        self.mem[addr as usize] as u16 | (self.mem[addr as usize + 1] as u16) << 8
    }
    fn set_vector(&mut self, n: u8, seg: u16, off: u16) {
        self.w16(n as u32 * 4, off);
        self.w16(n as u32 * 4 + 2, seg);
    }
}

impl CpuBus for Bus {
    fn mem_read8(&mut self, addr: u32) -> u8 {
        self.mem[(addr & 0xfffff) as usize]
    }
    fn mem_write8(&mut self, addr: u32, val: u8) {
        self.mem[(addr & 0xfffff) as usize] = val;
    }
    fn io_read8(&mut self, port: u16) -> u8 {
        self.io_reads.push(port);
        let v = self.io_next;
        self.io_next = self.io_next.wrapping_add(1);
        v
    }
    fn io_write8(&mut self, port: u16, val: u8) {
        self.io_writes.push((port, val));
    }
}

fn lin(seg: u16, off: u16) -> u32 {
    ((seg as u32) << 4) + off as u32
}

/// Fresh CPU with the conventional test segments; `code` is placed at
/// CODE_SEG:0000 followed by HLT.
fn setup(code: &[u8]) -> (I80186, Bus) {
    let mut bus = Bus::new();
    let mut c = code.to_vec();
    c.push(0xf4);
    bus.load(lin(CODE_SEG, 0), &c);
    let mut cpu = I80186::new();
    cpu.set_regs(&Regs {
        cs: CODE_SEG,
        ss: STACK_SEG,
        ds: DATA_SEG,
        es: EXTRA_SEG,
        sp: SP0,
        flags: 0x0002,
        ..Regs::default()
    });
    (cpu, bus)
}

fn run_to_halt(cpu: &mut I80186, bus: &mut Bus) -> u64 {
    let mut cycles = 0u64;
    for _ in 0..1_000_000 {
        if cpu.halted() {
            return cycles;
        }
        cycles += cpu.step(bus) as u64;
    }
    panic!("did not halt; regs {:x?}", cpu.regs());
}

fn exec(code: &[u8], f: impl FnOnce(&mut Regs)) -> (I80186, Bus) {
    let (mut cpu, mut bus) = setup(code);
    let mut r = cpu.regs();
    f(&mut r);
    cpu.set_regs(&r);
    run_to_halt(&mut cpu, &mut bus);
    (cpu, bus)
}

fn flags(cpu: &I80186) -> u16 {
    cpu.regs().flags & ARITH
}

/// MOV DX,port / MOV AX,val / OUT DX,AX
fn out16(port: u16, val: u16) -> Vec<u8> {
    vec![0xba, port as u8, (port >> 8) as u8, 0xb8, val as u8, (val >> 8) as u8, 0xef]
}

// ---------------------------------------------------------------------------
// Arithmetic and flags
// ---------------------------------------------------------------------------

#[test]
fn add_flags() {
    // MOV AL,7F; ADD AL,1
    let (cpu, _) = exec(&[0xb0, 0x7f, 0x04, 0x01], |_| {});
    assert_eq!(cpu.regs().ax & 0xff, 0x80);
    assert_eq!(flags(&cpu), OF | SF | AF);
    // MOV AL,FF; ADD AL,1
    let (cpu, _) = exec(&[0xb0, 0xff, 0x04, 0x01], |_| {});
    assert_eq!(cpu.regs().ax & 0xff, 0x00);
    assert_eq!(flags(&cpu), CF | ZF | AF | PF);
    // ADD AX,8000 with AX=8000: CF, OF, ZF
    let (cpu, _) = exec(&[0xb8, 0x00, 0x80, 0x05, 0x00, 0x80], |_| {});
    assert_eq!(cpu.regs().ax, 0);
    assert_eq!(flags(&cpu), CF | OF | ZF | PF);
    // ADD r/m16, r16 to memory: ADD [0010],BX
    let (cpu, bus) = exec(&[0xbb, 0x34, 0x12, 0x01, 0x1e, 0x10, 0x00], |_| {});
    assert_eq!(bus.r16(lin(DATA_SEG, 0x10)), 0x1234);
    assert_eq!(flags(&cpu), 0);
}

#[test]
fn sub_cmp_flags() {
    // MOV AL,0; SUB AL,1
    let (cpu, _) = exec(&[0xb0, 0x00, 0x2c, 0x01], |_| {});
    assert_eq!(cpu.regs().ax & 0xff, 0xff);
    assert_eq!(flags(&cpu), CF | SF | AF | PF);
    // MOV AL,80; SUB AL,1 -> 7F, OF
    let (cpu, _) = exec(&[0xb0, 0x80, 0x2c, 0x01], |_| {});
    assert_eq!(cpu.regs().ax & 0xff, 0x7f);
    assert_eq!(flags(&cpu), OF | AF);
    // MOV AX,5; CMP AX,6: AX unchanged, CF SF
    let (cpu, _) = exec(&[0xb8, 0x05, 0x00, 0x3d, 0x06, 0x00], |_| {});
    assert_eq!(cpu.regs().ax, 5);
    assert_eq!(flags(&cpu), CF | SF | AF | PF);
    // CMP equal -> ZF
    let (cpu, _) = exec(&[0xb8, 0x05, 0x00, 0x3d, 0x05, 0x00], |_| {});
    assert_eq!(flags(&cpu), ZF | PF);
}

#[test]
fn adc_sbb() {
    // STC; MOV AL,FE; ADC AL,1 -> 00, CF ZF
    let (cpu, _) = exec(&[0xf9, 0xb0, 0xfe, 0x14, 0x01], |_| {});
    assert_eq!(cpu.regs().ax & 0xff, 0);
    assert_eq!(flags(&cpu), CF | ZF | AF | PF);
    // CLC; ADC AL,1 with AL=FE -> FF
    let (cpu, _) = exec(&[0xf8, 0xb0, 0xfe, 0x14, 0x01], |_| {});
    assert_eq!(cpu.regs().ax & 0xff, 0xff);
    assert_eq!(flags(&cpu) & CF, 0);
    // STC; MOV AX,0; SBB AX,0 -> FFFF, CF
    let (cpu, _) = exec(&[0xf9, 0xb8, 0x00, 0x00, 0x1d, 0x00, 0x00], |_| {});
    assert_eq!(cpu.regs().ax, 0xffff);
    assert_eq!(flags(&cpu), CF | SF | AF | PF);
    // 32-bit add via ADD/ADC: 0001FFFF + 00000001 = 00020000
    let (cpu, _) = exec(
        &[0xb8, 0xff, 0xff, 0xba, 0x01, 0x00, 0x05, 0x01, 0x00, 0x83, 0xd2, 0x00],
        |_| {},
    );
    assert_eq!((cpu.regs().dx, cpu.regs().ax), (0x0002, 0x0000));
}

#[test]
fn inc_dec_neg() {
    // STC; MOV AX,7FFF; INC AX -> 8000, OF, CF preserved
    let (cpu, _) = exec(&[0xf9, 0xb8, 0xff, 0x7f, 0x40], |_| {});
    assert_eq!(cpu.regs().ax, 0x8000);
    assert_eq!(flags(&cpu), CF | OF | SF | AF | PF);
    // MOV AX,8000; DEC AX -> 7FFF OF
    let (cpu, _) = exec(&[0xb8, 0x00, 0x80, 0x48], |_| {});
    assert_eq!(cpu.regs().ax, 0x7fff);
    assert_eq!(flags(&cpu), OF | AF | PF);
    // INC byte in memory: INC BYTE [0020] (FF -> 00)
    let (cpu, bus) = exec(&[0xc6, 0x06, 0x20, 0x00, 0xff, 0xfe, 0x06, 0x20, 0x00], |_| {});
    assert_eq!(bus.mem[lin(DATA_SEG, 0x20) as usize], 0);
    assert_eq!(flags(&cpu), ZF | AF | PF);
    // NEG AL: 0 -> 0, CF=0
    let (cpu, _) = exec(&[0xb0, 0x00, 0xf6, 0xd8], |_| {});
    assert_eq!(flags(&cpu), ZF | PF);
    // NEG AL: 80 -> 80, CF OF
    let (cpu, _) = exec(&[0xb0, 0x80, 0xf6, 0xd8], |_| {});
    assert_eq!(cpu.regs().ax & 0xff, 0x80);
    assert_eq!(flags(&cpu), CF | OF | SF);
    // NEG AX: 1 -> FFFF
    let (cpu, _) = exec(&[0xb8, 0x01, 0x00, 0xf7, 0xd8], |_| {});
    assert_eq!(cpu.regs().ax, 0xffff);
    assert_eq!(flags(&cpu), CF | SF | AF | PF);
}

#[test]
fn logic_clears_cf_of() {
    // STC; MOV AL,F0; AND AL,0F -> 0, ZF, CF cleared
    let (cpu, _) = exec(&[0xf9, 0xb0, 0xf0, 0x24, 0x0f], |_| {});
    assert_eq!(flags(&cpu), ZF | PF);
    // XOR AX,AX
    let (cpu, _) = exec(&[0xb8, 0x34, 0x12, 0x31, 0xc0], |_| {});
    assert_eq!(cpu.regs().ax, 0);
    assert_eq!(flags(&cpu), ZF | PF);
}

// ---------------------------------------------------------------------------
// Multiply / divide
// ---------------------------------------------------------------------------

#[test]
fn mul_imul() {
    // MOV AL,80; MOV BL,2; MUL BL -> 0100, CF OF
    let (cpu, _) = exec(&[0xb0, 0x80, 0xb3, 0x02, 0xf6, 0xe3], |_| {});
    assert_eq!(cpu.regs().ax, 0x0100);
    assert_eq!(cpu.regs().flags & (CF | OF), CF | OF);
    // MUL BX: FFFF*FFFF = FFFE0001
    let (cpu, _) = exec(&[0xb8, 0xff, 0xff, 0xbb, 0xff, 0xff, 0xf7, 0xe3], |_| {});
    assert_eq!((cpu.regs().dx, cpu.regs().ax), (0xfffe, 0x0001));
    assert_eq!(cpu.regs().flags & (CF | OF), CF | OF);
    // MUL with small result: CF=OF=0
    let (cpu, _) = exec(&[0xb8, 0x03, 0x00, 0xbb, 0x04, 0x00, 0xf7, 0xe3], |_| {});
    assert_eq!((cpu.regs().dx, cpu.regs().ax), (0, 12));
    assert_eq!(cpu.regs().flags & (CF | OF), 0);
    // IMUL BL: -2 * 3 = -6 (fits): CF=OF=0
    let (cpu, _) = exec(&[0xb0, 0xfe, 0xb3, 0x03, 0xf6, 0xeb], |_| {});
    assert_eq!(cpu.regs().ax, 0xfffa);
    assert_eq!(cpu.regs().flags & (CF | OF), 0);
    // IMUL BL: 64 * 2 = 128 (does not fit in a signed byte): CF=OF=1
    let (cpu, _) = exec(&[0xb0, 0x40, 0xb3, 0x02, 0xf6, 0xeb], |_| {});
    assert_eq!(cpu.regs().ax, 0x0080);
    assert_eq!(cpu.regs().flags & (CF | OF), CF | OF);
    // IMUL BX: -1000 * 1000 = -1000000 = FFF0BDC0
    let (cpu, _) = exec(&[0xb8, 0x18, 0xfc, 0xbb, 0xe8, 0x03, 0xf7, 0xeb], |_| {});
    assert_eq!((cpu.regs().dx, cpu.regs().ax), (0xfff0, 0xbdc0));
    assert_eq!(cpu.regs().flags & (CF | OF), CF | OF);
}

#[test]
fn div_idiv() {
    // MOV AX,1000; MOV BL,7; DIV BL -> AL=142 AH=6
    let (cpu, _) = exec(&[0xb8, 0xe8, 0x03, 0xb3, 0x07, 0xf6, 0xf3], |_| {});
    assert_eq!(cpu.regs().ax, 0x068e);
    // DX:AX=00010000 / 3 -> AX=5555 DX=1
    let (cpu, _) = exec(&[0xba, 0x01, 0x00, 0xb8, 0x00, 0x00, 0xbb, 0x03, 0x00, 0xf7, 0xf3], |_| {});
    assert_eq!((cpu.regs().ax, cpu.regs().dx), (0x5555, 1));
    // IDIV BL: -7 / 2 -> AL=-3 AH=-1
    let (cpu, _) = exec(&[0xb8, 0xf9, 0xff, 0xb3, 0x02, 0xf6, 0xfb], |_| {});
    assert_eq!(cpu.regs().ax, 0xfffd);
    // IDIV BX: -100000 / 7 = -14285 r -5
    let (cpu, _) = exec(&[0xba, 0xfe, 0xff, 0xb8, 0x60, 0x79, 0xbb, 0x07, 0x00, 0xf7, 0xfb], |_| {});
    assert_eq!((cpu.regs().ax, cpu.regs().dx), ((-14285i16) as u16, (-5i16) as u16));
}

#[test]
fn divide_error_int0() {
    for code in [
        // MOV BL,0; DIV BL
        vec![0xb3, 0x00, 0xf6, 0xf3],
        // MOV AX,1000h; MOV BL,2; DIV BL  (quotient overflow)
        vec![0xb8, 0x00, 0x10, 0xb3, 0x02, 0xf6, 0xf3],
        // MOV DX,8000h; MOV AX,0; MOV BX,1; IDIV BX (overflow)
        vec![0xba, 0x00, 0x80, 0xb8, 0x00, 0x00, 0xbb, 0x01, 0x00, 0xf7, 0xfb],
    ] {
        let (mut cpu, mut bus) = setup(&code);
        bus.set_vector(0, 0x0050, 0x0000);
        bus.load(0x500, &[0xf4]);
        run_to_halt(&mut cpu, &mut bus);
        let r = cpu.regs();
        assert_eq!((r.cs, r.ip), (0x0050, 1), "entered INT 0 handler");
        assert_eq!(r.sp, SP0 - 6);
        let sp = lin(STACK_SEG, r.sp);
        // The 8086/80186 push the address of the next instruction.
        assert_eq!(bus.r16(sp), code.len() as u16);
        assert_eq!(bus.r16(sp + 2), CODE_SEG);
        assert_eq!(bus.r16(sp + 4) & 0xf002, 0xf002);
    }
}

// ---------------------------------------------------------------------------
// String instructions
// ---------------------------------------------------------------------------

#[test]
fn rep_movsb_with_cs_override_both_prefix_orders() {
    for prefix in [[0xf3u8, 0x2e], [0x2e, 0xf3]] {
        // MOV CX,5; MOV SI,40; MOV DI,20; <prefixes> MOVSB
        let mut code = vec![0xb9, 0x05, 0x00, 0xbe, 0x40, 0x00, 0xbf, 0x20, 0x00];
        code.extend_from_slice(&prefix);
        code.push(0xa4);
        let (mut cpu, mut bus) = setup(&code);
        bus.load(lin(CODE_SEG, 0x40), b"HELLO");
        bus.load(lin(DATA_SEG, 0x40), b"wrong");
        run_to_halt(&mut cpu, &mut bus);
        let r = cpu.regs();
        assert_eq!(&bus.mem[lin(EXTRA_SEG, 0x20) as usize..][..5], b"HELLO");
        assert_eq!((r.cx, r.si, r.di), (0, 0x45, 0x25));
    }
}

#[test]
fn rep_movsw_interrupted_resumes_with_prefix() {
    // MOV CX,200; XOR SI,SI; MOV DI,1000; REP CS: MOVSW  (copies 400 bytes)
    let code = [0xb9, 0xc8, 0x00, 0x31, 0xf6, 0xbf, 0x00, 0x10, 0xf3, 0x2e, 0xa5];
    let (mut cpu, mut bus) = setup(&code);
    for i in 0..400u32 {
        bus.mem[(lin(CODE_SEG, 0) + i) as usize] = (i * 7) as u8;
    }
    bus.load(lin(CODE_SEG, 0), &code);
    bus.load(lin(CODE_SEG, code.len() as u16), &[0xf4]);
    let src: Vec<u8> = bus.mem[lin(CODE_SEG, 0) as usize..][..400].to_vec();
    bus.set_vector(2, 0x0060, 0);
    // NMI handler: INC WORD [SS:0] ; IRET
    bus.load(0x600, &[0x36, 0xff, 0x06, 0x00, 0x00, 0xcf]);
    for _ in 0..4 {
        cpu.step(&mut bus);
    }
    // in the middle of the REP now
    assert!(cpu.regs().cx > 0 && cpu.regs().cx < 200);
    cpu.set_nmi(true);
    run_to_halt(&mut cpu, &mut bus);
    assert_eq!(bus.r16(lin(STACK_SEG, 0)), 1, "NMI handler ran once");
    assert_eq!(&bus.mem[lin(EXTRA_SEG, 0x1000) as usize..][..400], &src[..]);
    assert_eq!(cpu.regs().cx, 0);
}

#[test]
fn cmps_scas_lods_stos() {
    // REPE CMPSB: "ABCDE" vs "ABCxE" stops after the mismatch
    let code = [0xb9, 0x05, 0x00, 0xbe, 0x00, 0x00, 0xbf, 0x00, 0x00, 0xf3, 0xa6];
    let (mut cpu, mut bus) = setup(&code);
    bus.load(lin(DATA_SEG, 0), b"ABCDE");
    bus.load(lin(EXTRA_SEG, 0), b"ABCxE");
    run_to_halt(&mut cpu, &mut bus);
    let r = cpu.regs();
    assert_eq!((r.cx, r.si, r.di), (1, 4, 4));
    assert_eq!(r.flags & ZF, 0);
    assert_eq!(r.flags & CF, CF, "'D' < 'x'");

    // REPNE SCASB for 'Q'
    let code = [0xb0, b'Q', 0xb9, 0x10, 0x00, 0xbf, 0x00, 0x00, 0xf2, 0xae];
    let (mut cpu, mut bus) = setup(&code);
    bus.load(lin(EXTRA_SEG, 0), b"xyzQrst");
    run_to_halt(&mut cpu, &mut bus);
    let r = cpu.regs();
    assert_eq!((r.cx, r.di), (16 - 4, 4));
    assert_eq!(r.flags & ZF, ZF);

    // STD; MOV AX,ABCD; MOV CX,3; MOV DI,10; REP STOSW  -> words at 10, 0E, 0C
    let code = [0xfd, 0xb8, 0xcd, 0xab, 0xb9, 0x03, 0x00, 0xbf, 0x10, 0x00, 0xf3, 0xab];
    let (cpu, bus) = exec(&code, |_| {});
    for off in [0x10u16, 0x0e, 0x0c] {
        assert_eq!(bus.r16(lin(EXTRA_SEG, off)), 0xabcd);
    }
    assert_eq!(bus.r16(lin(EXTRA_SEG, 0x0a)), 0);
    assert_eq!(cpu.regs().di, 0x0a);

    // ES: LODSW (override applies to the source)
    let code = [0xbe, 0x08, 0x00, 0x26, 0xad];
    let (mut cpu, mut bus) = setup(&code);
    bus.w16(lin(EXTRA_SEG, 8), 0x1234);
    bus.w16(lin(DATA_SEG, 8), 0x5678);
    run_to_halt(&mut cpu, &mut bus);
    assert_eq!((cpu.regs().ax, cpu.regs().si), (0x1234, 10));

    // REP with CX=0 does nothing
    let (cpu, bus) = exec(&[0xb9, 0x00, 0x00, 0xb0, 0x55, 0xf3, 0xaa], |_| {});
    assert_eq!(bus.mem[lin(EXTRA_SEG, 0) as usize], 0);
    assert_eq!(cpu.regs().di, 0);
}

// ---------------------------------------------------------------------------
// BCD adjusts
// ---------------------------------------------------------------------------

#[test]
fn bcd_adjusts() {
    // MOV AL,15; ADD AL,27; DAA -> 42
    let (cpu, _) = exec(&[0xb0, 0x15, 0x04, 0x27, 0x27], |_| {});
    assert_eq!(cpu.regs().ax & 0xff, 0x42);
    assert_eq!(cpu.regs().flags & CF, 0);
    // MOV AL,99; ADD AL,1; DAA -> 00 CF
    let (cpu, _) = exec(&[0xb0, 0x99, 0x04, 0x01, 0x27], |_| {});
    assert_eq!(cpu.regs().ax & 0xff, 0x00);
    assert_eq!(cpu.regs().flags & (CF | ZF), CF | ZF);
    // MOV AL,42; SUB AL,15; DAS -> 27
    let (cpu, _) = exec(&[0xb0, 0x42, 0x2c, 0x15, 0x2f], |_| {});
    assert_eq!(cpu.regs().ax & 0xff, 0x27);
    // MOV AL,10; SUB AL,20; DAS -> 90 CF
    let (cpu, _) = exec(&[0xb0, 0x10, 0x2c, 0x20, 0x2f], |_| {});
    assert_eq!(cpu.regs().ax & 0xff, 0x90);
    assert_eq!(cpu.regs().flags & CF, CF);
    // MOV AX,0009; ADD AL,3; AAA -> 0102 CF AF
    let (cpu, _) = exec(&[0xb8, 0x09, 0x00, 0x04, 0x03, 0x37], |_| {});
    assert_eq!(cpu.regs().ax, 0x0102);
    assert_eq!(cpu.regs().flags & (CF | AF), CF | AF);
    // MOV AX,0102; SUB AL,3; AAS -> 0009 CF
    let (cpu, _) = exec(&[0xb8, 0x02, 0x01, 0x2c, 0x03, 0x3f], |_| {});
    assert_eq!(cpu.regs().ax, 0x0009);
    assert_eq!(cpu.regs().flags & CF, CF);
    // MOV AL,63; AAM -> 0603
    let (cpu, _) = exec(&[0xb0, 63, 0xd4, 0x0a], |_| {});
    assert_eq!(cpu.regs().ax, 0x0603);
    // MOV AX,0603; AAD -> 003F
    let (cpu, _) = exec(&[0xb8, 0x03, 0x06, 0xd5, 0x0a], |_| {});
    assert_eq!(cpu.regs().ax, 0x003f);
    // AAM 16 (non-decimal base)
    let (cpu, _) = exec(&[0xb0, 0x5a, 0xd4, 0x10], |_| {});
    assert_eq!(cpu.regs().ax, 0x050a);
}

// ---------------------------------------------------------------------------
// Control transfer
// ---------------------------------------------------------------------------

#[test]
fn far_call_ret_and_retf_imm() {
    // CALL 0050:0000
    let (mut cpu, mut bus) = setup(&[0x9a, 0x00, 0x00, 0x50, 0x00]);
    // MOV AX,1234; RETF
    bus.load(0x500, &[0xb8, 0x34, 0x12, 0xcb]);
    run_to_halt(&mut cpu, &mut bus);
    let r = cpu.regs();
    assert_eq!((r.ax, r.cs, r.ip, r.sp), (0x1234, CODE_SEG, 6, SP0));

    // PUSH 1; PUSH 2; CALL FAR 0050:0000; handler RETF 4
    let (mut cpu, mut bus) = setup(&[0x6a, 0x01, 0x6a, 0x02, 0x9a, 0x00, 0x00, 0x50, 0x00]);
    bus.load(0x500, &[0xca, 0x04, 0x00]);
    run_to_halt(&mut cpu, &mut bus);
    assert_eq!(cpu.regs().sp, SP0);

    // Indirect far call through memory: CALL FAR [0030]
    let (mut cpu, mut bus) = setup(&[0xff, 0x1e, 0x30, 0x00]);
    bus.w16(lin(DATA_SEG, 0x30), 0x0002);
    bus.w16(lin(DATA_SEG, 0x32), 0x0050);
    bus.load(0x502, &[0xb8, 0x99, 0x00, 0xcb]);
    run_to_halt(&mut cpu, &mut bus);
    assert_eq!((cpu.regs().ax, cpu.regs().cs, cpu.regs().sp), (0x99, CODE_SEG, SP0));

    // Near CALL / RET
    let (mut cpu, mut bus) = setup(&[0xe8, 0x01, 0x00, 0xf4, 0xb8, 0x77, 0x00, 0xc3]);
    run_to_halt(&mut cpu, &mut bus);
    assert_eq!((cpu.regs().ax, cpu.regs().ip, cpu.regs().sp), (0x77, 4, SP0));
}

#[test]
fn int_and_iret() {
    // STI; STC; INT 21h; HLT
    let (mut cpu, mut bus) = setup(&[0xfb, 0xf9, 0xcd, 0x21]);
    bus.set_vector(0x21, 0x0060, 0x0000);
    // PUSHF; POP BX; MOV AX,5678; IRET
    bus.load(0x600, &[0x9c, 0x5b, 0xb8, 0x78, 0x56, 0xcf]);
    run_to_halt(&mut cpu, &mut bus);
    let r = cpu.regs();
    assert_eq!(r.ax, 0x5678);
    assert_eq!(r.bx & (IF | 0x100), 0, "IF/TF cleared in handler");
    assert_eq!(r.bx & CF, CF);
    assert_eq!(r.flags & (IF | CF), IF | CF, "flags restored by IRET");
    assert_eq!((r.cs, r.ip, r.sp), (CODE_SEG, 5, SP0));

    // INT3 and INTO
    let (mut cpu, mut bus) = setup(&[0xcc]);
    bus.set_vector(3, 0x0060, 0);
    bus.load(0x600, &[0xb8, 0x03, 0x00, 0xcf]);
    run_to_halt(&mut cpu, &mut bus);
    assert_eq!(cpu.regs().ax, 3);
    // MOV AL,7F; ADD AL,1 (OF); INTO
    let (mut cpu, mut bus) = setup(&[0xb0, 0x7f, 0x04, 0x01, 0xce]);
    bus.set_vector(4, 0x0060, 0);
    bus.load(0x600, &[0xbb, 0x04, 0x00, 0xcf]);
    run_to_halt(&mut cpu, &mut bus);
    assert_eq!(cpu.regs().bx, 4);
}

#[test]
fn conditional_jumps() {
    // MOV AL,1; CMP AL,2; JL +2; MOV BL,1; HLT  -> BL stays 0
    let (cpu, _) = exec(&[0xb0, 0x01, 0x3c, 0x02, 0x7c, 0x02, 0xb3, 0x01], |_| {});
    assert_eq!(cpu.regs().bx, 0);
    // LOOP: MOV CX,5; XOR AX,AX; L: INC AX; LOOP L
    let (cpu, _) = exec(&[0xb9, 0x05, 0x00, 0x31, 0xc0, 0x40, 0xe2, 0xfd], |_| {});
    assert_eq!((cpu.regs().ax, cpu.regs().cx), (5, 0));
    // JCXZ taken
    let (cpu, _) = exec(&[0xb9, 0x00, 0x00, 0xe3, 0x02, 0xb3, 0x01], |_| {});
    assert_eq!(cpu.regs().bx, 0);
}

// ---------------------------------------------------------------------------
// 80186 additions
// ---------------------------------------------------------------------------

#[test]
fn pusha_popa() {
    // PUSHA; then clobber everything; POPA
    let code = [
        0x60, 0x31, 0xc0, 0x31, 0xdb, 0x31, 0xc9, 0x31, 0xd2, 0x31, 0xf6, 0x31, 0xff, 0x31, 0xed,
        0x61,
    ];
    let (mut cpu, mut bus) = setup(&code);
    let mut r = cpu.regs();
    (r.ax, r.cx, r.dx, r.bx, r.bp, r.si, r.di) = (1, 2, 3, 4, 5, 6, 7);
    cpu.set_regs(&r);
    // stop right after PUSHA to inspect the frame
    cpu.step(&mut bus);
    let sp = lin(STACK_SEG, SP0 - 16);
    let words: Vec<u16> = (0..8).map(|i| bus.r16(sp + 2 * i)).collect();
    assert_eq!(words, vec![7, 6, 5, SP0, 4, 3, 2, 1], "DI SI BP SP BX DX CX AX");
    run_to_halt(&mut cpu, &mut bus);
    let r = cpu.regs();
    assert_eq!((r.ax, r.cx, r.dx, r.bx, r.sp, r.bp, r.si, r.di), (1, 2, 3, 4, SP0, 5, 6, 7));
}

#[test]
fn bound() {
    // BOUND AX,[0100]  (bounds -5..10)
    let code = [0x62, 0x06, 0x00, 0x01];
    for (ax, trap) in [(3u16, false), (10, false), ((-5i16) as u16, false), (11, true), ((-6i16) as u16, true)] {
        let (mut cpu, mut bus) = setup(&code);
        bus.w16(lin(DATA_SEG, 0x100), (-5i16) as u16);
        bus.w16(lin(DATA_SEG, 0x102), 10);
        bus.set_vector(5, 0x0060, 0);
        bus.load(0x600, &[0xf4]);
        let mut r = cpu.regs();
        r.ax = ax;
        cpu.set_regs(&r);
        run_to_halt(&mut cpu, &mut bus);
        let r = cpu.regs();
        if trap {
            assert_eq!(r.cs, 0x0060, "ax={ax:04x}");
            // INT 5 is a fault: CS:IP points at the BOUND instruction.
            assert_eq!(bus.r16(lin(STACK_SEG, r.sp)), 0);
        } else {
            assert_eq!((r.cs, r.ip), (CODE_SEG, 5), "ax={ax:04x}");
        }
    }
}

#[test]
fn push_immediate_and_push_sp() {
    // PUSH 1234h; PUSH -2 (imm8 sign-extended); PUSH SP
    let (cpu, bus) = exec(&[0x68, 0x34, 0x12, 0x6a, 0xfe, 0x54], |_| {});
    let base = lin(STACK_SEG, 0);
    assert_eq!(bus.r16(base + SP0 as u32 - 2), 0x1234);
    assert_eq!(bus.r16(base + SP0 as u32 - 4), 0xfffe);
    // PUSH SP stores the decremented value on the 8086/80186
    assert_eq!(bus.r16(base + SP0 as u32 - 6), SP0 - 6);
    assert_eq!(cpu.regs().sp, SP0 - 6);
}

#[test]
fn imul_immediate() {
    // MOV BX,100; IMUL AX,BX,-3
    let (cpu, _) = exec(&[0xbb, 0x64, 0x00, 0x6b, 0xc3, 0xfd], |_| {});
    assert_eq!(cpu.regs().ax, (-300i16) as u16);
    assert_eq!(cpu.regs().flags & (CF | OF), 0);
    // MOV BX,10h; IMUL CX,BX,1000h -> 10000h truncated, CF OF
    let (cpu, _) = exec(&[0xbb, 0x10, 0x00, 0x69, 0xcb, 0x00, 0x10], |_| {});
    assert_eq!(cpu.regs().cx, 0);
    assert_eq!(cpu.regs().flags & (CF | OF), CF | OF);
    // IMUL DX,[0040],7 (memory source)
    let (mut cpu, mut bus) = setup(&[0x6b, 0x16, 0x40, 0x00, 0x07]);
    bus.w16(lin(DATA_SEG, 0x40), 6);
    run_to_halt(&mut cpu, &mut bus);
    assert_eq!(cpu.regs().dx, 42);
}

#[test]
fn ins_outs() {
    // MOV DX,80h; MOV DI,10h; MOV CX,4; REP INSB
    let (mut cpu, mut bus) = setup(&[0xba, 0x80, 0x00, 0xbf, 0x10, 0x00, 0xb9, 0x04, 0x00, 0xf3, 0x6c]);
    bus.io_next = 0xa0;
    run_to_halt(&mut cpu, &mut bus);
    assert_eq!(&bus.mem[lin(EXTRA_SEG, 0x10) as usize..][..4], &[0xa0, 0xa1, 0xa2, 0xa3]);
    assert_eq!(bus.io_reads, vec![0x80; 4]);
    assert_eq!((cpu.regs().cx, cpu.regs().di), (0, 0x14));

    // MOV DX,90h; MOV SI,20h; MOV CX,2; REP CS: OUTSW
    let (mut cpu, mut bus) = setup(&[0xba, 0x90, 0x00, 0xbe, 0x20, 0x00, 0xb9, 0x02, 0x00, 0xf3, 0x2e, 0x6f]);
    bus.load(lin(CODE_SEG, 0x20), &[0x11, 0x22, 0x33, 0x44]);
    run_to_halt(&mut cpu, &mut bus);
    assert_eq!(bus.io_writes, vec![(0x90, 0x11), (0x91, 0x22), (0x90, 0x33), (0x91, 0x44)]);
    assert_eq!(cpu.regs().si, 0x24);

    // INSW single, no REP
    let (mut cpu, mut bus) = setup(&[0xba, 0x70, 0x00, 0xbf, 0x00, 0x00, 0x6d]);
    bus.io_next = 0x01;
    run_to_halt(&mut cpu, &mut bus);
    assert_eq!(bus.r16(lin(EXTRA_SEG, 0)), 0x0201);
    assert_eq!(cpu.regs().di, 2);
}

#[test]
fn shifts_by_immediate_and_count_masking() {
    // MOV AL,11h; SHL AL,3
    let (cpu, _) = exec(&[0xb0, 0x11, 0xc0, 0xe0, 0x03], |_| {});
    assert_eq!(cpu.regs().ax & 0xff, 0x88);
    // MOV AX,1234h; SHR AX,4
    let (cpu, _) = exec(&[0xb8, 0x34, 0x12, 0xc1, 0xe8, 0x04], |_| {});
    assert_eq!(cpu.regs().ax, 0x0123);
    // ROL AX,8
    let (cpu, _) = exec(&[0xb8, 0x34, 0x12, 0xc1, 0xc0, 0x08], |_| {});
    assert_eq!(cpu.regs().ax, 0x3412);
    // SAR AX,15 (negative)
    let (cpu, _) = exec(&[0xb8, 0x00, 0x80, 0xc1, 0xf8, 0x0f], |_| {});
    assert_eq!(cpu.regs().ax, 0xffff);
    // Counts are masked to 5 bits: SHL AL,33 == SHL AL,1
    let (cpu, _) = exec(&[0xb0, 0x41, 0xc0, 0xe0, 0x21], |_| {});
    assert_eq!(cpu.regs().ax & 0xff, 0x82);
    // MOV CL,33; SHL AX,CL == SHL AX,1
    let (cpu, _) = exec(&[0xb8, 0x01, 0x00, 0xb1, 0x21, 0xd3, 0xe0], |_| {});
    assert_eq!(cpu.regs().ax, 2);
    // MOV CL,32: count masks to 0, nothing changes (flags too)
    let (cpu, _) = exec(&[0xf9, 0xb8, 0x01, 0x00, 0xb1, 0x20, 0xd3, 0xe0], |_| {});
    assert_eq!(cpu.regs().ax, 1);
    assert_eq!(cpu.regs().flags & CF, CF);
    // RCL by immediate through carry: STC; MOV AL,0; RCL AL,1 -> 1
    let (cpu, _) = exec(&[0xf9, 0xb0, 0x00, 0xc0, 0xd0, 0x01], |_| {});
    assert_eq!(cpu.regs().ax & 0xff, 1);
    // shift a memory word: SHL WORD [0050],2
    let (mut cpu, mut bus) = setup(&[0xc1, 0x26, 0x50, 0x00, 0x02]);
    bus.w16(lin(DATA_SEG, 0x50), 0x1001);
    run_to_halt(&mut cpu, &mut bus);
    assert_eq!(bus.r16(lin(DATA_SEG, 0x50)), 0x4004);
}

#[test]
fn enter_leave() {
    // MOV BP,0F00h; ENTER 8,0; then LEAVE
    let (mut cpu, mut bus) = setup(&[0xbd, 0x00, 0x0f, 0xc8, 0x08, 0x00, 0x00, 0xc9]);
    cpu.step(&mut bus);
    cpu.step(&mut bus);
    let r = cpu.regs();
    assert_eq!((r.bp, r.sp), (SP0 - 2, SP0 - 2 - 8));
    assert_eq!(bus.r16(lin(STACK_SEG, SP0 - 2)), 0x0f00);
    run_to_halt(&mut cpu, &mut bus);
    let r = cpu.regs();
    assert_eq!((r.bp, r.sp), (0x0f00, SP0));

    // Nested: MOV BP,0F00h; ENTER 4,3
    let (mut cpu, mut bus) = setup(&[0xbd, 0x00, 0x0f, 0xc8, 0x04, 0x00, 0x03]);
    bus.w16(lin(STACK_SEG, 0x0efe), 0xaaaa);
    bus.w16(lin(STACK_SEG, 0x0efc), 0xbbbb);
    run_to_halt(&mut cpu, &mut bus);
    let r = cpu.regs();
    let frame = SP0 - 2;
    assert_eq!(r.bp, frame);
    assert_eq!(bus.r16(lin(STACK_SEG, frame)), 0x0f00, "saved BP");
    assert_eq!(bus.r16(lin(STACK_SEG, frame - 2)), 0xaaaa, "display[1]");
    assert_eq!(bus.r16(lin(STACK_SEG, frame - 4)), 0xbbbb, "display[2]");
    assert_eq!(bus.r16(lin(STACK_SEG, frame - 6)), frame, "current frame");
    assert_eq!(r.sp, frame - 6 - 4);
}

#[test]
fn invalid_opcodes_trap_int6() {
    for code in [vec![0x90, 0x0f], vec![0x90, 0x63], vec![0x90, 0x8e, 0xc8], vec![0x90, 0x62, 0xc0]] {
        let (mut cpu, mut bus) = setup(&code);
        bus.set_vector(6, 0x0060, 0);
        bus.load(0x600, &[0xf4]);
        run_to_halt(&mut cpu, &mut bus);
        let r = cpu.regs();
        assert_eq!(r.cs, 0x0060, "code {code:02x?}");
        // Saved CS:IP points at the offending instruction.
        assert_eq!(bus.r16(lin(STACK_SEG, r.sp)), 1);
        assert_eq!(bus.r16(lin(STACK_SEG, r.sp + 2)), CODE_SEG);
    }
}

#[test]
fn esc_nop_or_trap() {
    // ESC with memory operand + disp16 is a NOP that consumes the ModRM bytes:
    // FADD DWORD [1234h]; MOV AX,5
    let (cpu, _) = exec(&[0xd8, 0x06, 0x34, 0x12, 0xb8, 0x05, 0x00], |_| {});
    assert_eq!(cpu.regs().ax, 5);
    // FNSTSW AX (DF E0) leaves AX alone without a coprocessor
    let (cpu, _) = exec(&[0xb8, 0x5a, 0x5a, 0xdf, 0xe0], |_| {});
    assert_eq!(cpu.regs().ax, 0x5a5a);

    // Set RELREG.ET (bit 15): ESC traps to INT 7 with CS:IP at the ESC.
    let mut code = out16(0xfffe, 0x80ff);
    let esc_at = code.len() as u16;
    code.extend_from_slice(&[0x26, 0xd9, 0xc0]); // ES: FLD ST(0)
    let (mut cpu, mut bus) = setup(&code);
    bus.set_vector(7, 0x0060, 0);
    bus.load(0x600, &[0xf4]);
    run_to_halt(&mut cpu, &mut bus);
    let r = cpu.regs();
    assert_eq!(cpu.relreg(), 0x80ff);
    assert_eq!(r.cs, 0x0060);
    assert_eq!(bus.r16(lin(STACK_SEG, r.sp)), esc_at);
}

#[test]
fn lock_and_segment_prefixes() {
    // LOCK ES: MOV AX,[0010]
    let (mut cpu, mut bus) = setup(&[0xf0, 0x26, 0xa1, 0x10, 0x00]);
    bus.w16(lin(EXTRA_SEG, 0x10), 0xbeef);
    let c = cpu.step(&mut bus);
    assert!(c > 0);
    assert_eq!(cpu.regs().ax, 0xbeef, "prefixes and opcode run as one step");
    // SS: override on [BX]
    let (mut cpu, mut bus) = setup(&[0xbb, 0x20, 0x00, 0x36, 0x8b, 0x07]);
    bus.w16(lin(STACK_SEG, 0x20), 0x1111);
    run_to_halt(&mut cpu, &mut bus);
    assert_eq!(cpu.regs().ax, 0x1111);
    // [BP] defaults to SS, DS: override switches it
    let (mut cpu, mut bus) = setup(&[0xbd, 0x20, 0x00, 0x3e, 0x8b, 0x46, 0x00]);
    bus.w16(lin(DATA_SEG, 0x20), 0x2222);
    run_to_halt(&mut cpu, &mut bus);
    assert_eq!(cpu.regs().ax, 0x2222);
}

#[test]
fn word_access_wraps_within_segment() {
    // MOV AX,[FFFF]: low byte from DS:FFFF, high byte from DS:0000
    let (mut cpu, mut bus) = setup(&[0xa1, 0xff, 0xff]);
    bus.mem[lin(DATA_SEG, 0xffff) as usize] = 0x34;
    bus.mem[lin(DATA_SEG, 0) as usize] = 0x12;
    bus.mem[lin(DATA_SEG, 0) as usize + 0x10000] = 0x99;
    run_to_halt(&mut cpu, &mut bus);
    assert_eq!(cpu.regs().ax, 0x1234);
}

// ---------------------------------------------------------------------------
// Peripheral control block
// ---------------------------------------------------------------------------

#[test]
fn reset_state() {
    let mut cpu = I80186::new();
    let r = cpu.regs();
    assert_eq!((r.cs, r.ip), (0xffff, 0));
    assert_eq!(cpu.pc(), 0xffff0);
    assert_eq!(r.flags, 0xf002);
    assert_eq!(cpu.relreg(), 0x20ff);
    assert_eq!(cpu.chip_selects().umcs, 0xfffb);
    assert!(!cpu.halted());
    assert!(cpu.tmrout(0) && cpu.tmrout(1));
    cpu.reset();
    assert_eq!(cpu.pc(), 0xffff0);
}

#[test]
fn pcb_io_is_internal() {
    // MOV DX,FFFE; IN AX,DX (RELREG) -> BX; MOV DX,FFA0; IN AX,DX (UMCS) -> CX
    // MOV DX,FFA1; IN AL,DX (high byte of UMCS)
    let mut code = vec![0xba, 0xfe, 0xff, 0xed, 0x89, 0xc3, 0xba, 0xa0, 0xff, 0xed, 0x89, 0xc1];
    code.extend_from_slice(&[0xba, 0xa1, 0xff, 0xec]);
    let (cpu, bus) = exec(&code, |_| {});
    assert_eq!(cpu.regs().bx, 0x20ff);
    assert_eq!(cpu.regs().cx, 0xfffb);
    assert_eq!(cpu.regs().ax & 0xff, 0xff);
    assert!(bus.io_reads.is_empty(), "PCB reads never reach the bus");

    // Chip-select programming
    let mut code = out16(0xffa2, 0x07f8); // LMCS
    code.extend(out16(0xffa4, 0x0040)); // PACS
    code.extend(out16(0xffa6, 0x81f8)); // MMCS
    code.extend(out16(0xffa8, 0x80b8)); // MPCS
    code.extend(out16(0xffa0, 0xf038)); // UMCS
    let (cpu, bus) = exec(&code, |_| {});
    let cs = cpu.chip_selects();
    assert_eq!(cs.lmcs, 0x07f8);
    assert_eq!(cs.pacs, 0x0078);
    assert_eq!(cs.mmcs, 0x81f8);
    assert_eq!(cs.mpcs, 0x80b8);
    assert_eq!(cs.umcs, 0xf038);
    assert!(bus.io_writes.is_empty(), "PCB writes never reach the bus");

    // OUT DX,AL to the PCB writes AH:AL as a word (80186 internal bus)
    let code = [0xba, 0xa2, 0xff, 0xb8, 0xf8, 0x07, 0xee];
    let (cpu, _) = exec(&code, |_| {});
    assert_eq!(cpu.chip_selects().lmcs, 0x07f8);
}

#[test]
fn pcb_relocation() {
    // Move the PCB to I/O 1000h-10FFh; then FFxx goes to the bus.
    let mut code = out16(0xfffe, 0x0010);
    // MOV DX,10FEh; IN AX,DX -> BX
    code.extend_from_slice(&[0xba, 0xfe, 0x10, 0xed, 0x89, 0xc3]);
    // MOV DX,FFA0h; IN AL,DX (now external)
    code.extend_from_slice(&[0xba, 0xa0, 0xff, 0xec]);
    let (cpu, bus) = exec(&code, |_| {});
    assert_eq!(cpu.relreg(), 0x0010);
    assert_eq!(cpu.regs().bx, 0x0010);
    assert_eq!(bus.io_reads, vec![0xffa0]);

    // Memory-mapped PCB: RELREG = M/IO (1000h) | A19..A8 (E00h) -> E0000-E00FF
    let mut code = out16(0xfffe, 0x1e00);
    // MOV AX,E000h; MOV ES,AX; MOV AX,ES:[00A0] (UMCS)
    code.extend_from_slice(&[0xb8, 0x00, 0xe0, 0x8e, 0xc0, 0x26, 0xa1, 0xa0, 0x00]);
    let (cpu, bus) = exec(&code, |_| {});
    assert_eq!(cpu.regs().ax, 0xfffb);
    assert_eq!(bus.mem[0xe00a0], 0);
}

/// Vector `n` -> handler at 0000:`addr` that increments the word at
/// 0000:`counter`, issues a specific EOI for `eoi`, and IRETs.
fn install_counting_handler(bus: &mut Bus, n: u8, addr: u16, counter: u16, eoi: u16) {
    bus.set_vector(n, 0, addr);
    let mut h = vec![0x2e, 0xff, 0x06, counter as u8, (counter >> 8) as u8]; // INC WORD CS:[counter]
    h.extend(out16(0xff22, eoi));
    h.push(0xcf);
    bus.load(addr as u32, &h);
}

#[test]
fn timer0_interrupts_through_pic() {
    let mut code = out16(0xff52, 100); // T0CMPA
    code.extend(out16(0xff56, 0xe001)); // T0CON: EN INH INT CONT
    code.extend(out16(0xff32, 0x0000)); // TCUCON: unmasked, priority 0
    code.extend_from_slice(&[0xfb, 0xeb, 0xfe]); // STI; JMP $
    let (mut cpu, mut bus) = setup(&code);
    install_counting_handler(&mut bus, 8, 0x700, 0x800, 0x0008);

    let mut cycles = 0u64;
    while cycles < 40_000 {
        cycles += cpu.step(&mut bus) as u64;
    }
    // 100 counts * 4 clocks = one interrupt per 400 clocks
    let n = bus.r16(0x800);
    assert!((95..=100).contains(&n), "timer interrupts: {n}");
    // timer still counting, max-count bit set
    assert!(cpu.pcb_peek(0x2b) & 0x0020 != 0);
    assert!(cpu.pcb_peek(0x28) < 100);
    // nothing left in service
    assert_eq!(cpu.pcb_peek(0x16), 0);
}

#[test]
fn timer_masked_does_not_interrupt_and_poll() {
    let mut code = out16(0xff52, 10);
    code.extend(out16(0xff56, 0xe001));
    // TCUCON left masked (reset value 000F)
    code.extend_from_slice(&[0xfb, 0xeb, 0xfe]);
    let (mut cpu, mut bus) = setup(&code);
    install_counting_handler(&mut bus, 8, 0x700, 0x800, 0x0008);
    let mut cycles = 0u64;
    while cycles < 5_000 {
        cycles += cpu.step(&mut bus) as u64;
    }
    assert_eq!(bus.r16(0x800), 0);
    // Interrupt status shows timer 0 pending, request register bit 0
    assert_eq!(cpu.pcb_peek(0x18) & 1, 1);
    assert_eq!(cpu.pcb_peek(0x17) & 1, 1);
}

#[test]
fn timer2_prescales_timer0() {
    let mut code = out16(0xff62, 10); // T2CMPA
    code.extend(out16(0xff66, 0xc001)); // T2CON: EN INH CONT
    code.extend(out16(0xff52, 5)); // T0CMPA
    code.extend(out16(0xff56, 0xe009)); // T0CON: EN INH INT P CONT
    code.extend(out16(0xff32, 0x0000));
    code.extend_from_slice(&[0xfb, 0xeb, 0xfe]);
    let (mut cpu, mut bus) = setup(&code);
    install_counting_handler(&mut bus, 8, 0x700, 0x800, 0x0008);
    let mut cycles = 0u64;
    while cycles < 20_000 {
        cycles += cpu.step(&mut bus) as u64;
    }
    // timer 2 elapses every 40 clocks, timer 0 every 5 of those = 200 clocks
    let n = bus.r16(0x800);
    assert!((95..=100).contains(&n), "prescaled timer interrupts: {n}");
}

#[test]
fn timer1_external_clock_and_tmrout() {
    let mut code = out16(0xff5a, 3); // T1CMPA
    code.extend(out16(0xff5c, 2)); // T1CMPB
    code.extend(out16(0xff5e, 0xc007)); // T1CON: EN INH EXT ALT CONT
    let (mut cpu, mut bus) = setup(&code);
    run_to_halt(&mut cpu, &mut bus);
    assert_eq!(cpu.pcb_peek(0x2c), 0, "no internal counting in EXT mode");
    for _ in 0..3 {
        cpu.set_tmrin(1, true);
        cpu.set_tmrin(1, false);
    }
    // max count A reached: output low while counting B
    assert!(!cpu.tmrout(1));
    for _ in 0..2 {
        cpu.set_tmrin(1, true);
        cpu.set_tmrin(1, false);
    }
    assert!(cpu.tmrout(1));
}

#[test]
fn external_int0_and_nmi() {
    let mut code = out16(0xff38, 0x0000); // I0CON: edge, unmasked, priority 0
    code.extend_from_slice(&[0xfb, 0xeb, 0xfe]);
    let (mut cpu, mut bus) = setup(&code);
    install_counting_handler(&mut bus, 0x0c, 0x700, 0x800, 0x000c);
    for _ in 0..50 {
        cpu.step(&mut bus);
    }
    cpu.set_int_line(0, true);
    for _ in 0..200 {
        cpu.step(&mut bus);
    }
    assert_eq!(bus.r16(0x800), 1, "edge-triggered: one interrupt while held high");
    cpu.set_int_line(0, false);
    cpu.set_int_line(0, true);
    for _ in 0..200 {
        cpu.step(&mut bus);
    }
    assert_eq!(bus.r16(0x800), 2);

    // NMI ignores IF
    let (mut cpu, mut bus) = setup(&[0xfa, 0xeb, 0xfe]);
    bus.set_vector(2, 0, 0x900);
    bus.load(0x900, &[0x2e, 0xff, 0x06, 0x02, 0x08, 0xcf]);
    for _ in 0..10 {
        cpu.step(&mut bus);
    }
    cpu.set_nmi(true);
    for _ in 0..50 {
        cpu.step(&mut bus);
    }
    cpu.set_nmi(false);
    assert_eq!(bus.r16(0x802), 1);
}

#[test]
fn hlt_wakes_on_interrupt() {
    let mut code = out16(0xff52, 50);
    code.extend(out16(0xff56, 0xe001));
    code.extend(out16(0xff32, 0x0000));
    code.extend_from_slice(&[0xfb, 0xf4, 0xeb, 0xfd]); // STI; L: HLT; JMP L
    let (mut cpu, mut bus) = setup(&code);
    install_counting_handler(&mut bus, 8, 0x700, 0x800, 0x8000); // non-specific EOI
    let mut halted_seen = false;
    let mut cycles = 0u64;
    while cycles < 10_000 {
        cycles += cpu.step(&mut bus) as u64;
        halted_seen |= cpu.halted();
    }
    assert!(halted_seen);
    let n = bus.r16(0x800);
    assert!((45..=50).contains(&n), "wakeups: {n}");
}

#[test]
fn dma_memory_to_memory_unsynchronised() {
    let mut code = out16(0xffc0, 0x5000); // D0 source low
    code.extend(out16(0xffc2, 0x0000)); // source high
    code.extend(out16(0xffc4, 0x6000)); // dest low
    code.extend(out16(0xffc6, 0x0000)); // dest high
    code.extend(out16(0xffc8, 8)); // count (words)
    // DEST mem, inc; SRC mem, inc; no sync; ST|CHG; word
    code.extend(out16(0xffca, 0xb407));
    let (mut cpu, mut bus) = setup(&code);
    for i in 0..16u32 {
        bus.mem[0x5000 + i as usize] = 0x40 + i as u8;
    }
    run_to_halt(&mut cpu, &mut bus);
    let expect: Vec<u8> = (0..16).map(|i| 0x40 + i).collect();
    assert_eq!(&bus.mem[0x6000..0x6010], &expect[..]);
    assert_eq!(bus.mem[0x6010], 0);
    assert_eq!(cpu.pcb_peek(0x64), 0, "count exhausted");
    assert_eq!(cpu.pcb_peek(0x65) & 2, 0, "channel stopped");
    assert_eq!(cpu.pcb_peek(0x62), 0x6010);
}

#[test]
fn dma_drq_from_io_with_terminal_count_interrupt() {
    let mut code = out16(0xffc0, 0x0040); // source: I/O port 40h
    code.extend(out16(0xffc2, 0x0000));
    code.extend(out16(0xffc4, 0x7000)); // dest: memory 07000h
    code.extend(out16(0xffc6, 0x0000));
    code.extend(out16(0xffc8, 4)); // count
    code.extend(out16(0xff34, 0x0000)); // DMA0 interrupt control: unmasked
    // DEST mem inc; SRC I/O fixed; TC; INT; source sync; ST|CHG; byte
    code.extend(out16(0xffca, 0xa346));
    code.extend_from_slice(&[0xfb, 0xeb, 0xfe]);
    let (mut cpu, mut bus) = setup(&code);
    install_counting_handler(&mut bus, 0x0a, 0x700, 0x800, 0x000a);
    bus.io_next = 0x61;
    for _ in 0..40 {
        cpu.step(&mut bus);
    }
    assert!(bus.io_reads.is_empty(), "no transfer without DRQ");
    cpu.set_drq(0, true);
    for _ in 0..100 {
        cpu.step(&mut bus);
    }
    cpu.set_drq(0, false);
    assert_eq!(&bus.mem[0x7000..0x7005], &[0x61, 0x62, 0x63, 0x64, 0x00]);
    assert_eq!(bus.io_reads, vec![0x40; 4]);
    assert_eq!(bus.r16(0x800), 1, "terminal-count interrupt");
}

#[test]
fn step_reports_plausible_cycles() {
    let (mut cpu, mut bus) = setup(&[0x90, 0xb8, 0x01, 0x00, 0xf7, 0xe3, 0xf6, 0xf3, 0xcd, 0x21]);
    bus.set_vector(0x21, 0x0060, 0);
    bus.load(0x600, &[0xf4]);
    let nop = cpu.step(&mut bus);
    let mov = cpu.step(&mut bus);
    let mul = cpu.step(&mut bus);
    assert_eq!((nop, mov), (2, 4));
    assert!((26..=41).contains(&mul), "MUL r16: {mul}");
    cpu.step(&mut bus); // DIV BL (BL=0 -> INT 0, vector 0 = 0000:0000)
    let mut r = cpu.regs();
    (r.cs, r.ip) = (CODE_SEG, 8);
    cpu.set_regs(&r);
    let int = cpu.step(&mut bus);
    assert!((45..=60).contains(&int), "INT n: {int}");
}
