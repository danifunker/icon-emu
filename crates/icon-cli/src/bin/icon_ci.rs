//! `icon-ci`: client for `icon-cli serve`, in the style of iris's `iris-ci`.
//!
//!   icon-ci [--socket /tmp/icon.sock] <command> [args]
//!
//! Commands (slot 0 = fileserver, 1.. = workstations):
//!   status                         machines, emulated time, packets
//!   hd-load IMG | hd-new [10mb|64mb]   disk for the next fileserver power-on
//!   floppy IMG | eject             change the fileserver's diskette
//!   floppy-new                     insert a blank pre-formatted 640K diskette
//!   power SLOT [on|off] [--no-wait]    power a machine (waits for Login:)
//!   add-workstation                add a workstation slot
//!   login [SLOT] [USER] [PASSWORD] default root/superuser
//!   run [--slot N] [--timeout S] 'COMMAND'   run a shell command, print its output
//!   type SLOT 'TEXT'               type (\r = Enter)
//!   key SLOT HEX                   raw key code
//!   mouse SLOT DX DY | click SLOT [right]
//!   wait-text SLOT 'TEXT' [TIMEOUT]
//!   idle SECONDS
//!   screen [SLOT] | transcript [SLOT] | screenshot SLOT FILE.ppm
//!   speech-wav SLOT FILE.wav   (the speech chip's recent output, 8 kHz)
//!   hd-save FILE | floppy-save FILE
//!
//! Files on the fileserver's disks. DISK is fd (the diskette), hd (the hard
//! disk's first partition) or hd2 / hd3 (its other partitions). Reading works
//! on a running machine; the diskette can be written any time, the hard disk
//! only while every machine is off. --text converts text: UTF-8 with LF line
//! ends on this side, the ICON's RS line ends and accents on the other.
//! --replace overwrites an existing file. rm deletes a folder with everything in it.
//!   ls DISK [PATH]                 list a folder
//!   get DISK PATH HOSTFILE [--text]
//!   put DISK HOSTFILE PATH [--text] [--replace]
//!   mkdir DISK PATH | rm DISK PATH
//!   cp DISK PATH TODISK DIR [--replace]   copy a file or folder into DIR, keeping
//!                                  permissions, owners and dates
//!   script FILE [--keep-going]     run icon-ci commands, one per line (# comments)
//!   quit
//!
//! Exit status: 0 on success, 1 on a failed command.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

fn request(sock: &mut UnixStream, reader: &mut BufReader<UnixStream>, cmd: &str, args: Value) -> Result<Value, String> {
    writeln!(sock, "{}", json!({"cmd": cmd, "args": args})).map_err(|e| e.to_string())?;
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&line).map_err(|e| format!("bad reply: {e}"))?;
    if v["ok"].as_bool() == Some(true) {
        Ok(v["data"].clone())
    } else {
        Err(v["error"].as_str().unwrap_or("error").to_string())
    }
}

/// Split a script line into words, honouring single and double quotes.
fn split_words(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut any = false;
    for ch in line.chars() {
        match (quote, ch) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, '\'' | '"') => {
                quote = Some(ch);
                any = true;
            }
            (None, c) if c.is_whitespace() => {
                if !cur.is_empty() || any {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            (None, c) => cur.push(c),
        }
    }
    if !cur.is_empty() || any {
        out.push(cur);
    }
    out
}

fn unescape(s: &str) -> String {
    s.replace("\\r", "\r").replace("\\e", "\x1b")
}

/// Translate one command line into a request; returns what to print.
fn exec(sock: &mut UnixStream, reader: &mut BufReader<UnixStream>, words: &[String]) -> Result<String, String> {
    let w = |i: usize| words.get(i).cloned().unwrap_or_default();
    let num = |i: usize, d: u64| words.get(i).and_then(|x| x.parse().ok()).unwrap_or(d);
    let (cmd, args) = match w(0).as_str() {
        "status" | "ping" | "add-workstation" | "quit" => (w(0), json!({})),
        "hd-load" => ("hd-load".into(), json!({"path": w(1)})),
        "hd-new" => ("hd-new".into(), json!({"size": if w(1).is_empty() { "10mb".into() } else { w(1) }})),
        "floppy" => ("floppy".into(), json!({"path": w(1)})),
        "eject" => ("floppy".into(), json!({"path": null})),
        "floppy-new" => ("floppy-new".into(), json!({})),
        "power" => ("power".into(), json!({"slot": num(1, 0), "on": w(2) != "off", "wait": !words.iter().any(|x| x == "--no-wait")})),
        "login" => ("login".into(), json!({"slot": num(1, 0), "user": if w(2).is_empty() { "root".into() } else { w(2) }, "password": if w(3).is_empty() { "superuser".into() } else { w(3) }})),
        "run" => {
            let mut slot = 0;
            let mut timeout = Value::Null;
            let mut i = 1;
            while i < words.len() && words[i].starts_with("--") {
                match words[i].as_str() {
                    "--slot" => slot = num(i + 1, 0),
                    "--timeout" => timeout = json!(words.get(i + 1).and_then(|x| x.parse::<f64>().ok())),
                    _ => return Err(format!("unknown option {}", words[i])),
                }
                i += 2;
            }
            ("run".into(), json!({"slot": slot, "command": words[i..].join(" "), "timeout": timeout}))
        }
        "type" => ("type".into(), json!({"slot": num(1, 0), "text": unescape(&w(2))})),
        "key" => ("key".into(), json!({"slot": num(1, 0), "code": u64::from_str_radix(w(2).trim_start_matches("0x"), 16).map_err(|e| e.to_string())?})),
        "mouse" => ("mouse".into(), json!({"slot": num(1, 0), "dx": w(2).parse::<i64>().unwrap_or(0), "dy": w(3).parse::<i64>().unwrap_or(0)})),
        "click" => ("click".into(), json!({"slot": num(1, 0), "button": w(2)})),
        "wait-text" => ("wait-text".into(), json!({"slot": num(1, 0), "text": w(2), "timeout": w(3).parse::<f64>().unwrap_or(120.0)})),
        "idle" => ("idle".into(), json!({"seconds": w(1).parse::<f64>().unwrap_or(1.0)})),
        "screen" | "transcript" => (w(0), json!({"slot": num(1, 0)})),
        "screenshot" => ("screenshot".into(), json!({"slot": num(1, 0), "path": w(2)})),
        "hd-save" | "floppy-save" => (w(0), json!({"path": w(1)})),
        "ls" | "get" | "put" | "mkdir" | "rm" | "cp" => {
            let flags: Vec<&String> = words.iter().filter(|x| x.starts_with("--")).collect();
            let pos: Vec<&String> = words.iter().filter(|x| !x.starts_with("--")).collect();
            for f in &flags {
                if !matches!(f.as_str(), "--text" | "--replace") {
                    return Err(format!("unknown option {f}"));
                }
            }
            let p = |i: usize| pos.get(i).map(|x| x.to_string()).ok_or_else(|| format!("{}: missing argument (icon-ci --help)", w(0)));
            // The server runs elsewhere: hand it absolute host paths.
            let host = |x: String| std::path::absolute(&x).map(|p| p.display().to_string()).unwrap_or(x);
            let parse_disk = |d: &str| -> Result<(&'static str, u64), String> {
                match d {
                    "fd" => Ok(("fd", 0)),
                    "hd" => Ok(("hd", 0)),
                    d if d.starts_with("hd") => Ok(("hd", d[2..].parse::<u64>().map_err(|_| format!("bad disk {d:?}"))?)),
                    d => Err(format!("unknown disk {d:?} (fd, hd, hd2, hd3)")),
                }
            };
            let (dname, part) = parse_disk(&p(1)?)?;
            let base = json!({"disk": dname, "part": part, "text": flags.iter().any(|f| *f == "--text"), "replace": flags.iter().any(|f| *f == "--replace")});
            let with = |extra: Value| {
                let mut v = base.clone();
                v.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
                v
            };
            match w(0).as_str() {
                "ls" => ("files".into(), with(json!({"path": pos.get(2).map(|x| x.to_string()).unwrap_or("/".into())}))),
                "get" => ("file-get".into(), with(json!({"path": p(2)?, "out": host(p(3)?)}))),
                "put" => ("file-put".into(), with(json!({"src": host(p(2)?), "path": p(3)?}))),
                "mkdir" => ("file-mkdir".into(), with(json!({"path": p(2)?}))),
                "cp" => {
                    // cp FROMDISK PATH TODISK DIR: the target is the disk the rules apply to.
                    let (tname, tpart) = parse_disk(&p(3)?)?;
                    let mut v = with(json!({"from_disk": dname, "from_part": part, "path": p(2)?, "dir": p(4)?}));
                    v["disk"] = json!(tname);
                    v["part"] = json!(tpart);
                    ("file-cp".into(), v)
                }
                _ => ("file-rm".into(), with(json!({"path": p(2)?}))),
            }
        }
        "speech-wav" => ("speech-wav".into(), json!({"slot": num(1, 1), "path": w(2)})),
        other => return Err(format!("unknown command {other:?} (icon-ci --help)")),
    };
    let data = request(sock, reader, &cmd, args)?;
    Ok(match cmd.as_str() {
        "run" => data["output"].as_str().unwrap_or("").to_string(),
        "screen" | "transcript" => data["text"].as_str().unwrap_or("").to_string(),
        "files" => {
            let mut out: Vec<String> = data["entries"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|e| {
                    let name = format!("{}{}", e["name"].as_str().unwrap_or(""), if e["dir"].as_bool() == Some(true) { "/" } else { "" });
                    format!("{:>8}  {:19}  {name}", if e["dir"].as_bool() == Some(true) { String::new() } else { e["size"].to_string() }, e["date"].as_str().unwrap_or(""))
                })
                .collect();
            out.push(format!("{} KB free of {} KB", data["free"].as_u64().unwrap_or(0) / 1024, data["total"].as_u64().unwrap_or(0) / 1024));
            out.join("\n")
        }
        _ => match data {
            Value::String(s) => s,
            v => v.to_string(),
        },
    })
}

fn main() {
    let mut argv: Vec<String> = std::env::args().skip(1).collect();
    let mut socket = std::env::var("ICON_CI_SOCKET").unwrap_or_else(|_| "/tmp/icon.sock".into());
    if argv.first().map(String::as_str) == Some("--socket") && argv.len() > 1 {
        socket = argv[1].clone();
        argv.drain(..2);
    }
    if argv.is_empty() || argv[0] == "--help" || argv[0] == "-h" {
        println!("{}", include_str!("icon_ci.rs").lines().skip_while(|l| !l.starts_with("//!")).take_while(|l| l.starts_with("//!")).map(|l| l.trim_start_matches("//!").trim_start_matches(' ')).collect::<Vec<_>>().join("\n"));
        return;
    }
    let mut sock = UnixStream::connect(&socket).unwrap_or_else(|e| {
        eprintln!("icon-ci: cannot connect to {socket}: {e} (start it with `icon-cli serve`)");
        std::process::exit(2)
    });
    let mut reader = BufReader::new(sock.try_clone().expect("clone"));
    if argv[0] == "script" {
        let keep_going = argv.iter().any(|a| a == "--keep-going");
        let text = std::fs::read_to_string(&argv[1]).unwrap_or_else(|e| panic!("{}: {e}", argv[1]));
        let mut failed = false;
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let words = split_words(line);
            match exec(&mut sock, &mut reader, &words) {
                Ok(out) => {
                    println!("[{}] {line}", n + 1);
                    if !out.is_empty() {
                        println!("{out}");
                    }
                }
                Err(e) => {
                    eprintln!("[{}] {line}\nFAILED: {e}", n + 1);
                    failed = true;
                    if !keep_going {
                        break;
                    }
                }
            }
        }
        std::process::exit(failed as i32);
    }
    match exec(&mut sock, &mut reader, &argv) {
        Ok(out) => {
            if !out.is_empty() {
                println!("{out}");
            }
        }
        Err(e) => {
            eprintln!("icon-ci: {e}");
            std::process::exit(1)
        }
    }
}
