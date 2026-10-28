//! Headless runner for bring-up: boot the ROM, then report where execution
//! ended up, which I/O ports were touched but not emulated, and dump RAM or
//! a guessed frame buffer.

mod serve;

use icon_core::devices::nvram::presets;
use icon_core::devices::wd1002::Disk;
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
    floppy: Option<String>,
    regions: usize,
    script: Option<String>,
    hd_out: Option<String>,
    swaps: Vec<(f64, String)>,
    lan: Option<f64>,
    ws_screen: Option<String>,
    type_text: Vec<(f64, String)>,
    trace_from: Option<u32>,
    trace_count: usize,
    hd: Option<String>,
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
        floppy: None,
        regions: 0,
        script: None,
        hd_out: None,
        swaps: Vec::new(),
        lan: None,
        ws_screen: None,
        type_text: Vec::new(),
        trace_from: None,
        trace_count: 2000,
        hd: None,
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
            "--floppy" => a.floppy = Some(v()),
            "--regions" => a.regions = parse_num(&v()) as usize,
            "--script" => a.script = Some(v()),
            "--hd-out" => a.hd_out = Some(v()),
            "--swap" => {
                // --swap SECONDS:IMAGE  (insert a different floppy)
                let s = v();
                let (t, p) = s.split_once(':').expect("--swap SECONDS:IMAGE");
                a.swaps.push((t.parse().unwrap(), p.to_string()));
            }
            "--lan" => a.lan = Some(v().parse().unwrap()),
            "--ws-screen" => a.ws_screen = Some(v()),
            "--type" => {
                // --type SECONDS:TEXT  (\r for Enter)
                let s = v();
                let (t, text) = s.split_once(':').expect("--type SECONDS:TEXT");
                a.type_text.push((t.parse().unwrap(), text.replace("\\r", "\r")));
            }
            "--trace-from" => a.trace_from = Some(parse_num(&v())),
            "--trace-count" => a.trace_count = parse_num(&v()) as usize,
            "--hd" => a.hd = Some(v()),
            "-h" | "--help" => {
                println!("icon-cli [--rom F] [--preset ws|fs] [--seconds S] [--ram-kb N] [--straps 0x1F]\n         [--trace-last N] [--dump-ram F] [--screen F.ppm] [--floppy F.img] [--hd F.img]");
                std::process::exit(0);
            }
            _ => panic!("unknown argument {k}"),
        }
    }
    a
}

/// `icon-cli session ...`: headless fileserver session for automated tests.
///
///   icon-cli session [--rom F] [--hd IMG] [--floppy IMG] [--user U --password P]
///                    [--cmd "files 1:/ +v"]... [--insert-floppy IMG] [--save-hd OUT]
///                    [--save-floppy OUT] [--transcript OUT]
///
/// Boots a LEXICON fileserver, logs in (root/superuser by default), runs each
/// --cmd in order (an --insert-floppy between commands swaps the diskette at
/// that point) and prints each command's output between `=== CMD` markers.
/// Exit status 1 on a timeout.
fn session_main(argv: Vec<String>) {
    use icon_core::session::Session;
    let mut rom = format!("{}/repos/icon_icon2_research/firmware/ICON1/icon1_rom_combined.bin", std::env::var("HOME").unwrap_or_default());
    let (mut hd, mut fd) = (None, None);
    let (mut user, mut pass) = ("root".to_string(), "superuser".to_string());
    let mut steps: Vec<(String, String)> = Vec::new();
    let (mut save_hd, mut save_fd, mut transcript) = (None, None, None);
    let mut it = argv.into_iter();
    while let Some(k) = it.next() {
        let mut v = || it.next().unwrap_or_else(|| panic!("missing value for {k}"));
        match k.as_str() {
            "--rom" => rom = v(),
            "--hd" => hd = Some(std::fs::read(v()).expect("read hd")),
            "--floppy" => fd = Some(std::fs::read(v()).expect("read floppy")),
            "--user" => user = v(),
            "--password" => pass = v(),
            "--cmd" => steps.push(("cmd".into(), v())),
            "--insert-floppy" => steps.push(("insert".into(), v())),
            "--eject" => steps.push(("eject".into(), String::new())),
            "--save-hd" => save_hd = Some(v()),
            "--save-floppy" => save_fd = Some(v()),
            "--transcript" => transcript = Some(v()),
            _ => panic!("unknown session argument {k}"),
        }
    }
    let rom = std::fs::read(&rom).expect("read ROM");
    let fail = |e: icon_core::session::SessionError| -> ! {
        eprintln!("session error: {e}");
        std::process::exit(1)
    };
    let mut s = Session::fileserver(&rom, hd, fd).unwrap_or_else(|e| fail(e));
    s.login(&user, &pass).unwrap_or_else(|e| fail(e));
    for (kind, arg) in steps {
        match kind.as_str() {
            "cmd" => {
                let out = s.run(&arg).unwrap_or_else(|e| fail(e));
                println!("=== CMD {arg}\n{out}");
            }
            "insert" => s.insert_floppy(Some(std::fs::read(&arg).expect("read floppy"))).unwrap_or_else(|e| fail(e)),
            _ => s.insert_floppy(None).unwrap(),
        }
    }
    println!("=== END");
    if let (Some(p), Some(d)) = (save_hd, s.hard_disk()) {
        std::fs::write(p, d).expect("write hd");
    }
    if let (Some(p), Some(d)) = (save_fd, s.floppy()) {
        std::fs::write(p, d).expect("write floppy");
    }
    if let Some(p) = transcript {
        std::fs::write(p, s.transcript()).expect("write transcript");
    }
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.first().map(String::as_str) == Some("session") {
        return session_main(argv[1..].to_vec());
    }
    if argv.first().map(String::as_str) == Some("serve") {
        return serve::main(argv[1..].to_vec());
    }
    let mut args = parse_args();
    let rom = std::fs::read(&args.rom).expect("read ROM");
    let nvram = match args.preset.as_str() {
        "ws" => presets::workstation(),
        "fs" => presets::fileserver(),
        p => panic!("unknown preset {p}"),
    };
    let fs = args.preset == "fs";
    let _ = fs;
    let mut m = Machine::new(Config { rom, patch_800k_floppy: false, ram_kb: args.ram_kb, nvram, straps: args.straps, host_time: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0), disk_controller: fs });
    if let Some(p) = &args.floppy {
        let d = Disk::floppy_from_image(std::fs::read(p).expect("read floppy")).expect("unknown floppy size");
        m.board.insert_floppy(Some(d));
    }
    if let Some(p) = &args.hd {
        m.board.attach_hard_disk(Some(Disk::hard_auto(std::fs::read(p).expect("read hd"))));
    }

    if let Some(script) = args.script.clone() {
        run_script(m, &args, &script);
        return;
    }
    if let Some(ws_at) = args.lan {
        run_lan(m, &args, ws_at);
        return;
    }
    let total = (args.seconds * CPU_HZ as f64) as u64;
    let mut ring: Vec<u32> = Vec::with_capacity(args.trace_last);
    let mut hot: HashMap<u32, u64> = HashMap::new();
    let mut instr = 0u64;
    let mut loud_ms = 0u64;
    let mut next_audio = 0u64;
    let mut last_region = u32::MAX;
    let mut region_logs = 0usize;
    let mut tracing = 0usize;
    while m.board.cycles < total {
        let pc = m.cpu.pc();
        if tracing == 0 && args.trace_from == Some(pc) {
            tracing = args.trace_count;
            m.board.io_log = Some(Vec::new());
        }
        if tracing > 0 {
            let r = m.cpu.regs();
            let io: Vec<String> = m.board.io_log.as_mut().map(|l| l.drain(..).map(|(_, p, v, w)| format!("{}{p:04X}={v:02X}", if w { "out " } else { "in " })).collect()).unwrap_or_default();
            if !io.is_empty() {
                println!("        io: {}", io.join(" "));
            }
            println!("T {:04X}:{:04X} AX={:04X} BX={:04X} CX={:04X} DX={:04X} SI={:04X} DI={:04X} BP={:04X} SP={:04X} DS={:04X} ES={:04X} FL={:04X}",
                r.cs, r.ip, r.ax, r.bx, r.cx, r.dx, r.si, r.di, r.bp, r.sp, r.ds, r.es, r.flags);
            tracing -= 1;
            if tracing == 0 { m.board.io_log = None; }
        }
        if let Some(i) = args.swaps.iter().position(|(t, _)| m.board.cycles as f64 >= t * CPU_HZ as f64) {
            let (_, p) = args.swaps.remove(i);
            let d = Disk::floppy_from_image(std::fs::read(&p).expect("read floppy")).expect("floppy size");
            m.board.insert_floppy(Some(d));
        }
        if let Some(i) = args.type_text.iter().position(|(t, _)| m.board.cycles as f64 >= t * CPU_HZ as f64) {
            let (_, text) = args.type_text.remove(i);
            for k in icon_core::devices::keyboard::type_text(&text) {
                m.board.key(k);
            }
        }
        if region_logs < args.regions && pc >> 12 != last_region {
            let r = m.cpu.regs();
            let prev = ring.last().copied().unwrap_or(0);
            println!(
                "[{:>10}] {prev:05X} -> {pc:05X}  CS={:04X} IP={:04X} AX={:04X} BX={:04X} CX={:04X} DX={:04X} SP={:04X} SS={:04X} DS={:04X} ES={:04X}",
                m.board.cycles, r.cs, r.ip, r.ax, r.bx, r.cx, r.dx, r.sp, r.ss, r.ds, r.es
            );
            last_region = pc >> 12;
            region_logs += 1;
        }
        if ring.len() == args.trace_last {
            ring.remove(0);
        }
        ring.push(pc);
        *hot.entry(pc).or_insert(0) += 1;
        m.step();
        instr += 1;
        if m.board.cycles >= next_audio {
            // Count 10 ms windows with audible speaker output.
            next_audio = m.board.cycles + (CPU_HZ / 100) as u64;
            let a = m.board.take_audio(8000);
            if a.iter().any(|x| x.abs() > 0.05) {
                loud_ms += 10;
            }
        }
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
    println!("speaker audible for ~{loud_ms} ms");
    println!("FA88 (trouble LED) edges: {}", m.board.fa88_edges);
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

fn write_ppm(path: &str, rgba: &[u8]) {
    let mut f = std::fs::File::create(path).unwrap();
    write!(f, "P6\n640 240\n255\n").unwrap();
    for px in rgba.chunks(4) {
        f.write_all(&px[..3]).unwrap();
    }
}

/// Fileserver (slot 0) plus a workstation (slot 1) powered on `ws_at`
/// seconds later, on one ICONET segment.
fn run_lan(fs: Machine, args: &Args, ws_at: f64) {
    use icon_core::lan::Lan;
    let rom = std::fs::read(&args.rom).expect("read ROM");
    let mut lan = Lan::new(2);
    lan.power_on(0, fs);
    let total = (args.seconds * CPU_HZ as f64) as u64;
    let ws_at = (ws_at * CPU_HZ as f64) as u64;
    let mut ws_on = false;
    let mut t = 0u64;
    while t < total {
        if !ws_on && t >= ws_at {
            let ws = Machine::new(Config {
                rom: rom.clone(),
                patch_800k_floppy: false,
                ram_kb: 512,
                nvram: presets::workstation(),
                straps: STRAPS_NORMAL,
                host_time: 0,
                disk_controller: false,
            });
            lan.power_on(1, ws);
            ws_on = true;
        }
        lan.run(CPU_HZ as u64 / 60);
        t += CPU_HZ as u64 / 60;
    }
    println!("LAN: {} packets delivered", lan.packets);
    for (i, m) in lan.nodes.iter().enumerate() {
        if let Some(m) = m {
            let r = m.cpu.regs();
            println!("node slot {i}: id {:02X} CS:IP={:04X}:{:04X}", m.board.arcnet.node_id, r.cs, r.ip);
        }
    }
    if let (Some(p), Some(m)) = (&args.screen, lan.nodes[0].as_ref()) {
        write_ppm(p, &m.board.render_rgba());
    }
    if let (Some(p), Some(m)) = (&args.ws_screen, lan.nodes[1].as_ref()) {
        write_ppm(p, &m.board.render_rgba());
    }
}

/// Run a scripted session on a two-machine network. The fileserver (slot 0)
/// is `fs` and is already configured by the command-line options. Each script
/// line is `SECONDS ACTION ...`:
///   ws power | fs power             toggle a machine's power
///   ws type TEXT | fs type TEXT     type text (\r = Enter, \e = Esc)
///   ws key HEX                      send one raw key code
///   ws mouse DX DY                  move the trackball
///   fs floppy PATH | fs eject       change the fileserver's diskette
///   shot NAME                       save both screens as NAME_fs.ppm / NAME_ws.ppm
///   save PATH                       write the fileserver's hard disk image
///   savefloppy PATH                 write the fileserver's diskette image
///   text NAME                       save each screen's decoded text as NAME_fs.txt / NAME_ws.txt
fn run_script(fs: Machine, args: &Args, path: &str) {
    use icon_core::lan::Lan;
    // `DISKS/` in a script stands for $ICON_DISKS (the folder holding the
    // YorkU disk image sets), so scripts can be shared.
    let disks = std::env::var("ICON_DISKS").unwrap_or_else(|_| "DISKS".into());
    let text = std::fs::read_to_string(path).expect("read script").replace("DISKS/", &format!("{disks}/"));
    let mut steps: Vec<(f64, Vec<String>)> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let mut it = l.splitn(2, ' ');
            let t: f64 = it.next().unwrap().parse().expect("time");
            let rest = it.next().unwrap_or("");
            (t, rest.splitn(3, ' ').map(String::from).collect())
        })
        .collect();
    steps.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let rom = std::fs::read(&args.rom).expect("read ROM");
    let shot_dir = std::path::Path::new(path).parent().unwrap_or(std::path::Path::new(".")).to_path_buf();
    let mut lan = Lan::new(2);
    let mut fs = Some(fs);
    let new_ws = || {
        Machine::new(Config {
            rom: rom.clone(),
            patch_800k_floppy: false,
            ram_kb: 512,
            nvram: presets::workstation(),
            straps: STRAPS_NORMAL,
            host_time: 0,
            disk_controller: false,
        })
    };
    let frame = CPU_HZ as u64 / 60;
    let total = (args.seconds * CPU_HZ as f64) as u64;
    let mut t = 0u64;
    let mut idx = 0;
    while t < total {
        while idx < steps.len() && (steps[idx].0 * CPU_HZ as f64) as u64 <= t {
            let (when, ref a) = steps[idx];
            idx += 1;
            let who = a[0].as_str();
            let slot = if who == "ws" { 1 } else { 0 };
            let verb = a.get(1).map(String::as_str).unwrap_or("");
            let arg = a.get(2).cloned().unwrap_or_default();
            println!("[{when:7.1}s] {}", a.join(" "));
            match (who, verb) {
                ("fs", "power") => match fs.take() {
                    Some(m) => lan.power_on(0, m),
                    None => {
                        if let Some(m) = lan.power_off(0) {
                            // Keep the disks: power back on with the same drives.
                            let mut n = Machine::new(Config {
                                rom: rom.clone(),
                                patch_800k_floppy: false,
                                ram_kb: args.ram_kb,
                                nvram: m.board.nvram.words,
                                straps: STRAPS_NORMAL,
                                host_time: now_secs(),
                                disk_controller: true,
                            });
                            n.board.wd.drives = m.board.wd.drives;
                            fs = Some(n);
                        }
                    }
                },
                ("ws", "power") => {
                    if lan.nodes[1].is_some() {
                        lan.power_off(1);
                    } else {
                        let mut w = new_ws();
            w.int5c_log = Some(Vec::new());
            w.board.watch = Some((0x74A, 0x74B));
            lan.power_on(1, w);
                    }
                }
                (_, "type") => {
                    let txt = arg.replace("\\r", "\r").replace("\\e", "\x1b");
                    if let Some(m) = lan.nodes[slot].as_mut() {
                        for k in icon_core::devices::keyboard::type_text(&txt) {
                            m.board.key(k);
                        }
                    }
                }
                (_, "key") => {
                    if let Some(m) = lan.nodes[slot].as_mut() {
                        m.board.key(u8::from_str_radix(&arg, 16).expect("hex key"));
                    }
                }
                (_, "mouse") => {
                    let mut p = arg.split_whitespace().map(|x| x.parse::<i32>().unwrap());
                    let (dx, dy) = (p.next().unwrap_or(0), p.next().unwrap_or(0));
                    if let Some(m) = lan.nodes[slot].as_mut() {
                        m.board.counters[1].motion(dx);
                        m.board.counters[0].motion(dy);
                    }
                }
                ("fs", "floppy") => {
                    let d = Disk::floppy_from_image(std::fs::read(&arg).expect("read floppy")).expect("floppy size");
                    if let Some(m) = lan.nodes[0].as_mut() { m.board.insert_floppy(Some(d)); }
                    else if let Some(m) = fs.as_mut() { m.board.insert_floppy(Some(d)); }
                }
                ("fs", "eject") => {
                    if let Some(m) = lan.nodes[0].as_mut() { m.board.insert_floppy(None); }
                    else if let Some(m) = fs.as_mut() { m.board.insert_floppy(None); }
                }
                ("shot", _) => {
                    let name = verb;
                    for (i, tag) in [(0, "fs"), (1, "ws")] {
                        if let Some(m) = lan.nodes[i].as_ref() {
                            write_ppm(shot_dir.join(format!("{name}_{tag}.ppm")).to_str().unwrap(), &m.board.render_rgba());
                        }
                    }
                }
                ("save", _) => save_hd(&lan, fs.as_ref(), verb),
                ("text", _) => {
                    // Decoded console text of each running machine.
                    for (i, tag) in [(0, "fs"), (1, "ws")] {
                        if let Some(m) = lan.nodes[i].as_ref() {
                            std::fs::write(shot_dir.join(format!("{verb}_{tag}.txt")), m.board.screen_text()).unwrap();
                        }
                    }
                }
                ("ram", _) => {
                    // RAM of each running machine, plus its display base (debugging).
                    for (i, tag) in [(0, "fs"), (1, "ws")] {
                        if let Some(m) = lan.nodes[i].as_ref() {
                            std::fs::write(shot_dir.join(format!("{verb}_{tag}.ram")), &m.board.ram).unwrap();
                            println!("{tag}: frame base {:05X}", m.board.frame_base());
                        }
                    }
                }
                ("savefloppy", _) => {
                    let m = lan.nodes[0].as_ref().or(fs.as_ref());
                    if let Some(d) = m.and_then(|m| m.board.wd.drives[3].as_ref()) {
                        std::fs::write(verb, &d.data).expect("write floppy");
                        println!("floppy saved to {verb}");
                    }
                }
                _ => println!("  ?? unknown action"),
            }
        }
        lan.run(frame);
        t += frame;
    }
    println!("LAN: {} packets", lan.packets);
    for (i, m) in lan.nodes.iter().enumerate() {
        if let Some(m) = m {
            let r = m.cpu.regs();
            println!("slot {i}: node {:02X} CS:IP={:04X}:{:04X}", m.board.arcnet.node_id, r.cs, r.ip);
            if m.board.has_wd {
                println!("  WD1002 cmds by nibble: {:?}", m.board.wd.cmd_counts);
                println!("  last: {:02X?}  status {:02X} err {:02X}", m.board.wd.last_cmds, m.board.wd.status, m.board.wd.error);
            }
            let reads = m.board.nvram.log.iter().filter(|l| l.0 == 2).count();
            let writes: Vec<_> = m.board.nvram.log.iter().filter(|l| l.0 != 2).collect();
            if !m.board.watch_log.is_empty() {
                let v: Vec<String> = m.board.watch_log.iter().zip(&m.board.watch_pcs).map(|((c, a, x), pc)| format!("{:.2}s {a:05X}={x:02X} @{pc:05X}", *c as f64 / CPU_HZ as f64)).collect();
                println!("  watch writes: {}", v.join(", "));
            }
            if let Some(log) = &m.int5c_log {
                let mut counts = std::collections::BTreeMap::new();
                for (_, f, r) in log {
                    if matches!(f, 0x13 | 0x1B | 0x2D | 0x3E | 0x48 | 0x4D) {
                        *counts.entry((*f, *r)).or_insert(0u32) += 1;
                    }
                }
                println!("  int5Ch (fn,result) counts: {:X?}", counts);
                let line: Vec<String> = log.iter().filter(|(c, f, _)| *c > 170 * CPU_HZ as u64 && !matches!(f, 0x0E | 0x18 | 0x0A)).map(|(c, f, r)| format!("{:.1}:{f:X}={r:X}", *c as f64 / CPU_HZ as f64)).collect();
                println!("  timeline: {}", line.join(" "));
            }
            println!("  NVRAM aborted reads: {}", m.board.nvram.aborted_reads);
            println!("  NVRAM: {reads} reads, other commands: {:?}", &writes[..writes.len().min(20)]);
            println!("  NVRAM words now: {:04X?}", m.board.nvram.words);
            if let Some(e) = &m.boot_entry {
                let pb = ((e.es as usize) << 4) + e.bx as usize;
                let rd = |a: usize| m.board.ram.get(a).copied();
                println!("  boot entry {:04X}:{:04X} ES:BX={:04X}:{:04X} DS={:04X} SS:SP={:04X}:{:04X}  param[0x38]={:02X?}",
                    e.cs, e.ip, e.es, e.bx, e.ds, e.ss, e.sp, rd(pb + 0x38));
                println!("  params at entry: {:02X?}", m.boot_params);
            }
        }
    }
    if let Some(p) = &args.hd_out {
        save_hd(&lan, fs.as_ref(), p);
    }
}

fn save_hd(lan: &icon_core::lan::Lan, off: Option<&Machine>, path: &str) {
    let m = lan.nodes[0].as_ref().or(off);
    if let Some(d) = m.and_then(|m| m.board.wd.drives[0].as_ref()) {
        std::fs::write(path, &d.data).expect("write hd");
        println!("hard disk saved to {path}");
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}
