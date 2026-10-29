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

/// A file's contents; with `text`, QNX text gets LF line endings.
pub fn read_file(image: &mut Vec<u8>, offset: u64, path: &str, text: bool) -> qnx2fs::Result<Vec<u8>> {
    let data = open(image, offset)?.get(path)?;
    Ok(if text && qnx2fs::looks_like_rs_text(&data) { qnx2fs::rs_to_lf(&data) } else { data })
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
    if replace {
        if let Ok(e) = vol.lookup(path) {
            if !e.is_dir {
                vol.remove(path)?;
            }
        }
    }
    let body = if text && looks_like_text(data) { qnx2fs::lf_to_rs(data) } else { data.to_vec() };
    vol.put(path, &body, &qnx2fs::NewEntry::default())
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
