//! `icon-cli serve`: a headless ICON network driven over a control socket,
//! in the style of iris's `--ci` mode. Use the `icon-ci` client.
//!
//! Protocol: one JSON object per line, `{"cmd": "...", "args": {...}}`; each
//! gets one reply line `{"ok": true, "data": ...}` or `{"ok": false,
//! "error": "..."}`. Emulated time only advances while a command runs, so a
//! scripted session is deterministic.

use icon_core::devices::wd1002::{Disk, HD_10MB, HD_64MB};
use icon_core::session::{Rig, FILESERVER};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};

pub const DEFAULT_SOCKET: &str = "/tmp/icon.sock";

struct Server {
    rig: Rig,
    /// Disks to attach the next time the fileserver powers on.
    hd: Option<Vec<u8>>,
    floppy: Option<Vec<u8>>,
    quit: bool,
}

pub fn main(argv: Vec<String>) {
    let mut socket = std::env::var("ICON_CI_SOCKET").unwrap_or_else(|_| DEFAULT_SOCKET.into());
    let mut rom = format!("{}/repos/icon_icon2_research/firmware/ICON1/icon1_rom_combined.bin", std::env::var("HOME").unwrap_or_default());
    let (mut hd, mut floppy, mut workstations, mut ram_kb) = (None, None, 1usize, 512u32);
    // --rom is used by every machine unless --fs-rom / --ws-rom override it.
    let (mut fs_rom, mut ws_rom): (Option<String>, Option<String>) = (None, None);
    let mut it = argv.into_iter();
    while let Some(k) = it.next() {
        let mut v = || it.next().unwrap_or_else(|| panic!("missing value for {k}"));
        match k.as_str() {
            "--socket" => socket = v(),
            "--rom" => rom = v(),
            "--fs-rom" => fs_rom = Some(v()),
            "--ws-rom" => ws_rom = Some(v()),
            "--hd" => hd = Some(std::fs::read(v()).expect("read hard disk image")),
            "--floppy" => floppy = Some(std::fs::read(v()).expect("read floppy image")),
            "--workstations" => workstations = v().parse().expect("number"),
            "--ram-kb" => ram_kb = v().parse().expect("number"),
            "-h" | "--help" => {
                println!("icon-cli serve [--socket {DEFAULT_SOCKET}] [--rom F] [--fs-rom F] [--ws-rom F] [--hd IMG] [--floppy IMG] [--workstations N] [--ram-kb 512|1024]");
                return;
            }
            _ => panic!("unknown serve argument {k}"),
        }
    }
    let read = |f: &str| std::fs::read(f).unwrap_or_else(|e| panic!("read ROM {f}: {e}"));
    let mut rig = Rig::new(&read(fs_rom.as_deref().unwrap_or(&rom)), workstations).expect("fileserver ROM");
    if fs_rom.is_some() || ws_rom.is_some() {
        let ws = read(ws_rom.as_deref().unwrap_or(&rom));
        if ws.len() != 0x8000 {
            panic!("workstation ROM must be the 32 KB interleaved image");
        }
        rig.workstation_rom = Some(ws);
    }
    rig.ram_kb = ram_kb;
    let mut server = Server { rig, hd, floppy, quit: false };
    let _ = std::fs::remove_file(&socket);
    let listener = UnixListener::bind(&socket).unwrap_or_else(|e| panic!("bind {socket}: {e}"));
    eprintln!("icon-cli serve: listening on {socket} ({} slots: fileserver + {workstations} workstations)", server.rig.slots());
    for stream in listener.incoming() {
        match stream {
            Ok(s) => server.handle(s),
            Err(e) => eprintln!("accept: {e}"),
        }
        if server.quit {
            break;
        }
    }
    let _ = std::fs::remove_file(&socket);
}

impl Server {
    fn handle(&mut self, stream: UnixStream) {
        let mut out = stream.try_clone().expect("clone stream");
        for line in BufReader::new(stream).lines() {
            let Ok(line) = line else { break };
            if line.trim().is_empty() {
                continue;
            }
            let reply = match serde_json::from_str::<Value>(&line) {
                Ok(req) => {
                    let cmd = req["cmd"].as_str().unwrap_or("").to_string();
                    let args = req.get("args").cloned().unwrap_or(json!({}));
                    match self.dispatch(&cmd, &args) {
                        Ok(data) => json!({"ok": true, "data": data}),
                        Err(e) => json!({"ok": false, "error": e}),
                    }
                }
                Err(e) => json!({"ok": false, "error": format!("bad JSON: {e}")}),
            };
            if writeln!(out, "{reply}").is_err() || self.quit {
                break;
            }
        }
    }

    fn dispatch(&mut self, cmd: &str, a: &Value) -> Result<Value, String> {
        let slot = || a["slot"].as_u64().unwrap_or(0) as usize;
        let s = |k: &str| a[k].as_str().map(str::to_string).ok_or_else(|| format!("missing argument {k}"));
        let read = |k: &str| -> Result<Vec<u8>, String> {
            let p = a[k].as_str().ok_or_else(|| format!("missing argument {k}"))?;
            std::fs::read(p).map_err(|e| format!("{p}: {e}"))
        };
        let e = |x: icon_core::session::SessionError| x.to_string();
        match cmd {
            "ping" => Ok(json!("pong")),
            "status" => Ok(json!({
                "slots": self.rig.slots(),
                "powered": (0..self.rig.slots()).map(|i| self.rig.machine(i).is_ok()).collect::<Vec<_>>(),
                "seconds": self.rig.seconds(),
                "packets": self.rig.lan.packets,
                "hard_disk_attached": self.rig.hard_disk().is_some() || self.hd.is_some(),
            })),
            "hd-load" => {
                self.hd = Some(read("path")?);
                Ok(json!("hard disk will be attached at the next fileserver power-on"))
            }
            "hd-new" => {
                let (c, h, sec) = match a["size"].as_str().unwrap_or("10mb") {
                    "64mb" => HD_64MB,
                    _ => HD_10MB,
                };
                self.hd = Some(Disk::blank_hard(c, h, sec).data);
                Ok(json!(format!("blank {c}x{h}x{sec} disk will be attached at the next fileserver power-on")))
            }
            "floppy" => {
                let img = if a["path"].is_null() { None } else { Some(read("path")?) };
                if self.rig.machine(FILESERVER).is_ok() {
                    self.rig.insert_floppy(img).map_err(e)?;
                } else {
                    self.floppy = img;
                }
                Ok(json!("ok"))
            }
            "floppy-new" => {
                let img = Some(Disk::formatted_floppy_640k());
                if self.rig.machine(FILESERVER).is_ok() {
                    self.rig.insert_floppy(img).map_err(e)?;
                } else {
                    self.floppy = img;
                }
                Ok(json!("blank pre-formatted 640K diskette inserted"))
            }
            "power" => {
                let on = a["on"].as_bool().unwrap_or(true);
                let n = slot();
                if !on {
                    if n == FILESERVER {
                        // Keep the disks for the next power-on.
                        if let Some(m) = self.rig.machine(FILESERVER).ok() {
                            self.hd = m.board.wd.drives[0].as_ref().map(|d| d.data.clone());
                            self.floppy = m.board.wd.drives[3].as_ref().map(|d| d.data.clone());
                        }
                    }
                    self.rig.power_off(n);
                    return Ok(json!("off"));
                }
                if n == FILESERVER {
                    self.rig.power_on_fileserver(self.hd.clone(), self.floppy.clone()).map_err(e)?;
                } else {
                    self.rig.power_on_workstation(n).map_err(e)?;
                }
                if a["wait"].as_bool().unwrap_or(true) {
                    self.rig.wait_login(n).map_err(e)?;
                }
                Ok(json!({"slot": n, "seconds": self.rig.seconds()}))
            }
            "add-workstation" => Ok(json!({"slot": self.rig.add_workstation(), "node": 15 + self.rig.slots() - 1})),
            "login" => {
                let user = a["user"].as_str().unwrap_or("root");
                let pass = a["password"].as_str().unwrap_or("superuser");
                self.rig.login(slot(), user, pass).map_err(e)?;
                Ok(json!("logged in"))
            }
            "run" => {
                if let Some(t) = a["timeout"].as_f64() {
                    self.rig.command_timeout = t;
                }
                let out = self.rig.run(slot(), &s("command")?).map_err(e)?;
                Ok(json!({"output": out}))
            }
            "type" => {
                self.rig.type_text(slot(), &s("text")?).map_err(e)?;
                Ok(json!("ok"))
            }
            "key" => {
                self.rig.key(slot(), a["code"].as_u64().ok_or("missing code")? as u8).map_err(e)?;
                Ok(json!("ok"))
            }
            "mouse" => {
                self.rig.trackball(slot(), a["dx"].as_i64().unwrap_or(0) as i32, a["dy"].as_i64().unwrap_or(0) as i32).map_err(e)?;
                Ok(json!("ok"))
            }
            "click" => {
                // Press and release the left (or right) ACTION key.
                let code = if a["button"].as_str() == Some("right") { 0x52 } else { 0x45 };
                self.rig.key(slot(), code).map_err(e)?;
                self.rig.idle(0.1);
                self.rig.key(slot(), code | 0x80).map_err(e)?;
                Ok(json!("ok"))
            }
            "wait-text" => {
                let text = s("text")?;
                let t = a["timeout"].as_f64().unwrap_or(120.0);
                self.rig.wait_for(slot(), |scr| scr.contains(text.as_str()), &text, t).map_err(e)?;
                Ok(json!("found"))
            }
            "idle" => {
                self.rig.idle(a["seconds"].as_f64().unwrap_or(1.0));
                Ok(json!({"seconds": self.rig.seconds()}))
            }
            "screen" => Ok(json!({"text": self.rig.screen(slot()).map_err(e)?})),
            "transcript" => Ok(json!({"text": self.rig.transcript(slot())})),
            "speech-wav" => {
                // The speech chip's recent output as an 8 kHz mono WAV file.
                let path = s("path")?;
                let pcm = self.rig.machine_mut(slot()).map_err(e)?.board.take_speech_samples();
                let peak = pcm.iter().map(|&v| (v as i32).abs()).max().unwrap_or(0);
                let mut wav = Vec::with_capacity(44 + pcm.len() * 2);
                let n = (pcm.len() * 2) as u32;
                wav.extend_from_slice(b"RIFF");
                wav.extend_from_slice(&(36 + n).to_le_bytes());
                wav.extend_from_slice(b"WAVEfmt ");
                for v in [16u32, 0x0001_0001, 8000, 16000, 0x0010_0002] {
                    wav.extend_from_slice(&v.to_le_bytes());
                }
                wav.extend_from_slice(b"data");
                wav.extend_from_slice(&n.to_le_bytes());
                pcm.iter().for_each(|v| wav.extend_from_slice(&v.to_le_bytes()));
                std::fs::write(&path, wav).map_err(|x| x.to_string())?;
                Ok(json!({"path": path, "samples": pcm.len(), "peak": peak}))
            }
            "screenshot" => {
                let path = s("path")?;
                let rgba = self.rig.machine(slot()).map_err(e)?.board.render_rgba();
                let mut ppm = format!("P6\n640 240\n255\n").into_bytes();
                for px in rgba.chunks(4) {
                    ppm.extend_from_slice(&px[..3]);
                }
                std::fs::write(&path, ppm).map_err(|x| x.to_string())?;
                Ok(json!({"path": path, "format": "ppm"}))
            }
            "hd-save" => {
                let path = s("path")?;
                let d = self.rig.hard_disk().or_else(|| self.hd.clone()).ok_or("no hard disk")?;
                std::fs::write(&path, d).map_err(|x| x.to_string())?;
                Ok(json!({"path": path}))
            }
            "floppy-save" => {
                let path = s("path")?;
                let d = self.rig.floppy().or_else(|| self.floppy.clone()).ok_or("no diskette")?;
                std::fs::write(&path, d).map_err(|x| x.to_string())?;
                Ok(json!({"path": path}))
            }
            "quit" => {
                self.quit = true;
                Ok(json!("bye"))
            }
            _ => Err(format!("unknown command {cmd:?}")),
        }
    }
}
