//! Headless runner for bring-up: boot the ROM, then report where execution
//! ended up, which I/O ports were touched but not emulated, and dump RAM or
//! a guessed frame buffer.

use icon_core::devices::nvram::presets;
use icon_core::machine::{Config, Machine, CPU_HZ, STRAPS_NORMAL};
use std::collections::HashMap;
use std::io::Write;

struct Args {
    rom: String,
    preset: String,
    seconds: f64,
    ram_kb: u32,
    straps: u8,
    trace_last: usize,
    dump_ram: Option<String>,
    screen: Option<String>,
}

fn parse_num(s: &str) -> u32 {
    if let Some(h) = s.strip_prefix("0x") {
        u32::from_str_radix(h, 16).expect("hex number")
    } else {
        s.parse().expect("number")
    }
}

fn parse_args() -> Args {
    let mut a = Args {
        rom: format!(
            "{}/repos/icon_icon2_research/firmware/ICON1/icon1_rom_combined.bin",
            std::env::var("HOME").unwrap_or_default()
        ),
        preset: "ws".into(),
        seconds: 5.0,
        ram_kb: 512,
        straps: STRAPS_NORMAL,
        trace_last: 32,
        dump_ram: None,
        screen: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        let mut v = || it.next().unwrap_or_else(|| panic!("missing value for {k}"));
        match k.as_str() {
            "--rom" => a.rom = v(),
            "--preset" => a.preset = v(),
            "--seconds" => a.seconds = v().parse().unwrap(),
            "--ram-kb" => a.ram_kb = parse_num(&v()),
            "--straps" => a.straps = parse_num(&v()) as u8,
            "--trace-last" => a.trace_last = parse_num(&v()) as usize,
            "--dump-ram" => a.dump_ram = Some(v()),
            "--screen" => a.screen = Some(v()),
            "-h" | "--help" => {
                println!("icon-cli [--rom F] [--preset ws|fs] [--seconds S] [--ram-kb N] [--straps 0x1F]\n         [--trace-last N] [--dump-ram F] [--screen F.ppm]");
                std::process::exit(0);
            }
            _ => panic!("unknown argument {k}"),
        }
    }
    a
}

fn main() {
    let args = parse_args();
    let rom = std::fs::read(&args.rom).expect("read ROM");
    let nvram = match args.preset.as_str() {
        "ws" => presets::workstation(),
        "fs" => presets::fileserver(),
        p => panic!("unknown preset {p}"),
    };
    let mut m = Machine::new(Config { rom, ram_kb: args.ram_kb, nvram, straps: args.straps });

    let total = (args.seconds * CPU_HZ as f64) as u64;
    let mut ring: Vec<u32> = Vec::with_capacity(args.trace_last);
    let mut hot: HashMap<u32, u64> = HashMap::new();
    let mut instr = 0u64;
    while m.board.cycles < total {
        let pc = m.cpu.pc();
        if ring.len() == args.trace_last {
            ring.remove(0);
        }
        ring.push(pc);
        *hot.entry(pc).or_insert(0) += 1;
        m.step();
        instr += 1;
        if m.cpu.halted() {
            println!("CPU halted at {pc:05X}");
            break;
        }
    }

    let r = m.cpu.regs();
    println!(
        "ran {instr} instructions, {} cycles ({:.2} s emulated)",
        m.board.cycles,
        m.board.cycles as f64 / CPU_HZ as f64
    );
    println!(
        "CS:IP={:04X}:{:04X}  AX={:04X} BX={:04X} CX={:04X} DX={:04X} SI={:04X} DI={:04X} BP={:04X} SP={:04X} DS={:04X} ES={:04X} SS={:04X} FL={:04X}",
        r.cs, r.ip, r.ax, r.bx, r.cx, r.dx, r.si, r.di, r.bp, r.sp, r.ds, r.es, r.ss, r.flags
    );
    // ROM file offsets are linear - F8000.
    let rom_off = |pc: u32| if pc >= 0xF8000 { format!(" (rom {:04X})", pc - 0xF8000) } else { String::new() };
    println!("last {} PCs:", ring.len());
    for pc in &ring {
        println!("  {pc:05X}{}", rom_off(*pc));
    }
    let mut hv: Vec<_> = hot.into_iter().collect();
    hv.sort_by(|a, b| b.1.cmp(&a.1));
    println!("hottest PCs:");
    for (pc, n) in hv.iter().take(8) {
        println!("  {pc:05X}{} x{n}", rom_off(*pc));
    }
    println!("FA88 (beeper/LED) edges: {}", m.board.fa88_edges);
    println!("CRTC regs: {:02X?}", &m.board.crtc.regs[..10]);
    println!("palette: {:?}  NVRAM: {:04X?}", m.board.palette, m.board.nvram.words);
    if !m.board.unknown_io.is_empty() {
        println!("unemulated I/O ports: {:04X?}", m.board.unknown_io);
    }
    if let Some(p) = args.dump_ram {
        std::fs::write(&p, &m.board.ram).unwrap();
        println!("RAM written to {p}");
    }
    if let Some(p) = args.screen {
        let rgba = m.board.render_rgba();
        let mut f = std::fs::File::create(&p).unwrap();
        write!(f, "P6\n640 240\n255\n").unwrap();
        for px in rgba.chunks(4) {
            f.write_all(&px[..3]).unwrap();
        }
        println!("screen written to {p} (frame base {:05X})", m.board.frame_base());
    }
}
