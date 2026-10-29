//! SingleStepTests hardware-generated CPU tests, run against the 80186 core.
//!
//! The SingleStepTests 80186 repository holds no data yet, so two other
//! hardware-captured suites stand in for it:
//!  * `singlestep_8086`: the 8086 v1 suite (captured from a real 80C86), for
//!    the whole 8086 instruction set, skipping the forms whose behaviour
//!    differs on the 80186 (see `skip_file_8086` / `skip_case_8086`);
//!  * `singlestep_80286_186_ops`: the 80286 real-mode suite, only for the
//!    instructions the 80186 added (PUSHA/POPA, BOUND, PUSH imm, IMUL imm,
//!    shift/rotate imm8, ENTER/LEAVE), which the 286 executes the same way.
//!
//! Fetch the data first (gitignored, ~200 MB):
//!     python3 scripts/fetch_cpu_tests.py
//! then run:
//!     cargo test --release -p icon-core --test singlestep -- --nocapture
//! Without the data each test prints a note and passes.
//! SSTS_VERBOSE=1 prints the first failures of each file in detail;
//! SSTS_ONLY=<prefix> restricts the run to matching opcode files.

use icon_core::cpu::{CpuBus, Regs, I80186};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

struct Ram {
    mem: Vec<u8>,
}

impl CpuBus for Ram {
    fn mem_read8(&mut self, addr: u32) -> u8 {
        self.mem[(addr & 0xfffff) as usize]
    }
    fn mem_write8(&mut self, addr: u32, val: u8) {
        self.mem[(addr & 0xfffff) as usize] = val;
    }
    fn io_read8(&mut self, _port: u16) -> u8 {
        0xff
    }
    fn io_write8(&mut self, _port: u16, _val: u8) {}
}

const NAMES: [&str; 14] = [
    "ax", "bx", "cx", "dx", "cs", "ss", "ds", "es", "sp", "bp", "si", "di", "ip", "flags",
];
const FLAGS: usize = 13;

fn to_regs(v: &[u16; 14]) -> Regs {
    Regs {
        ax: v[0],
        bx: v[1],
        cx: v[2],
        dx: v[3],
        cs: v[4],
        ss: v[5],
        ds: v[6],
        es: v[7],
        sp: v[8],
        bp: v[9],
        si: v[10],
        di: v[11],
        ip: v[12],
        flags: v[13],
    }
}

fn from_regs(r: &Regs) -> [u16; 14] {
    [r.ax, r.bx, r.cx, r.dx, r.cs, r.ss, r.ds, r.es, r.sp, r.bp, r.si, r.di, r.ip, r.flags]
}

struct Test {
    idx: u32,
    init: [u16; 14],
    ram: Vec<(u32, u8)>,
    fin: [u16; 14],
    fram: Vec<(u32, u8)>,
    /// Exception number and linear address of the FLAGS it pushed (286 suite).
    exc: Option<(u8, u32)>,
}

fn nums(s: &str) -> Vec<u32> {
    s.split_ascii_whitespace().map(|x| x.parse().unwrap()).collect()
}

fn pairs(s: &str) -> Vec<(u32, u8)> {
    nums(s).chunks(2).map(|c| (c[0], c[1] as u8)).collect()
}

fn parse(text: &str) -> Vec<Test> {
    let mut out: Vec<Test> = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(l) = lines.next() {
        if l.is_empty() {
            continue;
        }
        let idx: u32 = l[2..].trim().parse().unwrap();
        let i = nums(&lines.next().unwrap()[1..]);
        let ram = pairs(&lines.next().unwrap()[1..]);
        let f = lines.next().unwrap();
        let fram = pairs(&lines.next().unwrap()[1..]);
        let mut exc = None;
        if lines.peek().is_some_and(|l| l.starts_with('X')) {
            let v = nums(&lines.next().unwrap()[1..]);
            exc = Some((v[0] as u8, v[1]));
        }
        let mut init = [0u16; 14];
        for (k, v) in i.iter().enumerate() {
            init[k] = *v as u16;
        }
        let mut fin = init;
        for kv in f[1..].split_ascii_whitespace() {
            let (k, v) = kv.split_once('=').unwrap();
            let pos = NAMES.iter().position(|n| *n == k).unwrap();
            fin[pos] = v.parse::<u32>().unwrap() as u16;
        }
        out.push(Test { idx, init, ram, fin, fram, exc });
    }
    out
}

/// Decoded view of the instruction under test.
struct Insn {
    rep: bool,
    op: u8,
    modrm: u8,
    /// The three bytes following the opcode.
    next: [u8; 3],
}

fn decode(mem: &[u8], cs: u16, ip: u16) -> Insn {
    let at = |ip: u16| mem[((((cs as u32) << 4) + ip as u32) & 0xfffff) as usize];
    let mut ip = ip;
    let mut rep = false;
    loop {
        match at(ip) {
            0x26 | 0x2e | 0x36 | 0x3e | 0xf0 => {}
            0xf2 | 0xf3 => rep = true,
            b => {
                let next = [at(ip.wrapping_add(1)), at(ip.wrapping_add(2)), at(ip.wrapping_add(3))];
                return Insn { rep, op: b, modrm: next[0], next };
            }
        }
        ip = ip.wrapping_add(1);
    }
}

struct Suite {
    dir: &'static str,
    /// Bits of FLAGS that are comparable at all (the 286 clears 12-15).
    flags_valid: u16,
    /// Run until HLT (286 suite) instead of a single instruction.
    until_halt: bool,
    skip_file: fn(&str, &str) -> Option<&'static str>,
    skip_case: fn(&Insn, &Test) -> Option<&'static str>,
}

/// 8086 forms that do not apply to the 80186.
fn skip_file_8086(stem: &str, status: &str) -> Option<&'static str> {
    let op = u8::from_str_radix(&stem[..2], 16).unwrap();
    match op {
        0x0f => return Some("POP CS: invalid opcode (INT 6) on the 80186"),
        0x60..=0x6f => return Some("8086 Jcc aliases; 80186 instructions/INT 6"),
        0xc0 | 0xc1 | 0xc8 | 0xc9 => return Some("8086 RET aliases; 80186 shift-imm/ENTER/LEAVE"),
        _ => {}
    }
    match status {
        "undefined" => Some("undefined 8086 form"),
        "undocumented" if stem != "D6" => Some("undocumented 8086 form (SETMO)"),
        "alias" if stem == "FF.7" => Some("FF /7: undefined"),
        _ => None,
    }
}

fn skip_case_8086(insn: &Insn, t: &Test) -> Option<&'static str> {
    match insn.op {
        // MOV sreg with reg field CS or 4..7: 80186 raises INT 6.
        0x8e if !matches!(insn.modrm & 0x38, 0x00 | 0x10 | 0x18) => Some("MOV CS/invalid sreg"),
        // The 80186 masks shift counts to 5 bits; the 8086 does not.
        0xd2 | 0xd3 if (t.init[2] & 0xff) > 0x1f => Some("shift count > 31"),
        // 8086 microcode quirk: a REP prefix negates the IDIV quotient.
        0xf6 | 0xf7 if insn.rep && insn.modrm & 0x38 == 0x38 => Some("REP IDIV (8086 quirk)"),
        // PCB occupies I/O FF00-FFFF after reset.
        0xec..=0xef if t.init[3] >= 0xff00 => Some("I/O to PCB"),
        _ => None,
    }
}

fn skip_file_286(_stem: &str, status: &str) -> Option<&'static str> {
    match status {
        "undocumented" => Some("undocumented form"),
        _ => None,
    }
}

fn skip_case_286(insn: &Insn, t: &Test) -> Option<&'static str> {
    // The 286 does not wrap at 1 MB (no A20 gate in the test rig).
    if t.ram.iter().chain(t.fram.iter()).any(|&(a, _)| a > 0xfffff) {
        return Some("address above 1 MB");
    }
    // Word accesses at offset FFFF (data or stack) raise #GP/#SS on the 286
    // but wrap within the segment on the 8086/80186. BOUND's INT 5 and the
    // #UD of BOUND with a register operand are kept.
    if let Some((num, _)) = t.exc {
        if !(insn.op == 0x62 && (num == 5 || num == 6)) {
            return Some("286-only exception");
        }
    }
    // ENTER with an odd frame pointer whose display copy reads back words
    // it has just pushed: the 286 reads stale data (internal ordering).
    if insn.op == 0xc8 && insn.next[2] & 0x1f > 1 && t.init[9] & 1 != 0 && t.init[8] & 1 != 0 {
        return Some("ENTER, odd BP/SP, overlapping display copy");
    }
    None
}

fn run_suite(s: &Suite) {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata").join(s.dir);
    let meta = match fs::read_to_string(dir.join("meta.txt")) {
        Ok(m) => m,
        Err(_) => {
            eprintln!("SingleStepTests data not found; run scripts/fetch_cpu_tests.py (skipping)");
            return;
        }
    };
    let verbose = std::env::var("SSTS_VERBOSE").is_ok();
    let only = std::env::var("SSTS_ONLY").ok();
    let mut masks: HashMap<String, (String, u16)> = HashMap::new();
    for l in meta.lines() {
        let mut it = l.split_whitespace();
        let stem = it.next().unwrap().to_string();
        let status = it.next().unwrap().to_string();
        let mask: u32 = it.next().unwrap().parse().unwrap();
        masks.insert(stem, (status, mask as u16));
    }

    let mut stems: Vec<_> = masks.keys().cloned().collect();
    stems.sort();
    let (mut total_pass, mut total_fail, mut total_skip) = (0usize, 0usize, 0usize);
    let mut failing_files = Vec::new();
    let mut mem = vec![0u8; 1 << 20];

    for stem in stems {
        if let Some(o) = &only {
            if !stem.starts_with(o.as_str()) {
                continue;
            }
        }
        let (status, mask) = masks[&stem].clone();
        let mut mask = mask & s.flags_valid;
        if std::env::var("SSTS_CHECK_OF").is_ok() {
            mask |= 0x0800;
        }
        let path = dir.join(format!("{stem}.txt"));
        let Ok(text) = fs::read_to_string(&path) else { continue };
        if let Some(why) = (s.skip_file)(&stem, &status) {
            let n = text.lines().filter(|l| l.starts_with('T')).count();
            total_skip += n;
            eprintln!("{stem:6} skipped ({n} tests): {why}");
            continue;
        }
        let tests = parse(&text);
        let (mut pass, mut fail, mut skip) = (0, 0, 0);
        let mut shown = 0;
        for t in &tests {
            for &(a, v) in &t.ram {
                mem[(a & 0xfffff) as usize] = v;
            }
            let insn = decode(&mem, t.init[4], t.init[12]);
            if (s.skip_case)(&insn, t).is_some() {
                skip += 1;
                for &(a, _) in &t.ram {
                    mem[(a & 0xfffff) as usize] = 0;
                }
                continue;
            }
            let mut bus = Ram { mem: std::mem::take(&mut mem) };
            let mut cpu = I80186::new();
            cpu.set_regs(&to_regs(&t.init));
            let start = (t.init[4], t.init[12]);
            let is_string = matches!(insn.op, 0xa4..=0xa7 | 0xaa..=0xaf);
            for _ in 0..10_000 {
                cpu.step(&mut bus);
                if s.until_halt {
                    if cpu.halted() {
                        break;
                    }
                    continue;
                }
                let r = cpu.regs();
                if !(insn.rep && is_string && (r.cs, r.ip) == start) {
                    break;
                }
            }
            let got = from_regs(&cpu.regs());
            let mut ok = true;
            for (k, &g) in got.iter().enumerate().take(14) {
                let m = if k == FLAGS { mask } else { 0xffff };
                if g & m != t.fin[k] & m {
                    ok = false;
                }
            }

            // FLAGS pushed by an exception: compare only the defined bits.
            // (A divide error on the 8086 pushes flags whose undefined bits
            // the microcode has already clobbered.)
            let mut ram_mask: HashMap<u32, u8> = HashMap::new();
            let mut flag_addr = t.exc.map(|e| e.1);
            let div = matches!(insn.op, 0xf6 | 0xf7) && insn.modrm & 0x30 == 0x30 || insn.op == 0xd4;
            if flag_addr.is_none() && div && t.fin[8] == t.init[8].wrapping_sub(6) {
                let off = t.fin[8].wrapping_add(4);
                flag_addr = Some((((t.fin[5] as u32) << 4) + off as u32) & 0xfffff);
            }
            if let Some(a) = flag_addr {
                ram_mask.insert(a, mask as u8);
                ram_mask.insert(a + 1, (mask >> 8) as u8);
            }
            let mut bad_ram = Vec::new();
            for &(a, v) in &t.fram {
                let m = ram_mask.get(&a).copied().unwrap_or(0xff);
                let g = bus.mem[(a & 0xfffff) as usize];
                if g & m != v & m {
                    ok = false;
                    bad_ram.push((a, g, v));
                }
            }
            if ok {
                pass += 1;
            } else {
                fail += 1;
                if verbose && shown < 3 {
                    shown += 1;
                    eprintln!("--- {stem} idx {} op {:02x} modrm {:02x}", t.idx, insn.op, insn.modrm);
                    for k in 0..14 {
                        let m = if k == FLAGS { mask } else { 0xffff };
                        let flag = if got[k] & m != t.fin[k] & m { " <==" } else { "" };
                        eprintln!(
                            "  {:5} init {:04x} want {:04x} got {:04x}{}",
                            NAMES[k], t.init[k], t.fin[k], got[k], flag
                        );
                    }
                    for (a, g, w) in bad_ram.iter().take(8) {
                        eprintln!("  ram {a:05x}: want {w:02x} got {g:02x}");
                    }
                }
            }
            mem = bus.mem;
            for &(a, _) in t.ram.iter().chain(t.fram.iter()) {
                mem[(a & 0xfffff) as usize] = 0;
            }
        }
        total_pass += pass;
        total_fail += fail;
        total_skip += skip;
        if fail > 0 {
            failing_files.push(format!("{stem}({fail})"));
        }
        eprintln!("{stem:6} pass {pass:5} fail {fail:5} skip {skip:5}");
    }
    eprintln!(
        "{} TOTAL pass {total_pass} fail {total_fail} skipped {total_skip}; failing: {}",
        s.dir,
        failing_files.join(" ")
    );
    assert_eq!(total_fail, 0, "failing files: {}", failing_files.join(" "));
}

#[test]
fn singlestep_8086() {
    run_suite(&Suite {
        dir: "8086",
        flags_valid: 0xffff,
        until_halt: false,
        skip_file: skip_file_8086,
        skip_case: skip_case_8086,
    });
}

#[test]
fn singlestep_80286_186_ops() {
    run_suite(&Suite {
        dir: "80286",
        flags_valid: 0x0fff,
        until_halt: true,
        skip_file: skip_file_286,
        skip_case: skip_case_286,
    });
}
