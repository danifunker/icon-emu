//! Files on the ICON's disks, for moving them in and out from the host: find
//! the QNX 2 volumes in a diskette or hard-disk image, and edit them with the
//! `qnx2fs` crate.
//!
//! Never edit a disk that a running fileserver has mounted: QNX caches it
//! (the hard disk especially) and would write its own view back over the change.

use std::io::Cursor;

pub use qnx2fs;
use qnx2fs::Volume;

/// One QNX 2 volume in a disk image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeInfo {
    pub label: String,
    /// Byte offset of the volume's block 1.
    pub offset: u64,
}

/// The QNX 2 volumes in a disk image. A diskette is one volume at offset 0.
/// An ICON hard disk has a PC-style partition table in its first sector;
/// types 07, 08 and 09 are QNX 2, and their start is a 1-based block number.
pub fn volumes(image: &[u8]) -> Vec<VolumeInfo> {
    let is_qnx = |offset: u64| qnx2fs::looks_like_qnx2(&mut Cursor::new(image), offset);
    if is_qnx(0) {
        return vec![VolumeInfo { label: "whole disk".into(), offset: 0 }];
    }
    let mut out = Vec::new();
    if image.len() >= 512 && image[510..512] == [0x55, 0xAA] {
        for i in 0..4 {
            let e = 0x1BE + 16 * i;
            let start = u32::from_le_bytes([image[e + 8], image[e + 9], image[e + 10], image[e + 11]]) as u64;
            if (0x07..=0x09).contains(&image[e + 4]) && start >= 1 && is_qnx((start - 1) * 512) {
                out.push(VolumeInfo { label: format!("partition {}", i + 1), offset: (start - 1) * 512 });
            }
        }
    }
    out
}

/// Open the volume at `offset` for reading and editing in place.
pub fn open(image: &mut Vec<u8>, offset: u64) -> qnx2fs::Result<Volume<Cursor<&mut Vec<u8>>>> {
    Volume::open(Cursor::new(image), offset)
}

/// The volume to use: `part` 0 means the first one (a diskette's only volume,
/// or a hard disk's first partition); `n` means hard-disk partition `n`.
pub fn select(image: &[u8], part: usize) -> Result<u64, String> {
    let vols = volumes(image);
    let found = if part == 0 {
        vols.first()
    } else {
        vols.iter().find(|v| v.label == format!("partition {part}") || (part == 1 && v.offset == 0))
    };
    found.map(|v| v.offset).ok_or_else(|| {
        if vols.is_empty() {
            "no QNX filesystem on this disk (a blank diskette must be formatted: use a new pre-formatted one)".into()
        } else {
            format!("no QNX partition {part} (found: {})", vols.iter().map(|v| v.label.as_str()).collect::<Vec<_>>().join(", "))
        }
    })
}

/// A file's contents. With `text`, an ICON text file comes back as UTF-8 with
/// LF line ends (accents and underline codes decoded, see [`crate::text`]);
/// anything else is returned as it is.
pub fn read_file(image: &mut Vec<u8>, offset: u64, path: &str, text: bool) -> qnx2fs::Result<Vec<u8>> {
    let data = open(image, offset)?.get(path)?;
    if !text {
        return Ok(data);
    }
    Ok(match crate::text::decode(&data) {
        Some(d) if d.exact => d.text.into_bytes(),
        _ if qnx2fs::looks_like_rs_text(&data) => qnx2fs::rs_to_lf(&data),
        _ => data,
    })
}

/// Host text as the ICON stores it: UTF-8 is encoded (RS line ends, Telidon
/// accents); other text only gets RS line ends; binary data is unchanged.
pub fn host_text_to_icon(data: &[u8]) -> qnx2fs::Result<Vec<u8>> {
    match std::str::from_utf8(data) {
        Ok(s) if crate::text::is_text(data) => {
            crate::text::encode(&s.replace("\r\n", "\n"), crate::text::ICON_DEFAULT).map_err(|bad| unstorable(&bad))
        }
        _ if looks_like_text(data) => Ok(qnx2fs::lf_to_rs(data)),
        _ => Ok(data.to_vec()),
    }
}

/// The error for characters the ICON has no way to store.
pub fn unstorable(bad: &[char]) -> qnx2fs::Error {
    let list: Vec<String> = bad.iter().map(|c| format!("{c} (U+{:04X})", *c as u32)).collect();
    qnx2fs::Error::InvalidData(format!("the ICON can't store these characters: {}", list.join(", ")))
}

/// Write a file. With `text`, host text gets QNX's RS line endings; with
/// `replace`, an existing file of that name is deleted first. `now` is Unix
/// seconds as the ICON's clock reads (see `devices::rtc::icon_time`).
pub fn write_file(
    image: &mut Vec<u8>,
    offset: u64,
    path: &str,
    data: &[u8],
    text: bool,
    replace: bool,
    now: i64,
) -> qnx2fs::Result<qnx2fs::Entry> {
    let mut vol = open(image, offset)?;
    vol.set_clock(now);
    let body = if text { host_text_to_icon(data)? } else { data.to_vec() };
    if replace {
        // A file already there keeps its permissions and owner.
        vol.replace(path, &body, &qnx2fs::NewEntry::default())
    } else {
        vol.put(path, &body, &qnx2fs::NewEntry::default())
    }
}

/// Copy the file or folder at `src_path` (volume `src_offset` of `src`) into
/// the folder `dst_dir` of volume `dst_offset` in `dst`, keeping permissions,
/// owners and dates. `src` and `dst` may be the same disk's bytes: the source
/// is read from its own snapshot.
#[allow(clippy::too_many_arguments)]
pub fn copy(
    src: &[u8],
    src_offset: u64,
    src_path: &str,
    dst: &mut Vec<u8>,
    dst_offset: u64,
    dst_dir: &str,
    replace: bool,
    now: i64,
) -> qnx2fs::Result<qnx2fs::CopyReport> {
    let mut from = Volume::open(Cursor::new(src.to_vec()), src_offset)?;
    let mut to = open(dst, dst_offset)?;
    to.set_clock(now);
    let dst_dir = if dst_dir.is_empty() { "/" } else { dst_dir };
    // The source is its own snapshot, so even a folder copied into itself
    // is copied once, as it was.
    to.copy_from(&mut from, src_path, dst_dir, replace)
}

// ---- Watcom 3.00 on system 4.10
//
// Watcom's 1987 programs for the ICON (the editor `wedit` and the Pascal
// runtime `pascal_m`, from the Pascal 3.00 diskette) save the screen palette
// at start-up from the kernel's word at sys:0x6C and set it again. In the
// 640-pixel mode they take colour registers 0-1 from the low nibble and 2-3
// from the high nibble of the word's *low byte*: right for the 4.01 kernel,
// which keeps NVRAM's palette word there as it is (F4F4), wrong for 4.10,
// which stores one nibble per register (FF44). On a 4.10 system every
// register becomes 4 and the screen is red on red. Taking registers 2-3 from
// the high nibble of the *high byte* (`mov ah,bh` for `mov ah,bl`) is right
// for both kernels.

/// The 640-pixel palette routine: regs 0,1 <- bl & 0F; regs 2,3 <- (bl|bh) & F0.
fn watcom_palette_routine(data: &[u8]) -> Option<(usize, bool)> {
    const HEAD: [u8; 7] = [0x8A, 0xE3, 0x80, 0xE4, 0x0F, 0x2A, 0xC0];
    data.windows(21).enumerate().find_map(|(i, w)| {
        let shape = w[..7] == HEAD
            && w[7] == 0xE8
            && w[10..12] == [0xFE, 0xC0]
            && w[12] == 0xE8
            && w[15] == 0x8A
            && w[17..20] == [0x80, 0xE4, 0xF0];
        match (shape, w[16]) {
            (true, 0xE3) => Some((i + 16, false)),
            (true, 0xE7) => Some((i + 16, true)),
            _ => None,
        }
    })
}

/// Programs in `/util` with Watcom's palette bug, not yet fixed.
pub fn watcom_unfixed(image: &mut Vec<u8>, offset: u64) -> qnx2fs::Result<Vec<String>> {
    let mut vol = open(image, offset)?;
    let mut out = Vec::new();
    for e in vol.list("/util").unwrap_or_default() {
        if !e.is_dir && e.size < 1 << 20 && matches!(watcom_palette_routine(&vol.read_file(&e, usize::MAX)?), Some((_, false))) {
            out.push(e.path);
        }
    }
    Ok(out)
}

/// Fix Watcom's palette bug in every program in `/util` that has it, keeping
/// each file's permissions, owner and date. Returns the paths it changed.
pub fn fix_watcom(image: &mut Vec<u8>, offset: u64) -> qnx2fs::Result<Vec<String>> {
    let paths = watcom_unfixed(image, offset)?;
    let mut vol = open(image, offset)?;
    for path in &paths {
        let e = vol.lookup(path)?;
        let mut data = vol.read_file(&e, usize::MAX)?;
        if let Some((at, false)) = watcom_palette_routine(&data) {
            data[at] = 0xE7; // mov ah,bh
            let keep = qnx2fs::NewEntry { mtime: e.modified(), ..qnx2fs::NewEntry::like(&e) };
            vol.replace(path, &data, &keep)?;
        }
    }
    Ok(paths)
}

/// Files and folders inside the folder at `path` (not counting it).
pub fn count_tree<D: std::io::Read + std::io::Seek>(vol: &mut Volume<D>, path: &str) -> qnx2fs::Result<(usize, usize)> {
    let (mut files, mut dirs) = (0, 0);
    for e in vol.list(path)? {
        if e.is_dir {
            let (f, d) = count_tree(vol, &e.path)?;
            files += f;
            dirs += d + 1;
        } else {
            files += 1;
        }
    }
    Ok((files, dirs))
}

/// Delete a file, or a folder and everything in it. Returns (files, folders) deleted.
pub fn remove_tree<D: std::io::Read + std::io::Write + std::io::Seek>(
    vol: &mut Volume<D>,
    path: &str,
) -> qnx2fs::Result<(usize, usize)> {
    let e = vol.lookup(path)?;
    if path.trim_matches('/').is_empty() {
        return Err(qnx2fs::Error::InvalidData("the top folder of a disk can't be deleted".into()));
    }
    let mut n = (0, 0);
    if e.is_dir {
        for child in vol.list(path)? {
            let (f, d) = remove_tree(vol, &child.path)?;
            n.0 += f;
            n.1 += d;
        }
        n.1 += 1;
    } else {
        n.0 += 1;
    }
    vol.remove(path)?;
    Ok(n)
}

/// Whether `data` looks like host text (LF or CR LF lines) that should get
/// QNX's RS line endings: no NULs and hardly any other control bytes.
pub fn looks_like_text(data: &[u8]) -> bool {
    if data.is_empty() || data.contains(&0) || !data.contains(&b'\n') {
        return false;
    }
    let odd = data
        .iter()
        .filter(|&&b| b < 0x20 && !matches!(b, b'\t' | b'\n' | b'\r' | 0x0C | 0x1B))
        .count();
    odd * 100 <= data.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::devices::wd1002::Disk;
    use qnx2fs::NewEntry;

    #[test]
    fn a_blank_diskette_takes_a_file_and_gives_it_back() {
        let mut img = Disk::formatted_floppy_640k();
        let vols = volumes(&img);
        assert_eq!(vols, [VolumeInfo { label: "whole disk".into(), offset: 0 }]);
        let text = b"one\ntwo\n";
        assert!(looks_like_text(text));
        {
            let mut vol = open(&mut img, 0).unwrap();
            vol.set_clock(700_000_000);
            vol.put("/notes", &qnx2fs::lf_to_rs(text), &NewEntry::default()).unwrap();
        }
        let mut vol = open(&mut img, 0).unwrap();
        assert_eq!(qnx2fs::rs_to_lf(&vol.get("/notes").unwrap()), text);
        assert!(vol.check().unwrap().is_clean());
    }

    #[test]
    fn host_text_gets_icon_accents_and_folders_delete_recursively() {
        assert_eq!(host_text_to_icon("café\r\nça\n".as_bytes()).unwrap(), b"caf\x19\x42e\x1e\x19\x4bca\x1e");
        assert_eq!(host_text_to_icon("é".as_bytes()).unwrap(), b"\x19\x42e");
        assert!(host_text_to_icon("emoji 😀\n".as_bytes()).is_err());
        let bin = [0u8, 1, 2, b'\n', 0xFF];
        assert_eq!(host_text_to_icon(&bin).unwrap(), bin);

        let mut img = Disk::formatted_floppy_640k();
        let mut vol = open(&mut img, 0).unwrap();
        vol.set_clock(700_000_000);
        vol.mkdir("/a", &NewEntry::default()).unwrap();
        vol.mkdir("/a/b", &NewEntry::default()).unwrap();
        vol.put("/a/one", b"1", &NewEntry::default()).unwrap();
        vol.put("/a/b/two", b"2", &NewEntry::default()).unwrap();
        let free = vol.free_blocks();
        assert_eq!(count_tree(&mut vol, "/a").unwrap(), (2, 1));
        assert_eq!(remove_tree(&mut vol, "/a").unwrap(), (2, 2));
        assert!(vol.lookup("/a").is_err());
        assert!(vol.free_blocks() > free);
        assert!(remove_tree(&mut vol, "/").is_err());
        assert!(vol.check().unwrap().is_clean());
    }

    #[test]
    fn watcom_palette_bug_is_found_fixed_once_and_dates_kept() {
        // wedit's 640-pixel palette routine, with calls to a setter at +0x15.
        let routine = [
            0x8A, 0xE3, 0x80, 0xE4, 0x0F, 0x2A, 0xC0, 0xE8, 0x19, 0x00, 0xFE, 0xC0, 0xE8, 0x14, 0x00, 0x8A, 0xE3, 0x80,
            0xE4, 0xF0, 0xD0, 0xEC,
        ];
        let mut prog = vec![0x90u8; 64];
        prog.extend_from_slice(&routine);
        assert_eq!(watcom_palette_routine(&prog), Some((64 + 16, false)));
        let mut img = Disk::formatted_floppy_640k();
        {
            let mut vol = open(&mut img, 0).unwrap();
            vol.set_clock(560_000_000); // 1987
            vol.mkdir("/util", &NewEntry::default()).unwrap();
            vol.put("/util/wedit", &prog, &NewEntry { mode: Some(0o555), ..Default::default() }).unwrap();
            vol.put("/util/other", b"not watcom", &NewEntry::default()).unwrap();
        }
        let before = open(&mut img, 0).unwrap().lookup("/util/wedit").unwrap();
        assert_eq!(watcom_unfixed(&mut img, 0).unwrap(), ["/util/wedit"]);
        assert_eq!(fix_watcom(&mut img, 0).unwrap(), ["/util/wedit"]);
        assert!(fix_watcom(&mut img, 0).unwrap().is_empty());
        let mut vol = open(&mut img, 0).unwrap();
        let after = vol.lookup("/util/wedit").unwrap();
        assert_eq!(after.attributes(), before.attributes());
        assert_eq!(vol.get("/util/wedit").unwrap()[64 + 16], 0xE7);
        assert!(vol.check().unwrap().is_clean());
    }

    #[test]
    fn hard_disk_partitions_are_found_by_type_and_one_based_start() {
        // A 64 MB-style table: one QNX partition starting at block 30, holding a diskette's volume.
        let fd = Disk::formatted_floppy_640k();
        let mut img = vec![0u8; 29 * 512 + fd.len()];
        img[29 * 512..].copy_from_slice(&fd);
        img[0x1BE + 4] = 0x07;
        img[0x1BE + 8..0x1BE + 12].copy_from_slice(&30u32.to_le_bytes());
        img[510] = 0x55;
        img[511] = 0xAA;
        assert_eq!(volumes(&img), [VolumeInfo { label: "partition 1".into(), offset: 29 * 512 }]);
        assert!(!looks_like_text(b"\x00binary\n"));
    }
}
