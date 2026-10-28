//! ICON keyboard key numbers (manual Table D-1) and an ASCII helper.
//!
//! The keyboard sends key numbers 01h–66h on press and the same with bit 7
//! set on release; 7Fh is the auto-repeat tick. CAPS LOCK (3Fh) and CURSOR
//! LOCK (2Fh) report their lock state: make = locked, break = unlocked.
//! See docs/input_video.md §2.

pub const SHIFT: u8 = 0x30;
pub const CTRL: u8 = 0x21;
pub const REPEAT_TICK: u8 = 0x7F;
pub const RELEASE: u8 = 0x80;

/// Key number and whether Shift is needed to type `c`.
pub fn ascii_to_key(c: u8) -> Option<(u8, bool)> {
    const ROW_NUM: &[u8] = b"1234567890-=`";
    const ROW_NUM_SHIFT: &[u8] = b"!@#$%^&*()_+~";
    const ROW_Q: &[u8] = b"qwertyuiop";
    const ROW_A: &[u8] = b"asdfghjkl";
    const ROW_Z: &[u8] = b"zxcvbnm";
    let lc = c.to_ascii_lowercase();
    let shift = c.is_ascii_uppercase();
    if let Some(i) = ROW_NUM.iter().position(|&k| k == c) {
        return Some((0x02 + i as u8, false));
    }
    if let Some(i) = ROW_NUM_SHIFT.iter().position(|&k| k == c) {
        return Some((0x02 + i as u8, true));
    }
    if let Some(i) = ROW_Q.iter().position(|&k| k == lc) {
        return Some((0x12 + i as u8, shift));
    }
    if let Some(i) = ROW_A.iter().position(|&k| k == lc) {
        return Some((0x22 + i as u8, shift));
    }
    if let Some(i) = ROW_Z.iter().position(|&k| k == lc) {
        return Some((0x31 + i as u8, shift));
    }
    Some(match c {
        0x1B => (0x01, false),
        0x08 => (0x0F, false),
        b'\t' => (0x11, false),
        b'[' => (0x1F, false),
        b'{' => (0x1F, true),
        b']' => (0x20, false),
        b'}' => (0x20, true),
        b';' => (0x2B, false),
        b':' => (0x2B, true),
        b'\'' => (0x2C, false),
        b'"' => (0x2C, true),
        b'\r' | b'\n' => (0x2D, false),
        b',' => (0x38, false),
        b'<' => (0x38, true),
        b'.' => (0x39, false),
        b'>' => (0x39, true),
        b'/' => (0x3A, false),
        b'?' => (0x3A, true),
        b' ' => (0x40, false),
        0x7F => (0x42, false),
        b'\\' => (0x43, false),
        b'|' => (0x43, true),
        _ => return None,
    })
}

/// The press/release sequence that types `text`.
pub fn type_text(text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for c in text.bytes() {
        let Some((key, shift)) = ascii_to_key(c) else { continue };
        if shift {
            out.push(SHIFT);
        }
        out.push(key);
        out.push(key | RELEASE);
        if shift {
            out.push(SHIFT | RELEASE);
        }
    }
    out
}
