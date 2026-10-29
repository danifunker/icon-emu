//! `icon-cli disk`: the files on an ICON disk image, with no machine running.
//! The standalone counterpart of `icon-ci ls/get/put/mkdir/rm` (which work on
//! a running network) and of the web page's Open image.

use icon_core::devices::rtc::icon_time;
use icon_core::devices::wd1002::Disk;
use icon_core::files::{self, qnx2fs};

const USAGE: &str = "icon-cli disk IMAGE [--part N] COMMAND
  ls [PATH]                          list a folder (default /)
  cat PATH                           print a text file (accents and line ends decoded)
  get PATH HOSTFILE [--text]         copy a file to this computer
  put HOSTFILE PATH [--text] [--replace]   copy a file onto the image
  mkdir PATH                         make a folder
  rm PATH                            delete a file, or a folder with everything in it
  cp PATH TO_IMAGE DIR [--to-part N] [--replace]
                                     copy a file or folder into DIR on another (or
                                     the same) image, keeping permissions, owners, dates
  check                              check the filesystem

IMAGE is a diskette or hard-disk image; --part picks a hard-disk partition
(default the first QNX one). Changes are written back to IMAGE. --text converts
text: UTF-8 with LF line ends here, RS line ends and the ICON's accents there.";

/// An ICON date (local wall-clock seconds) as a host time, in this machine's time zone.
fn local_to_system_time(t: i64) -> Option<std::time::SystemTime> {
    use chrono::{Local, TimeZone};
    let naive = chrono::DateTime::from_timestamp(t, 0)?.naive_utc();
    let local = Local.from_local_datetime(&naive).earliest()?;
    let secs = u64::try_from(local.timestamp()).ok()?;
    Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs))
}

fn now() -> i64 {
    icon_time(crate::local_now())
}

pub fn main(argv: Vec<String>) {
    if let Err(e) = run(argv) {
        eprintln!("icon-cli disk: {e}");
        std::process::exit(1);
    }
}

fn run(argv: Vec<String>) -> Result<(), String> {
    let flags: Vec<&str> = argv.iter().map(String::as_str).filter(|a| a.starts_with("--")).collect();
    let mut pos: Vec<String> = Vec::new();
    let mut part = 0usize;
    let mut to_part = 0usize;
    let mut it = argv.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--part" => part = it.next().and_then(|n| n.parse().ok()).ok_or("--part needs a number")?,
            "--to-part" => to_part = it.next().and_then(|n| n.parse().ok()).ok_or("--to-part needs a number")?,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            "--text" | "--replace" => {}
            f if f.starts_with("--") => return Err(format!("unknown option {f}\n\n{USAGE}")),
            _ => pos.push(a.clone()),
        }
    }
    let text = flags.contains(&"--text");
    let replace = flags.contains(&"--replace");
    let arg = |i: usize| pos.get(i).cloned().ok_or_else(|| format!("missing argument\n\n{USAGE}"));
    let path = arg(0)?;
    let cmd = arg(1)?;
    let raw = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
    // Hard disks read with padding heads are laid out as the emulator sees them.
    let mut img = Disk::hard_auto(raw.clone()).data;
    let padded = img.len() != raw.len();
    let off = files::select(&img, part)?;
    if padded && matches!(cmd.as_str(), "put" | "mkdir" | "rm") {
        return Err("this image stores padding heads; save a plain copy first (the web page's Download image does), then edit that".into());
    }
    let e = |x: qnx2fs::Error| x.to_string();
    let mut changed = false;
    match cmd.as_str() {
        "ls" => {
            let dir = pos.get(2).cloned().unwrap_or_else(|| "/".into());
            let mut vol = files::open(&mut img, off).map_err(e)?;
            let mut list = vol.list(&dir).map_err(e)?;
            list.sort_by_key(|x| (!x.is_dir, x.name.to_lowercase()));
            for x in &list {
                let size = if x.is_dir { String::new() } else { x.size.to_string() };
                let date = x.modified().map(qnx2fs::format_date).unwrap_or_default();
                println!("{size:>8}  {date:19}  {}{}", x.name, if x.is_dir { "/" } else { "" });
            }
            println!("{} KB free of {} KB", vol.free_blocks() / 2, vol.total_blocks() / 2);
        }
        "cat" => {
            let data = files::read_file(&mut img, off, &arg(2)?, true).map_err(e)?;
            use std::io::Write;
            std::io::stdout().write_all(&data).map_err(|x| x.to_string())?;
        }
        "get" => {
            let src = arg(2)?;
            let data = files::read_file(&mut img, off, &src, text).map_err(e)?;
            let out = arg(3)?;
            std::fs::write(&out, &data).map_err(|x| format!("{out}: {x}"))?;
            // Give the copy the ICON file's date. The ICON kept local time, so
            // read it as local time here.
            let when = files::open(&mut img, off).and_then(|mut v| v.lookup(&src)).ok().and_then(|x| x.modified());
            if let Some(t) = when.and_then(local_to_system_time) {
                let _ = std::fs::File::options().write(true).open(&out).and_then(|f| f.set_modified(t));
            }
            println!("{} bytes to {out}", data.len());
        }
        "cp" => {
            // Into another image (or this one), written back to that image.
            let to_path = arg(3)?;
            let dir = arg(4)?;
            let same = std::fs::canonicalize(&to_path).ok() == std::fs::canonicalize(&path).ok();
            let to_raw = if same { raw.clone() } else { std::fs::read(&to_path).map_err(|x| format!("{to_path}: {x}"))? };
            let mut to = Disk::hard_auto(to_raw.clone()).data;
            if to.len() != to_raw.len() {
                return Err(format!("{to_path} stores padding heads; save a plain copy first, then copy into that"));
            }
            let to_off = files::select(&to, to_part)?;
            let r = files::copy(&img, off, &arg(2)?, &mut to, to_off, &dir, replace, now()).map_err(e)?;
            println!("copied {} files, {} folders", r.files, r.dirs);
            let tmp = format!("{to_path}.tmp");
            std::fs::write(&tmp, &to).map_err(|x| format!("{tmp}: {x}"))?;
            std::fs::rename(&tmp, &to_path).map_err(|x| format!("{to_path}: {x}"))?;
        }
        "put" => {
            let src = arg(2)?;
            let data = std::fs::read(&src).map_err(|x| format!("{src}: {x}"))?;
            let entry = files::write_file(&mut img, off, &arg(3)?, &data, text, replace, now()).map_err(e)?;
            println!("{} ({} bytes)", entry.path, entry.size);
            changed = true;
        }
        "mkdir" => {
            let mut vol = files::open(&mut img, off).map_err(e)?;
            vol.set_clock(now());
            vol.mkdir(&arg(2)?, &qnx2fs::NewEntry::default()).map_err(e)?;
            changed = true;
        }
        "rm" => {
            let mut vol = files::open(&mut img, off).map_err(e)?;
            vol.set_clock(now());
            let (f, d) = files::remove_tree(&mut vol, &arg(2)?).map_err(e)?;
            println!("deleted {f} files, {d} folders");
            changed = true;
        }
        "check" => {
            let r = files::open(&mut img, off).and_then(|mut v| v.check()).map_err(e)?;
            for i in r.errors.iter().chain(r.warnings.iter().filter(|w| !w.debug)) {
                println!("{}: {}", i.code, i.message);
            }
            println!(
                "{} files, {} folders; {} of {} blocks free; {} deleted entries ({} recoverable){}",
                r.files,
                r.dirs,
                r.free_blocks,
                r.total_blocks,
                r.deleted,
                r.recoverable,
                if r.is_clean() { "; clean" } else { "" }
            );
            if !r.is_clean() {
                return Err("the filesystem has errors".into());
            }
        }
        c => return Err(format!("unknown command {c:?}\n\n{USAGE}")),
    }
    if changed {
        // Write a temporary file and rename it, so a failure can't leave half an image.
        let tmp = format!("{path}.tmp");
        std::fs::write(&tmp, &img).map_err(|x| format!("{tmp}: {x}"))?;
        std::fs::rename(&tmp, &path).map_err(|x| format!("{path}: {x}"))?;
    }
    Ok(())
}
