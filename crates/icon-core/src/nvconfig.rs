//! The ICON's NVRAM settings: decode and edit the 16 words by name, as the
//! ICON's own `/cmds/nv_config` reports them (docs/nvram_rtc_disk_install.md §1.3).
//!
//! Images are kept as 16 words. On disk they are 32 bytes, each word **low
//! byte first**: the order an EPROM programmer saves when it reads the
//! NMC9306 chip out of a real ICON in 16-bit mode.

use crate::devices::nvram::presets;

pub type Words = [u16; 16];

/// Size of an NVRAM image file.
pub const FILE_LEN: usize = 32;

/// Read a 32-byte image (words low byte first).
pub fn from_bin(b: &[u8]) -> Result<Words, String> {
    if b.len() != FILE_LEN {
        return Err(format!("an NVRAM image is {FILE_LEN} bytes (16 words), not {}", b.len()));
    }
    Ok(std::array::from_fn(|i| u16::from_le_bytes([b[2 * i], b[2 * i + 1]])))
}

/// The 32-byte image of `w` (words low byte first).
pub fn to_bin(w: &Words) -> Vec<u8> {
    w.iter().flat_map(|x| x.to_le_bytes()).collect()
}

/// Sixteen words as hex, w0 first: `998A 5173 ...`.
pub fn to_hex(w: &Words) -> String {
    w.iter().map(|x| format!("{x:04X}")).collect::<Vec<_>>().join(" ")
}

/// Parse sixteen hex words, separated by spaces or commas.
pub fn parse_hex(s: &str) -> Result<Words, String> {
    let v: Vec<u16> = s
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|x| !x.is_empty())
        .map(|x| u16::from_str_radix(x.trim_start_matches("0x"), 16).map_err(|_| format!("bad hex word {x:?}")))
        .collect::<Result<_, _>>()?;
    v.try_into().map_err(|v: Vec<u16>| format!("expected 16 words, got {}", v.len()))
}

/// Built-in images by name: `fileserver` (or `fs`), `workstation` (or `ws`).
pub fn preset(name: &str) -> Option<Words> {
    match name {
        "fileserver" | "fs" => Some(presets::fileserver()),
        "workstation" | "ws" => Some(presets::workstation()),
        _ => None,
    }
}

/// Boot loaders in the order the ROM tries them, with their word 0 disable bit.
const LOADERS: [(&str, u16); 4] = [("floppy", 0x2000), ("network", 0x1000), ("hd", 0x4000), ("rom", 0x8000)];

/// Word 0 options: name, bit, and whether a set bit means the option is present.
const OPTIONS: [(&str, u16, bool, &str); 6] = [
    ("local-floppy", 0x0008, false, "on-board 1797 floppy controller (pedestal drive)"),
    ("serial-ports", 0x0010, false, "serial ports and baud-rate counter"),
    ("rtc", 0x0020, false, "battery real-time clock"),
    ("keyboard", 0x0080, false, "keyboard"),
    ("speech", 0x0100, false, "TMS5220 speech chip"),
    ("board", 0x0004, true, "fileserver (WD1002-05 disk controller) or workstation"),
];

/// Every field `set` accepts, with the values it takes.
pub const FIELDS: [(&str, &str); 17] = [
    ("loaders", "boot loaders to try, any of floppy,network,hd,rom (tried in that order), or none"),
    ("board", "fileserver | workstation (word 0 bit 2: which floppy path the ROM uses)"),
    ("ram", "128 | 256 | 384 | 512 | 1024 (KB; 1024 = the CEMCORP 1 MB card)"),
    ("local-floppy", "yes | no (on-board 1797 floppy controller)"),
    ("serial-ports", "yes | no"),
    ("rtc", "yes | no (battery clock)"),
    ("keyboard", "yes | no"),
    ("speech", "yes | no (TMS5220)"),
    ("node", "0-255, this machine's network node number"),
    ("server", "0-255, the fileserver it boots from (its own node = it is the network fileserver)"),
    ("boot-file", "0-15, the boot file type it asks the server for (1 = ws_boot, 2 = ws_fsys_boot)"),
    ("monitor", "rgb | icon-colour | roland-colour | icon-mono | roland-mono"),
    ("hw-serial", "hardware serial number, e.g. 707-870712-867"),
    ("sw-serial", "software serial number, e.g. 300-850803-188"),
    ("palette", "word 11, four hex digits (the colour/attribute map)"),
    ("crtc", "CRTC registers R0-R9, ten hex bytes"),
    ("wN / wN.B", "raw: word N (0-15) as hex, or bit B of word N as 0 | 1"),
];

fn yes_no(v: &str) -> Result<bool, String> {
    match v {
        "yes" | "y" | "1" | "on" | "true" => Ok(true),
        "no" | "n" | "0" | "off" | "false" => Ok(false),
        _ => Err(format!("expected yes or no, not {v:?}")),
    }
}

fn num(v: &str, max: u32) -> Result<u16, String> {
    let n = match v.strip_prefix("0x") {
        Some(h) => u32::from_str_radix(h, 16),
        None => v.parse(),
    }
    .map_err(|_| format!("expected a number, not {v:?}"))?;
    if n > max {
        return Err(format!("{n} is out of range (0-{max})"));
    }
    Ok(n as u16)
}

/// The enabled loaders, in the order tried.
pub fn loaders(w: &Words) -> Vec<&'static str> {
    LOADERS.iter().filter(|l| w[0] & l.1 == 0).map(|l| l.0).collect()
}

/// Memory size word 0 asks the ROM to test, in KB (None: not a valid setting).
pub fn ram_kb(w: &Words) -> Option<u32> {
    match w[0] & 0x0803 {
        0x0803 => Some(128),
        0x0800 => Some(256),
        0x0801 => Some(384),
        0x0802 => Some(512),
        0x0001 => Some(1024),
        _ => None,
    }
}

/// Monitor setting as `set` names it, and as nv_config prints it.
pub fn monitor(w: &Words) -> (&'static str, &'static str) {
    match (w[13] >> 2) & 3 {
        1 => ("rgb", "Colour RGB"),
        2 if w[8] >> 8 == 0x3D => ("icon-colour", "ICON Colour"),
        2 => ("roland-colour", "Roland Colour CC-141"),
        3 if w[13] & 0xF == 0xE => ("icon-mono", "ICON Monochrome"),
        3 => ("roland-mono", "Roland Monochrome MB-121A"),
        _ => ("unknown", "unknown"),
    }
}

/// A serial number from two words, printed as nv_config does:
/// `%d%02d-%02d%02d%02d-%03d` from b, a, year, month, day, sequence.
pub fn serial(hi: u16, lo: u16) -> String {
    let v = (hi as u32) << 16 | lo as u32;
    let (a, b) = (v >> 28, (v >> 24) & 15);
    let (yy, mm, dd, seq) = (((v >> 20) & 15) + 80, (v >> 16) & 15, (v >> 10) & 31, v & 0x3FF);
    format!("{b}{a:02}-{yy:02}{mm:02}{dd:02}-{seq:03}")
}

/// Pack a serial number; bit 15 of the low word (unused) is kept from `lo`.
fn parse_serial(s: &str, lo: u16) -> Result<(u16, u16), String> {
    let bad = || format!("expected a serial number like 707-870712-867, not {s:?}");
    let parts: Vec<&str> = s.split('-').collect();
    let [ba, date, seq] = parts[..] else { return Err(bad()) };
    if ba.len() < 3 || date.len() != 6 || !s.chars().all(|c| c.is_ascii_digit() || c == '-') {
        return Err(bad());
    }
    let n = |x: &str| x.parse::<u32>().map_err(|_| bad());
    let (b, a) = (n(&ba[..ba.len() - 2])?, n(&ba[ba.len() - 2..])?);
    let (yy, mm, dd, seq) = (n(&date[..2])?, n(&date[2..4])?, n(&date[4..])?, n(seq)?);
    if a > 15 || b > 15 || !(80..=95).contains(&yy) || mm > 15 || dd > 31 || seq > 1023 {
        return Err(format!("{s:?} does not fit: the first two numbers 0-15, year 80-95, month 0-15, day 0-31, last 0-1023"));
    }
    let v = a << 28 | b << 24 | (yy - 80) << 20 | mm << 16 | dd << 10 | seq | (lo as u32 & 0x8000);
    Ok(((v >> 16) as u16, v as u16))
}

/// CRTC R0-R9 from words 5-9 (low byte = even register).
pub fn crtc(w: &Words) -> [u8; 10] {
    std::array::from_fn(|r| (w[5 + r / 2] >> (8 * (r % 2))) as u8)
}

/// One decoded setting: the name `set` takes, its value in that form, and
/// the line nv_config (or the hardware notes) would describe it with.
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub name: &'static str,
    pub value: String,
    pub meaning: String,
}

/// Every named setting in `w`.
pub fn fields(w: &Words) -> Vec<Field> {
    let mut out = Vec::new();
    let mut f = |name, value: String, meaning: String| out.push(Field { name, value, meaning });
    let l = loaders(w);
    let order = if l.is_empty() { "none (the ROM has nothing to boot from)".into() } else { l.join(" → ") };
    f("loaders", if l.is_empty() { "none".into() } else { l.join(",") }, format!("boot: {order}"));
    let fs = w[0] & 0x0004 == 0;
    f("board", if fs { "fileserver" } else { "workstation" }.into(),
      if fs { "fileserver board: the floppy loader goes through the WD1002-05" } else { "workstation board: the floppy loader uses the on-board 1797" }.into());
    let ram = ram_kb(w);
    f("ram", ram.map_or(format!("0x{:04X}", w[0] & 0x0803), |k| k.to_string()),
      match ram {
          Some(1024) => "1 Megabyte (CEMCORP card)".into(),
          Some(k) => format!("{k} Kbytes"),
          None => "unknown memory setting (the ROM hangs on the memory test)".into(),
      });
    for (name, bit, inverted, what) in OPTIONS.iter().take(5) {
        let present = (w[0] & bit != 0) == *inverted;
        f(name, if present { "yes" } else { "no" }.into(), format!("{what}: {}", if present { "installed" } else { "not installed" }));
    }
    let (node, server) = (w[10] >> 8, w[10] & 0xFF);
    f("node", node.to_string(), format!("network node number {node}"));
    f("server", server.to_string(), if node == server && node != 0 {
        "boots and serves as the network fileserver (server = own node)".into()
    } else {
        format!("boots from the fileserver on node {server}")
    });
    f("boot-file", (w[12] & 0xF).to_string(), format!("boot file type {}", w[12] & 0xF));
    let (m, mname) = monitor(w);
    f("monitor", m.into(), format!("monitor: {mname}"));
    f("hw-serial", serial(w[1], w[2]), "hardware serial number".into());
    f("sw-serial", serial(w[3], w[4]), "software serial number".into());
    f("palette", format!("{:04X}", w[11]), "colour/attribute map (nibbles to F900-F906)".into());
    let c = crtc(w);
    f("crtc", c.iter().map(|x| format!("{x:02X}")).collect::<Vec<_>>().join(" "),
      format!("CRTC: {} chars/line ({} shown), {} rows ({} shown), {} lines per row", c[0] as u32 + 1, c[1], c[4] as u32 + 1, c[6], c[9] as u32 + 1));
    out
}

/// Bits no field above covers, as `(word, mask)` with their current value.
pub fn other_bits(w: &Words) -> Vec<(usize, u16, u16)> {
    const KNOWN: [(usize, u16); 3] = [(0, 0xF9BF), (12, 0x000F), (13, 0x000C)];
    let mut v = Vec::new();
    for (i, mask) in [(0usize, 0xFFFFu16), (12, 0xFFFF), (13, 0xFFFF)] {
        let known = KNOWN.iter().find(|k| k.0 == i).map_or(0, |k| k.1);
        let mut rest = mask & !known;
        if i == 13 && monitor(w).0.ends_with("mono") {
            rest &= !0x3; // bits 0-1 pick ICON vs Roland monochrome
        }
        if rest != 0 {
            v.push((i, rest, w[i] & rest));
        }
    }
    v
}

/// A text report in the spirit of the ICON's `nv_config`.
pub fn report(w: &Words) -> String {
    let mut s = String::new();
    for f in fields(w) {
        s += &format!("{:<13} {:<29} {}\n", f.name, f.value, f.meaning);
    }
    s += "other bits   ";
    for (i, mask, val) in other_bits(w) {
        s += &format!(" w{i}&{mask:04X}={val:04X}");
    }
    s += "\n";
    if w[0] & 0x0400 != 0 || ram_kb(w) == Some(1024) {
        s += &format!("              w0 bit 10 = {}: with the 1 MB card the Rel 2.11 ROM needs it set (else Diag error 50)\n", (w[0] >> 10) & 1);
    }
    s += &format!("words         {}\n", to_hex(w));
    s
}

/// Change one setting. `name` is a [`FIELDS`] name, `wN` or `wN.B`.
pub fn set(w: &mut Words, name: &str, value: &str) -> Result<(), String> {
    let value = value.trim();
    let lower = value.to_ascii_lowercase();
    let v = lower.as_str();
    if let Some(opt) = OPTIONS.iter().take(5).find(|o| o.0 == name) {
        let present = yes_no(v)?;
        w[0] = if present == opt.2 { w[0] | opt.1 } else { w[0] & !opt.1 };
        return Ok(());
    }
    match name {
        "loaders" => {
            let mut disable = 0xF000;
            for l in v.split(',').map(str::trim).filter(|x| !x.is_empty() && *x != "none") {
                let l = match l { "net" | "server" => "network", "hard-disk" | "harddisk" => "hd", "fd" | "diskette" => "floppy", x => x };
                let bit = LOADERS.iter().find(|x| x.0 == l).ok_or_else(|| format!("unknown loader {l:?} (floppy, network, hd, rom)"))?.1;
                disable &= !bit;
            }
            w[0] = (w[0] & 0x0FFF) | disable;
        }
        "board" => match v {
            "fileserver" | "fs" => w[0] &= !0x0004,
            "workstation" | "ws" => w[0] |= 0x0004,
            _ => return Err(format!("board is fileserver or workstation, not {value:?}")),
        },
        "ram" => {
            let kb = num(v.trim_end_matches("kb").trim_end_matches('k'), 1024)? as u32;
            if ![128, 256, 384, 512, 1024].contains(&kb) {
                return Err(format!("ram is 128, 256, 384, 512 or 1024, not {kb}"));
            }
            presets::set_ram_kb(w, kb);
        }
        "node" => w[10] = (w[10] & 0x00FF) | num(v, 255)? << 8,
        "server" => w[10] = (w[10] & 0xFF00) | num(v, 255)?,
        "boot-file" => w[12] = (w[12] & 0xFFF0) | num(v, 15)?,
        "monitor" => {
            let t = |w: &mut Words, t: u16| w[13] = (w[13] & !0x000C) | t << 2;
            let r7 = |w: &mut Words, r: u16| w[8] = (w[8] & 0x00FF) | r << 8;
            match v {
                "rgb" => t(w, 1),
                "icon-colour" | "icon-color" => {
                    t(w, 2);
                    r7(w, 0x3D);
                }
                "roland-colour" | "roland-color" => {
                    t(w, 2);
                    if w[8] >> 8 == 0x3D {
                        r7(w, 0x3C);
                    }
                }
                "icon-mono" => w[13] = (w[13] & 0xFFF0) | 0xE,
                "roland-mono" => w[13] = (w[13] & 0xFFF0) | 0xF,
                _ => return Err(format!("monitor is rgb, icon-colour, roland-colour, icon-mono or roland-mono, not {value:?}")),
            }
        }
        "hw-serial" => (w[1], w[2]) = parse_serial(value, w[2])?,
        "sw-serial" => (w[3], w[4]) = parse_serial(value, w[4])?,
        "palette" => w[11] = u16::from_str_radix(v.trim_start_matches("0x"), 16).map_err(|_| format!("palette is four hex digits, not {value:?}"))?,
        "crtc" => {
            let r: Vec<u8> = v
                .split(|c: char| c.is_whitespace() || c == ',')
                .filter(|x| !x.is_empty())
                .map(|x| u8::from_str_radix(x.trim_start_matches("0x"), 16).map_err(|_| format!("bad CRTC byte {x:?}")))
                .collect::<Result<_, _>>()?;
            if r.len() != 10 {
                return Err(format!("crtc takes ten hex bytes (R0-R9), got {}", r.len()));
            }
            for i in 0..5 {
                w[5 + i] = u16::from_le_bytes([r[2 * i], r[2 * i + 1]]);
            }
        }
        _ => {
            let raw = name.strip_prefix('w').ok_or_else(|| format!("unknown setting {name:?} (icon-cli nvram --help lists them)"))?;
            let (word, bit) = match raw.split_once('.') {
                Some((a, b)) => (a, Some(b)),
                None => (raw, None),
            };
            let i = word.parse::<usize>().ok().filter(|&i| i < 16).ok_or_else(|| format!("unknown setting {name:?}"))?;
            match bit {
                Some(b) => {
                    let b = b.parse::<u16>().ok().filter(|&b| b < 16).ok_or_else(|| format!("bad bit number in {name:?} (0-15)"))?;
                    w[i] = if yes_no(v)? { w[i] | 1 << b } else { w[i] & !(1 << b) };
                }
                None => w[i] = u16::from_str_radix(v.trim_start_matches("0x"), 16).map_err(|_| format!("{name} is a hex word, not {value:?}"))?,
            }
        }
    }
    Ok(())
}

/// Apply `name=value` assignments in order.
pub fn apply<'a>(w: &mut Words, assignments: impl IntoIterator<Item = &'a str>) -> Result<(), String> {
    for a in assignments {
        let (k, v) = a.split_once('=').ok_or_else(|| format!("expected name=value, not {a:?}"))?;
        set(w, k.trim(), v).map_err(|e| format!("{}: {e}", k.trim()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Read out of a real fileserver (1 MB card, Roland monitor) with a chip
    /// programmer, and the same machine as it came: the bytes as saved.
    const REAL_FS: &str = "7d06 7777 6333 5803 bc0c 3828 2d44 4104 3c3c 0003 0101 f0f0 0101 faeb ffff ffff";
    const REAL_ORIG: &str = "7de6 5803 bc0c 5803 bc0c 3828 2d44 4100 3c3c 0003 01fe f0f0 0100 faeb ffff ffff";

    fn bin(s: &str) -> Words {
        let bytes: Vec<u8> = s.split_whitespace().flat_map(|x| u16::from_str_radix(x, 16).unwrap().to_be_bytes()).collect();
        from_bin(&bytes).unwrap()
    }
    fn get(w: &Words, name: &str) -> String {
        fields(w).into_iter().find(|f| f.name == name).unwrap().value
    }

    #[test]
    fn real_fileserver_decodes_as_nv_config_did() {
        let w = bin(REAL_FS);
        assert_eq!(w[0], 0x067D);
        assert_eq!(get(&w, "hw-serial"), "707-870712-867");
        assert_eq!(get(&w, "sw-serial"), "300-850803-188");
        assert_eq!(get(&w, "node"), "1");
        assert_eq!(get(&w, "server"), "1");
        assert_eq!(get(&w, "ram"), "1024");
        assert_eq!(get(&w, "monitor"), "roland-colour");
        assert_eq!(get(&w, "boot-file"), "1");
        assert_eq!(get(&w, "loaders"), "floppy,network,hd,rom");
        assert_eq!(get(&w, "board"), "workstation");
        assert_eq!(get(&w, "crtc"), "38 28 2D 44 41 04 3C 3C 00 03");
        assert_eq!(to_bin(&w), bin(REAL_FS).iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<_>>());
    }

    #[test]
    fn real_original_is_a_factory_workstation() {
        let w = bin(REAL_ORIG);
        assert_eq!(get(&w, "node"), "254");
        assert_eq!(get(&w, "server"), "1");
        assert_eq!(get(&w, "loaders"), "network");
        assert_eq!(get(&w, "hw-serial"), "300-850803-188");
    }

    /// The bit flips logged on the real machine (NVRAM_bits.txt), as `set` does them.
    #[test]
    fn matches_bench_tests() {
        let mut w = bin(REAL_FS);
        set(&mut w, "w2.7", "1").unwrap();
        assert_eq!(get(&w, "hw-serial"), "707-870712-995");
        let mut w = bin(REAL_FS);
        set(&mut w, "w1.11", "1").unwrap();
        assert_eq!(get(&w, "hw-serial"), "1507-870712-867");
        let mut w = bin(REAL_FS);
        set(&mut w, "w12.3", "1").unwrap();
        assert_eq!(get(&w, "boot-file"), "9");
        let mut w = bin(REAL_FS);
        set(&mut w, "w13.2", "1").unwrap();
        assert_eq!(monitor(&w).1, "ICON Monochrome");
        let mut w = bin(REAL_FS);
        set(&mut w, "w10.7", "1").unwrap();
        assert_eq!(get(&w, "server"), "129");
        // 7916 in the bench notes: fileserver floppy path, no network loader.
        let mut w = bin(REAL_FS);
        apply(&mut w, ["board=fileserver", "loaders=floppy,hd,rom"]).unwrap();
        assert_eq!(w[0].to_le_bytes(), [0x79, 0x16]);
    }

    #[test]
    fn set_round_trips() {
        let mut w = presets::fileserver();
        apply(&mut w, ["hw-serial=1507-951231-1023", "node=200", "server=1", "monitor=icon-colour", "ram=1024", "keyboard=no", "w14=1234"]).unwrap();
        assert_eq!(get(&w, "hw-serial"), "1507-951231-1023");
        assert_eq!(get(&w, "node"), "200");
        assert_eq!(get(&w, "monitor"), "icon-colour");
        assert_eq!(get(&w, "ram"), "1024");
        assert_eq!(get(&w, "keyboard"), "no");
        assert_eq!(w[14], 0x1234);
        assert_eq!(parse_hex(&to_hex(&w)).unwrap(), w);
        assert!(set(&mut w, "node", "256").is_err());
        assert!(set(&mut w, "hw-serial", "707-790712-867").is_err());
        assert!(set(&mut w, "nonsense", "1").is_err());
    }
}
