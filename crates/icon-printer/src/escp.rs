//! An Epson FX-80 (ESC/P, 9-pin) interpreter: turns the bytes a program sent
//! the printer into pages of characters and dots.
//!
//! Units: horizontal positions are in 1/240 inch from the left of the print
//! line, vertical ones in 1/216 inch from the top of the form. One column of
//! a character is 1/120 inch, one pin 1/72 inch.
//!
//! What the ICON sends (4.10.01 `print`, `/config/print/PAT.fx80`): it copies
//! the ROM characters to RAM (`ESC :`), switches to them (`ESC %`), downloads
//! its own characters with `ESC &`, then text with DC2/DC4/SI pitch changes,
//! `ESC E/F/G/H`, `ESC 0/2` line spacing and FF. Screen dumps (`print_screen`)
//! use `ESC A 8` and `ESC * 4` bit images.

use crate::font::GLCD;

/// Dots per character column (11 half-dot columns, pins in bits 0-8, bit 0 at the top).
pub type Glyph = [u16; 11];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Script {
    Normal,
    Super,
    Sub,
}

/// One printed character.
#[derive(Clone, Debug)]
pub struct CharMark {
    pub x: f32,
    pub y: f32,
    /// What it is, when it is one of the printer's own characters; `None`
    /// for a character the program downloaded (only its dots are known).
    pub ch: Option<char>,
    pub glyph: Glyph,
    /// Width of the character cell (1/240 inch).
    pub width: f32,
    /// Spacing of the glyph's half-dot columns (1/240 inch): 2 in pica.
    pub col: f32,
    pub emphasized: bool,
    pub double_strike: bool,
    pub italic: bool,
    pub underline: bool,
    pub script: Script,
    /// Print line number on the page, for the authentic look's bidirectional offset.
    pub line: u32,
}

/// One dot of a bit image.
#[derive(Clone, Copy, Debug)]
pub struct Dot {
    pub x: f32,
    pub y: f32,
    pub line: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Page {
    pub chars: Vec<CharMark>,
    pub dots: Vec<Dot>,
}

impl Page {
    pub fn is_empty(&self) -> bool {
        self.chars.is_empty() && self.dots.is_empty()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Document {
    pub pages: Vec<Page>,
}

/// A RAM (downloadable) character: its glyph and, if copied from ROM, what it is.
#[derive(Clone, Copy)]
struct RamChar {
    glyph: Glyph,
    ch: Option<char>,
    italic: bool,
}

/// Epson international character sets (ESC R n): the 12 codes they change.
const INTL_CODES: [u8; 12] = [0x23, 0x24, 0x40, 0x5B, 0x5C, 0x5D, 0x5E, 0x60, 0x7B, 0x7C, 0x7D, 0x7E];
const INTL: [[char; 12]; 9] = [
    ['#', '$', '@', '[', '\\', ']', '^', '`', '{', '|', '}', '~'], // USA
    ['#', '$', 'à', '°', 'ç', '§', '^', '`', 'é', 'ù', 'è', '¨'], // France
    ['#', '$', '§', 'Ä', 'Ö', 'Ü', '^', '`', 'ä', 'ö', 'ü', 'ß'], // Germany
    ['£', '$', '@', '[', '\\', ']', '^', '`', '{', '|', '}', '~'], // UK
    ['#', '$', '@', 'Æ', 'Ø', 'Å', '^', '`', 'æ', 'ø', 'å', '~'], // Denmark
    ['#', '¤', 'É', 'Ä', 'Ö', 'Å', 'Ü', 'é', 'ä', 'ö', 'å', 'ü'], // Sweden
    ['#', '$', '@', '°', '\\', 'é', '^', 'ù', 'à', 'ò', 'è', 'ì'], // Italy
    ['₧', '$', '@', '¡', 'Ñ', '¿', '^', '`', '¨', 'ñ', '}', '~'], // Spain
    ['#', '$', '@', '[', '¥', ']', '^', '`', '{', '|', '}', '~'], // Japan
];

/// What the characters the ICON downloads are (4.10.01 `print`, set up from
/// `/config/print/PAT.fx80`): lowered capitals that take an accent above,
/// accents printed over a letter after a backspace, and a few symbols. Only
/// used for the clean look's text; the dots always come from the download.
const ICON_DOWNLOADS: [(u8, char); 13] = [
    (0xC1, 'A'), (0xC5, 'E'), (0xC9, 'I'), (0xCF, 'O'), (0xD5, 'U'),
    (0x5E, '^'), (0xC0, '¸'), (0xDE, '¨'),
    (0xBC, '«'), (0xBE, '»'), (0xAD, '÷'), (0xAA, '×'), (0xC2, 'ß'),
];

/// Code page 437 position of a character, for the glyph table.
fn cp437(ch: char) -> Option<u8> {
    if (' '..='~').contains(&ch) {
        return Some(ch as u8);
    }
    Some(match ch {
        'Ç' => 0x80, 'ü' => 0x81, 'é' => 0x82, 'â' => 0x83, 'ä' => 0x84, 'à' => 0x85, 'å' => 0x86, 'ç' => 0x87,
        'ê' => 0x88, 'ë' => 0x89, 'è' => 0x8A, 'ï' => 0x8B, 'î' => 0x8C, 'ì' => 0x8D, 'Ä' => 0x8E, 'Å' => 0x8F,
        'É' => 0x90, 'æ' => 0x91, 'Æ' => 0x92, 'ô' => 0x93, 'ö' => 0x94, 'ò' => 0x95, 'û' => 0x96, 'ù' => 0x97,
        'Ö' => 0x99, 'Ü' => 0x9A, '£' => 0x9C, '¥' => 0x9D, '₧' => 0x9E, 'ñ' => 0xA4, 'Ñ' => 0xA5, '¿' => 0xA8,
        '¡' => 0xAD, 'ß' => 0xE1, '°' => 0xF8, '§' => 0x15, '¤' => 0x0F,
        _ => return None,
    })
}

/// The ROM glyph for a character: the 5 font columns on even half-dot columns.
fn rom_glyph(ch: char) -> Glyph {
    let mut g = [0u16; 11];
    let code = match ch {
        '¨' => return diaeresis(),
        'Ø' => return slashed('O'),
        'ø' => return slashed('o'),
        c => cp437(c).unwrap_or(b'?'),
    };
    for c in 0..5 {
        g[2 * c] = GLCD[code as usize * 5 + c] as u16;
    }
    g
}

fn diaeresis() -> Glyph {
    let mut g = [0u16; 11];
    g[2] = 0b11;
    g[6] = 0b11;
    g
}

fn slashed(base: char) -> Glyph {
    let mut g = rom_glyph(base);
    for (c, pin) in [(0usize, 6u16), (2, 5), (4, 3), (6, 2), (8, 1)] {
        g[c] |= 1 << pin;
    }
    g
}

/// The character and glyph the FX-80 ROM prints for `code` in set `intl`.
/// Codes 0xA0-0xFE are the italic versions of 0x20-0x7E.
fn rom_char(code: u8, intl: usize) -> (char, Glyph, bool) {
    let c = code & 0x7F;
    let ch = match INTL_CODES.iter().position(|&x| x == c) {
        Some(i) => INTL[intl.min(INTL.len() - 1)][i],
        None => c as char,
    };
    (ch, rom_glyph(ch), code >= 0x80)
}

const PICA: f32 = 24.0;
const ELITE: f32 = 20.0;
const CONDENSED: f32 = 14.0;
const LINE_SIXTH: f32 = 36.0;
const DEFAULT_PAGE: f32 = 66.0 * LINE_SIXTH;
const LINE_WIDTH: f32 = 8.0 * 240.0;

struct Printer {
    doc: Document,
    page: Page,
    x: f32,
    y: f32,
    line: u32,
    line_spacing: f32,
    page_len: f32,
    skip_perf: f32,
    left: f32,
    right: f32,
    elite: bool,
    condensed: bool,
    expanded: bool,
    expanded_line: bool,
    emphasized: bool,
    double_strike: bool,
    italic: bool,
    underline: bool,
    script: Script,
    intl: usize,
    msb: Option<bool>,
    use_ram: bool,
    ram: Vec<Option<RamChar>>,
    tabs: Vec<f32>,
    graphics_modes: [u8; 4],
    selected: bool,
}

impl Printer {
    fn new() -> Self {
        let mut p = Printer {
            doc: Document::default(),
            page: Page::default(),
            x: 0.0,
            y: 0.0,
            line: 0,
            line_spacing: LINE_SIXTH,
            page_len: DEFAULT_PAGE,
            skip_perf: 0.0,
            left: 0.0,
            right: LINE_WIDTH,
            elite: false,
            condensed: false,
            expanded: false,
            expanded_line: false,
            emphasized: false,
            double_strike: false,
            italic: false,
            underline: false,
            script: Script::Normal,
            intl: 0,
            msb: None,
            use_ram: false,
            ram: vec![None; 256],
            tabs: Vec::new(),
            graphics_modes: [0, 1, 2, 3],
            selected: true,
        };
        p.reset();
        p
    }

    /// ESC @: settings back to power-on, except the page and RAM characters.
    fn reset(&mut self) {
        self.line_spacing = LINE_SIXTH;
        self.page_len = DEFAULT_PAGE;
        self.skip_perf = 0.0;
        self.left = 0.0;
        self.right = LINE_WIDTH;
        self.elite = false;
        self.condensed = false;
        self.expanded = false;
        self.expanded_line = false;
        self.emphasized = false;
        self.double_strike = false;
        self.italic = false;
        self.underline = false;
        self.script = Script::Normal;
        self.msb = None;
        self.use_ram = false;
        self.graphics_modes = [0, 1, 2, 3];
        self.tabs = (1..=32).map(|i| (i * 8) as f32 * PICA).collect();
        self.x = self.left;
    }

    fn char_width(&self) -> f32 {
        let w = if self.condensed { CONDENSED } else if self.elite { ELITE } else { PICA };
        if self.expanded || self.expanded_line {
            w * 2.0
        } else {
            w
        }
    }

    fn carriage_return(&mut self) {
        self.x = self.left;
    }

    fn line_feed(&mut self, amount: f32) {
        self.y += amount;
        self.line += 1;
        self.expanded_line = false;
        if self.y >= self.page_len || (self.skip_perf > 0.0 && self.y > self.page_len - self.skip_perf) {
            let rest = if self.y >= self.page_len { self.y - self.page_len } else { 0.0 };
            self.new_page();
            self.y = rest;
        }
    }

    fn new_page(&mut self) {
        self.doc.pages.push(std::mem::take(&mut self.page));
        self.y = 0.0;
        self.line = 0;
    }

    fn form_feed(&mut self) {
        self.new_page();
        self.carriage_return();
        self.expanded_line = false;
    }

    fn print(&mut self, code: u8) {
        let ram = if self.use_ram { self.ram[code as usize] } else { None };
        let (ch, glyph, italic) = match ram {
            Some(r) => (r.ch, r.glyph, r.italic),
            None => {
                let (c, g, i) = rom_char(code, self.intl);
                (Some(c), g, i)
            }
        };
        let width = self.char_width();
        if self.x + width > self.right + 0.5 {
            self.carriage_return();
            self.line_feed(self.line_spacing);
        }
        let col = width / 12.0;
        self.page.chars.push(CharMark {
            x: self.x,
            y: self.y,
            ch,
            glyph,
            width,
            col,
            // The FX-80 can't print emphasized condensed characters.
            emphasized: self.emphasized && !self.condensed,
            double_strike: self.double_strike,
            italic: self.italic || italic,
            underline: self.underline,
            script: self.script,
            line: self.line,
        });
        self.x += width;
    }

    fn graphics(&mut self, mode: u8, data: &[u8]) {
        let step = match mode {
            0 => 4.0,
            1 | 2 => 2.0,
            3 => 1.0,
            4 => 3.0,
            5 => 240.0 / 72.0,
            6 => 240.0 / 90.0,
            _ => 4.0,
        };
        for &b in data {
            // A full line prints and feeds by itself (the KX-P1091 setup
            // sends 640-column bands with no CR LF between them).
            if self.x + step > self.right + 0.5 {
                self.carriage_return();
                self.line_feed(self.line_spacing);
            }
            for pin in 0..8 {
                if b & (0x80 >> pin) != 0 {
                    self.page.dots.push(Dot { x: self.x, y: self.y + pin as f32 * 3.0, line: self.line });
                }
            }
            self.x += step;
        }
    }

    fn horizontal_tab(&mut self) {
        let w = self.char_width();
        let col = ((self.x - self.left) / w).floor() + 1.0;
        let stop = self.tabs.iter().map(|t| t / PICA).find(|&t| t >= col);
        if let Some(t) = stop {
            self.x = self.left + t * w;
        }
    }

    fn finish(mut self) -> Document {
        self.doc.pages.push(std::mem::take(&mut self.page));
        while self.doc.pages.last().is_some_and(Page::is_empty) {
            self.doc.pages.pop();
        }
        self.doc
    }
}

/// Interpret a print job.
pub fn interpret(data: &[u8]) -> Document {
    let mut p = Printer::new();
    let mut i = 0;
    while i < data.len() {
        let mut b = data[i];
        i += 1;
        if !p.selected {
            if b == 0x11 {
                p.selected = true;
            }
            continue;
        }
        if let Some(set) = p.msb {
            if b >= 0x20 && b != 0x7F && (b & 0x7F) >= 0x20 {
                b = if set { b | 0x80 } else { b & 0x7F };
            }
        }
        // 0x80-0x9F are control codes too, like 0x00-0x1F.
        let ctrl = b & 0x7F;
        if ctrl < 0x20 || b == 0x7F {
            match ctrl {
                0x08 => p.x = (p.x - p.char_width()).max(p.left),
                0x09 => p.horizontal_tab(),
                0x0A | 0x0B => p.line_feed(p.line_spacing),
                0x0C => p.form_feed(),
                0x0D => p.carriage_return(),
                0x0E => p.expanded_line = true,
                0x0F => p.condensed = true,
                0x12 => p.condensed = false,
                0x13 => p.selected = false,
                0x14 => p.expanded_line = false,
                0x18 => p.carriage_return(),
                0x1B => i = escape(&mut p, data, i),
                _ => {}
            }
            continue;
        }
        p.print(b);
    }
    p.finish()
}

/// Run the ESC sequence whose command byte is at `i`; returns the next index.
fn escape(p: &mut Printer, data: &[u8], mut i: usize) -> usize {
    let at = |k: usize| data.get(k).copied().unwrap_or(0);
    let cmd = at(i);
    i += 1;
    let n = at(i);
    match cmd {
        b'@' => p.reset(),
        b'E' => p.emphasized = true,
        b'F' => p.emphasized = false,
        b'G' => p.double_strike = true,
        b'H' => p.double_strike = false,
        b'4' => p.italic = true,
        b'5' => p.italic = false,
        b'M' => p.elite = true,
        b'P' => p.elite = false,
        b'T' => p.script = Script::Normal,
        b'O' => p.skip_perf = 0.0,
        b'#' => p.msb = None,
        b'=' => p.msb = Some(false),
        b'>' => p.msb = Some(true),
        b'0' => p.line_spacing = 27.0,
        b'1' => p.line_spacing = 21.0,
        b'2' => p.line_spacing = LINE_SIXTH,
        b'-' => {
            p.underline = n & 1 != 0;
            i += 1;
        }
        b'W' => {
            p.expanded = n & 1 != 0;
            i += 1;
        }
        b'S' => {
            p.script = if n & 1 == 0 { Script::Super } else { Script::Sub };
            i += 1;
        }
        b'3' => {
            p.line_spacing = n as f32;
            i += 1;
        }
        b'A' => {
            p.line_spacing = n as f32 * 3.0;
            i += 1;
        }
        b'J' => {
            p.y += n as f32;
            if p.y >= p.page_len {
                let rest = p.y - p.page_len;
                p.new_page();
                p.y = rest;
            }
            i += 1;
        }
        b'j' => {
            p.y = (p.y - n as f32).max(0.0);
            i += 1;
        }
        b'C' => {
            if n == 0 {
                p.page_len = at(i + 1) as f32 * 216.0;
                i += 2;
            } else {
                p.page_len = n as f32 * p.line_spacing;
                i += 1;
            }
            p.page_len = p.page_len.max(p.line_spacing);
        }
        b'N' => {
            p.skip_perf = n as f32 * p.line_spacing;
            i += 1;
        }
        b'l' => {
            p.left = n as f32 * p.char_width();
            i += 1;
        }
        b'Q' => {
            p.right = (n as f32 * p.char_width()).min(LINE_WIDTH * 1.72);
            i += 1;
        }
        b'R' => {
            p.intl = n as usize;
            i += 1;
        }
        b'!' => {
            p.elite = n & 0x01 != 0;
            p.condensed = n & 0x04 != 0;
            p.emphasized = n & 0x08 != 0;
            p.double_strike = n & 0x10 != 0;
            p.expanded = n & 0x20 != 0;
            p.italic = n & 0x40 != 0;
            p.underline = n & 0x80 != 0;
            i += 1;
        }
        b'K' | b'L' | b'Y' | b'Z' => {
            let mode = p.graphics_modes[b"KLYZ".iter().position(|&c| c == cmd).unwrap()];
            let len = n as usize | (at(i + 1) as usize) << 8;
            let start = (i + 2).min(data.len());
            let end = (start + len).min(data.len());
            p.graphics(mode, &data[start..end]);
            i = end;
        }
        b'*' => {
            let len = at(i + 1) as usize | (at(i + 2) as usize) << 8;
            let start = (i + 3).min(data.len());
            let end = (start + len).min(data.len());
            p.graphics(n, &data[start..end]);
            i = end;
        }
        b'^' => {
            // 9-pin graphics: two bytes per column; print the top 8 pins.
            let len = at(i + 1) as usize | (at(i + 2) as usize) << 8;
            let start = (i + 3).min(data.len());
            let end = (start + 2 * len).min(data.len());
            let cols: Vec<u8> = data[start..end].chunks(2).map(|c| c[0]).collect();
            p.graphics(n, &cols);
            i = end;
        }
        b'?' => {
            if let Some(k) = b"KLYZ".iter().position(|&c| c == n) {
                p.graphics_modes[k] = at(i + 1);
            }
            i += 2;
        }
        b':' => {
            // Copy the ROM characters to RAM.
            for code in 0..256usize {
                let c = code as u8;
                if c & 0x7F >= 0x20 && c & 0x7F != 0x7F {
                    let (ch, glyph, italic) = rom_char(c, p.intl);
                    p.ram[code] = Some(RamChar { glyph, ch: Some(ch), italic });
                }
            }
            i += 3;
        }
        b'%' => {
            p.use_ram = n & 1 != 0;
            i += 2;
        }
        b'&' => {
            // ESC & 0 first last, then per character: attribute + 11 columns.
            let (first, last) = (at(i + 1), at(i + 2));
            i += 3;
            for code in first..=last.max(first) {
                if i + 12 > data.len() {
                    i = data.len();
                    break;
                }
                let a = data[i];
                let mut glyph = [0u16; 11];
                for (c, g) in glyph.iter_mut().enumerate() {
                    let col = data[i + 1 + c];
                    // MSB is the top pin; attribute bit 7 clear = use the lower 8 pins.
                    let shift = if a & 0x80 != 0 { 0 } else { 1 };
                    for pin in 0..8 {
                        if col & (0x80 >> pin) != 0 {
                            *g |= 1 << (pin + shift);
                        }
                    }
                }
                let ch = ICON_DOWNLOADS.iter().find(|d| d.0 == code).map(|d| d.1);
                p.ram[code as usize] = Some(RamChar { glyph, ch, italic: false });
                i += 12;
                if code == 255 {
                    break;
                }
            }
        }
        b'D' | b'B' => {
            let mut stops = Vec::new();
            while i < data.len() && data[i] != 0 {
                stops.push(data[i] as f32 * PICA);
                i += 1;
            }
            i += 1;
            if cmd == b'D' {
                p.tabs = stops;
            }
        }
        b'b' => {
            i += 1;
            while i < data.len() && data[i] != 0 {
                i += 1;
            }
            i += 1;
        }
        // One-byte arguments this printer accepts and ignores.
        b'U' | b's' | b'i' | b'I' | b'x' | b'k' | b'p' | b'/' | b'a' | b'q' | b'w' | b'r' | b't' => i += 1,
        b'e' | b'f' | b'\\' | b'$' => i += 2,
        _ => {}
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_lines_and_form_feed() {
        let doc = interpret(b"AB\r\nC\x0cD");
        assert_eq!(doc.pages.len(), 2);
        let p = &doc.pages[0];
        assert_eq!(p.chars.len(), 3);
        assert_eq!((p.chars[1].x, p.chars[1].y), (24.0, 0.0));
        assert_eq!((p.chars[2].x, p.chars[2].y), (0.0, 36.0));
        assert_eq!(p.chars[0].ch, Some('A'));
    }

    #[test]
    fn pitch_and_styles() {
        let doc = interpret(b"\x0fA\x12\x1bE\x1bGB\x1b4C\x1b!\x20D");
        let c = &doc.pages[0].chars;
        assert_eq!(c[0].width, CONDENSED);
        assert!(c[1].emphasized && c[1].double_strike);
        assert!(c[2].italic);
        assert_eq!(c[3].width, 48.0);
    }

    #[test]
    fn downloaded_characters() {
        // ESC : 0 0 0, ESC % 1 0, ESC & 0 C1 C1 + 8B + 11 columns, then C1.
        let mut job = vec![0x1B, b':', 0, 0, 0, 0x1B, b'%', 1, 0, 0x1B, b'&', 0, 0xC1, 0xC1, 0x8B];
        job.extend([0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x01]);
        job.extend([0xC1, b'A']);
        let c = &interpret(&job).pages[0].chars;
        assert_eq!(c[0].ch, Some('A'));
        assert_eq!(c[0].glyph[0], 1);
        assert_eq!(c[0].glyph[10], 1 << 7);
        assert_eq!(c[1].ch, Some('A'));
    }

    #[test]
    fn bit_image() {
        let doc = interpret(&[0x1B, b'*', 4, 2, 0, 0x80, 0x01]);
        let d = &doc.pages[0].dots;
        assert_eq!(d.len(), 2);
        assert_eq!((d[1].x, d[1].y), (3.0, 21.0));
    }

    #[test]
    fn full_graphics_lines_wrap() {
        // ESC 1 (7/72 inch), then three 640-column bands at 80 dpi.
        let mut job = vec![0x1B, b'1'];
        for _ in 0..3 {
            job.extend([0x1B, b'*', 4, 0x80, 0x02]);
            job.extend(std::iter::repeat_n(0x80, 640));
        }
        let d = &interpret(&job).pages[0].dots;
        assert_eq!(d.len(), 1920);
        assert_eq!((d[640].x, d[640].y), (0.0, 21.0));
        assert_eq!(d[1280].y, 42.0);
    }

    #[test]
    fn international_and_overflow() {
        let doc = interpret(b"\x1bR\x01{@");
        let c = &doc.pages[0].chars;
        assert_eq!(c[0].ch, Some('é'));
        assert_eq!(c[1].ch, Some('à'));
        let long = vec![b'x'; 81];
        let c = &interpret(&long).pages[0].chars;
        assert_eq!((c[80].x, c[80].y), (0.0, 36.0));
    }

    #[test]
    fn page_length_wraps() {
        let mut job = Vec::new();
        for _ in 0..70 {
            job.extend(b"x\r\n");
        }
        assert_eq!(interpret(&job).pages.len(), 2);
        assert!(interpret(b"\x0c\x0c").pages.is_empty());
    }
}
