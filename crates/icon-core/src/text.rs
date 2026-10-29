//! ICON text files <-> Unicode, for editing them on the host.
//!
//! What ICON text files hold (checked on the 4.10.01 system and Help disks):
//! - Lines end with RS (0x1E), QNX 2's line separator.
//! - Accented letters are Telidon (T.61) sequences, `0x19 <diacritic> <letter>`:
//!   `19 42 65` is é, `19 41 61` à, `19 43 6F` ô, `19 4B 63` ç. This is what
//!   the ICON's own editor and ICON-mode keyboard produce.
//! - 0x99 and 0x9A switch underline on and off (Telidon display codes; Help
//!   pages underline their headings this way, as `cat` on an ICON shows).
//!   They decode to [`UNDERLINE_ON`] ⟦ and [`UNDERLINE_OFF`] ⟧.
//! - Other bytes 0x80-0xFF are code page 437 (QNX mode, the IBM PC character
//!   set), e.g. the box drawing in `/config/iconnet_logo`.
//!
//! [`decode`] remembers which line ending and accent style a file uses, so
//! [`encode`] writes it back the same way. [`Decoded::exact`] says whether
//! that round trip reproduces the file byte for byte.

use crate::text_tables::{COMPOSED, CP437_HIGH, DIACRITICS};

const RS: u8 = 0x1E;
const SS2: u8 = 0x19;
const UL_ON: u8 = 0x99;
const UL_OFF: u8 = 0x9A;
/// How the editor shows the ICON's underline-on code (0x99).
pub const UNDERLINE_ON: char = '\u{27E6}';
/// How the editor shows the ICON's underline-off code (0x9A).
pub const UNDERLINE_OFF: char = '\u{27E7}';

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Newline {
    /// RS (0x1E), as QNX 2 writes.
    Rs,
    Lf,
    CrLf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accents {
    /// `0x19` diacritic sequences (ICON mode); letters with no sequence fall back to code page 437.
    Telidon,
    /// Code page 437 bytes (QNX mode); letters not in it fall back to Telidon sequences.
    Cp437,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Format {
    pub newline: Newline,
    pub accents: Accents,
}

/// What a new or uploaded text file gets: the ICON's own conventions.
pub const ICON_DEFAULT: Format = Format { newline: Newline::Rs, accents: Accents::Telidon };

#[derive(Debug, Clone)]
pub struct Decoded {
    /// The text, with `\n` line ends.
    pub text: String,
    pub format: Format,
    /// Encoding `text` with `format` gives back exactly the original bytes.
    pub exact: bool,
}

/// Whether `data` is text: no NULs, and hardly any control bytes other than
/// line ends, tab, form feed, escape and the Telidon accent prefix.
pub fn is_text(data: &[u8]) -> bool {
    if data.contains(&0) {
        return false;
    }
    let odd = data
        .iter()
        .filter(|&&b| b < 0x20 && !matches!(b, RS | b'\n' | b'\r' | b'\t' | 0x0C | 0x1B | SS2 | 0x08))
        .count();
    odd * 100 <= data.len()
}

fn diacritic(code: u8) -> Option<(char, char)> {
    DIACRITICS.iter().find(|d| d.0 == code).map(|d| (d.1, d.2))
}

fn compose(base: u8, code: u8) -> Option<char> {
    COMPOSED.iter().find(|c| c.1 == base && c.2 == code).map(|c| c.0)
}

fn decompose(c: char) -> Option<(u8, u8)> {
    COMPOSED.binary_search_by_key(&c, |e| e.0).ok().map(|i| (COMPOSED[i].1, COMPOSED[i].2))
}

/// The code page 437 byte for `c`, except the two the ICON uses for underline.
fn cp437_byte(c: char) -> Option<u8> {
    CP437_HIGH
        .iter()
        .position(|&x| x == c)
        .map(|i| 0x80 + i as u8)
        .filter(|&b| b != UL_ON && b != UL_OFF)
}

/// Decode an ICON text file; `None` if it is binary.
pub fn decode(data: &[u8]) -> Option<Decoded> {
    if !is_text(data) {
        return None;
    }
    let crlf = data.windows(2).filter(|w| w == b"\r\n").count();
    let lf = data.iter().filter(|&&b| b == b'\n').count() - crlf;
    let rs = data.iter().filter(|&&b| b == RS).count();
    let newline = if crlf > rs && crlf >= lf {
        Newline::CrLf
    } else if lf > rs {
        Newline::Lf
    } else {
        Newline::Rs
    };
    let telidon = data.windows(2).any(|w| w[0] == SS2 && diacritic(w[1]).is_some());
    let cp437_letters = data.iter().any(|&b| (0x80..=0xA5).contains(&b) && b != UL_ON && b != UL_OFF);
    let accents = if cp437_letters && !telidon { Accents::Cp437 } else { Accents::Telidon };

    let mut text = String::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        match b {
            RS if newline == Newline::Rs => text.push('\n'),
            b'\n' if newline == Newline::Lf => text.push('\n'),
            b'\r' if newline == Newline::CrLf && data.get(i + 1) == Some(&b'\n') => {
                text.push('\n');
                i += 1;
            }
            SS2 if i + 2 < data.len() && diacritic(data[i + 1]).is_some() && data[i + 2] < 0x80 => {
                let (code, base) = (data[i + 1], data[i + 2]);
                let (mark, spacing) = diacritic(code).unwrap();
                if base == b' ' {
                    text.push(spacing);
                } else if let Some(c) = compose(base, code) {
                    text.push(c);
                } else {
                    text.push(base as char);
                    text.push(mark);
                }
                i += 2;
            }
            UL_ON => text.push(UNDERLINE_ON),
            UL_OFF => text.push(UNDERLINE_OFF),
            0x80..=0xFF => text.push(CP437_HIGH[(b - 0x80) as usize]),
            _ => text.push(b as char),
        }
        i += 1;
    }
    let format = Format { newline, accents };
    let exact = encode(&text, format).is_ok_and(|e| e == data);
    Some(Decoded { text, format, exact })
}

/// Encode text for the ICON. On failure, returns the characters it can't store.
pub fn encode(text: &str, format: Format) -> Result<Vec<u8>, Vec<char>> {
    let mut out = Vec::with_capacity(text.len());
    let mut bad: Vec<char> = Vec::new();
    let telidon_seq = |out: &mut Vec<u8>, base: u8, code: u8| out.extend_from_slice(&[SS2, code, base]);
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        // A letter followed by a combining mark the ICON knows: one sequence.
        if let (true, Some(code)) = (
            c.is_ascii() && c != '\n',
            chars.get(i + 1).and_then(|m| DIACRITICS.iter().find(|d| d.1 == *m)).map(|d| d.0),
        ) {
            telidon_seq(&mut out, c as u8, code);
            i += 2;
            continue;
        }
        match c {
            UNDERLINE_ON => out.push(UL_ON),
            UNDERLINE_OFF => out.push(UL_OFF),
            '\n' => match format.newline {
                Newline::Rs => out.push(RS),
                Newline::Lf => out.push(b'\n'),
                Newline::CrLf => out.extend_from_slice(b"\r\n"),
            },
            c if c.is_ascii() => out.push(c as u8),
            c => {
                let seq = decompose(c);
                let cp = cp437_byte(c);
                let spacing = DIACRITICS.iter().find(|d| d.2 == c).map(|d| d.0);
                match (format.accents, seq, cp, spacing) {
                    (_, _, _, Some(code)) => telidon_seq(&mut out, b' ', code),
                    (Accents::Telidon, Some((base, code)), _, _) => telidon_seq(&mut out, base, code),
                    (_, _, Some(byte), _) => out.push(byte),
                    (_, Some((base, code)), None, _) => telidon_seq(&mut out, base, code),
                    _ => {
                        if !bad.contains(&c) {
                            bad.push(c);
                        }
                    }
                }
            }
        }
        i += 1;
    }
    if bad.is_empty() {
        Ok(out)
    } else {
        Err(bad)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telidon_accents_and_rs_lines_round_trip() {
        // As the ICON editor writes French: "Enchanté, ça va à l'école?" plus a line.
        let raw = b"Enchant\x19\x42e, \x19\x4bca va \x19\x41a l'\x19\x42ecole?\x1eFin\x1e";
        let d = decode(raw).unwrap();
        assert_eq!(d.text, "Enchanté, ça va à l'école?\nFin\n");
        assert_eq!(d.format, Format { newline: Newline::Rs, accents: Accents::Telidon });
        assert!(d.exact);
        // Edit and save: new accented letters use the same style.
        let edited = d.text.replace("Fin", "Fête");
        assert_eq!(encode(&edited, d.format).unwrap(), b"Enchant\x19\x42e, \x19\x4bca va \x19\x41a l'\x19\x42ecole?\x1eF\x19\x43ete\x1e");
    }

    #[test]
    fn code_page_437_files_stay_code_page_437() {
        // Box drawing as in iconnet_logo, and QNX-mode letters.
        let raw = b"\xc9\xcd\xbb\x1e\xba \x94 \xba\x1e\x82t\x82\x1e";
        let d = decode(raw).unwrap();
        assert_eq!(d.text, "╔═╗\n║ ö ║\nété\n");
        assert_eq!(d.format.accents, Accents::Cp437);
        assert!(d.exact);
        assert_eq!(encode("ç", d.format).unwrap(), b"\x87");
        assert_eq!(encode("ç", ICON_DEFAULT).unwrap(), b"\x19\x4bc");
    }

    #[test]
    fn spacing_diacritics_and_odd_letters_are_kept_exactly() {
        // A circumflex on its own (19 43 20, seen in the system disk's editor config),
        // and an accent on a letter with no precomposed form.
        let raw = b"a\x19\x43 b \x19\x42x\x1e";
        let d = decode(raw).unwrap();
        assert_eq!(d.text, "a\u{02c6}b x\u{0301}\n");
        assert!(d.exact);
    }

    #[test]
    fn underline_codes_are_markers_not_letters() {
        // A Help page heading, as in /help/editor/notebook.text.
        let raw = b"HELP:  Editing a Notebook\x1e\x99Typing Text\x9a\x1e";
        let d = decode(raw).unwrap();
        assert_eq!(d.text, "HELP:  Editing a Notebook\n⟦Typing Text⟧\n");
        assert!(d.exact);
        assert_eq!(d.format.accents, Accents::Telidon);
        // Ö never becomes the underline code, in either style.
        assert_eq!(encode("Ö", ICON_DEFAULT).unwrap(), b"\x19\x48O");
        assert_eq!(encode("Ö", Format { accents: Accents::Cp437, ..ICON_DEFAULT }).unwrap(), b"\x19\x48O");
    }

    #[test]
    fn host_text_line_endings_are_recognised() {
        let d = decode(b"one\r\ntwo\r\n").unwrap();
        assert_eq!(d.format.newline, Newline::CrLf);
        assert!(d.exact);
        let d = decode(b"one\ntwo\n").unwrap();
        assert_eq!(d.format.newline, Newline::Lf);
        assert_eq!(encode(&d.text, ICON_DEFAULT).unwrap(), b"one\x1etwo\x1e");
    }

    #[test]
    fn mixed_files_are_flagged_not_silently_changed() {
        // RS lines with a stray LF inside: decoding keeps the LF as a character,
        // but saving would turn it into RS, so the round trip is not exact.
        let d = decode(b"a\nb\x1ec\x1e").unwrap();
        assert!(!d.exact);
    }

    #[test]
    fn unstorable_characters_are_named() {
        assert_eq!(encode("smile 😀 and ł", ICON_DEFAULT), Err(vec!['😀', 'ł']));
        assert!(decode(b"\x00\x01binary").is_none());
        assert_eq!(decode(b"").unwrap().format, ICON_DEFAULT);
    }

    /// With `ICON_TEXT_CORPUS` set to a folder of files copied off an ICON disk
    /// (e.g. `rb-cli tar IMG@1 / x.tar`, unpacked), every text file must decode
    /// and re-encode byte for byte.
    #[test]
    fn a_real_disk_round_trips() {
        let Ok(dir) = std::env::var("ICON_TEXT_CORPUS") else { return };
        let (mut text, mut bad) = (0, Vec::new());
        let mut stack = vec![std::path::PathBuf::from(dir)];
        while let Some(p) = stack.pop() {
            if p.is_dir() {
                stack.extend(std::fs::read_dir(&p).unwrap().map(|e| e.unwrap().path()));
            } else if let Some(d) = decode(&std::fs::read(&p).unwrap()) {
                text += 1;
                if !d.exact {
                    bad.push(p);
                }
            }
        }
        eprintln!("{text} text files, {} not exact: {bad:?}", bad.len());
        assert!(bad.is_empty());
    }
}
