//! Pages to PDF, and small previews.
//!
//! Two looks:
//! - **Clean**: Letter paper, the text in Courier (selectable and searchable),
//!   downloaded characters and bit images as crisp dots.
//! - **Authentic**: 9.5-inch continuous tractor-feed paper (sprocket holes,
//!   perforations, optionally green bar), every character printed as 9-pin
//!   dots with ribbon ink that varies along the ribbon, from pin to pin and
//!   from dot to dot, and the small offset of bidirectional printing.
//!
//! Ink is drawn as one image per page: an ink-coloured image with a soft
//! mask (the dots), cropped to where there is ink.

use crate::escp::{CharMark, Document, Page, Script};
use miniz_oxide::deflate::compress_to_vec_zlib;
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref, Str};

/// Raster resolution of the ink, dots per inch (one horizontal printer unit).
const DPI: f32 = 240.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    Clean,
    Authentic,
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub style: Style,
    /// Authentic only: green-bar paper rather than plain.
    pub green_bar: bool,
    /// Authentic only: a ribbon near the end of its life.
    pub worn_ribbon: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { style: Style::Authentic, green_bar: false, worn_ribbon: false }
    }
}

/// Sheet size and where the print line and top of form fall on it, in inches.
struct Sheet {
    w: f32,
    h: f32,
    left: f32,
    top: f32,
}

fn sheet(opt: &Options) -> Sheet {
    match opt.style {
        // 8.5 x 11 inch sheets between 0.5 inch tractor strips; the head's
        // first column 0.25 inch in, the first line 0.25 inch down.
        Style::Authentic => Sheet { w: 9.5, h: 11.0, left: 0.75, top: 0.25 },
        Style::Clean => Sheet { w: 8.5, h: 11.0, left: 0.25, top: 0.25 },
    }
}

/// A dot of ink on the sheet: pixel position at `DPI`, and strength 0-1.
#[derive(Clone, Copy)]
struct Ink {
    x: f32,
    y: f32,
    k: f32,
}

/// Each dot of a character, as (x, y, pin) in printer units.
fn char_dots(c: &CharMark, out: &mut Vec<(f32, f32, usize)>) {
    for (col, bits) in c.glyph.iter().enumerate() {
        for pin in 0..9 {
            if bits & (1 << pin) == 0 {
                continue;
            }
            let shear = if c.italic { (7.0 - pin as f32) * 0.7 } else { 0.0 };
            let x = c.x + col as f32 * c.col + shear;
            let y = c.y
                + match c.script {
                    Script::Normal => pin as f32 * 3.0,
                    Script::Super => pin as f32 * 1.5,
                    Script::Sub => 10.5 + pin as f32 * 1.5,
                };
            out.push((x, y, pin));
            if c.emphasized {
                out.push((x + c.col, y, pin));
            }
            if c.double_strike {
                out.push((x, y + 1.0, pin));
                if c.emphasized {
                    out.push((x + c.col, y + 1.0, pin));
                }
            }
        }
    }
    if c.underline {
        let mut x = c.x;
        while x < c.x + c.width - 0.1 {
            out.push((x, c.y + 24.0, 8));
            x += 2.0;
        }
    }
}

/// A repeatable pseudo-random number in [0, 1) from a few integers.
fn hash(a: u32, b: u32, c: u32) -> f32 {
    let mut h = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA77) ^ c.wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h >> 8) as f32 / (1u32 << 24) as f32
}

/// The ink dots of a page. `text` includes characters the PDF draws as text
/// (false for the clean look, which writes those in Courier).
fn ink_dots(page: &Page, opt: &Options, number: usize, text: bool) -> Vec<Ink> {
    let s = sheet(opt);
    let authentic = opt.style == Style::Authentic;
    let worn = opt.worn_ribbon;
    let (base, band, noise, jitter) = match (authentic, worn) {
        (false, _) => (1.0, 0.0, 0.0, 0.0),
        (true, false) => (1.0, 0.08, 0.12, 0.25),
        (true, true) => (0.8, 0.24, 0.28, 0.35),
    };
    // Some pins wear faster: their dots come out lighter.
    let pin_k = |pin: usize| -> f32 {
        if !authentic {
            return 1.0;
        }
        let weak: [f32; 9] = if worn {
            [0.72, 0.9, 0.95, 0.88, 0.97, 0.93, 0.9, 0.8, 0.7]
        } else {
            [0.93, 0.98, 1.0, 0.97, 1.0, 0.99, 0.98, 0.95, 0.92]
        };
        weak[pin.min(8)]
    };
    let mut raw = Vec::new();
    let mut pts: Vec<(f32, f32, usize, u32)> = Vec::new();
    for c in &page.chars {
        if !text && c.ch.is_some() {
            continue;
        }
        raw.clear();
        char_dots(c, &mut raw);
        pts.extend(raw.iter().map(|&(x, y, pin)| (x, y, pin, c.line)));
    }
    for d in &page.dots {
        pts.push((d.x, d.y, 4, d.line));
    }
    let n = number as u32;
    pts.into_iter()
        .map(|(x, y, pin, line)| {
            let (xi, yi) = ((x * 4.0) as u32, (y * 4.0) as u32);
            let mut px = (s.left * 240.0 + x) * DPI / 240.0;
            let mut py = s.top * DPI + y * DPI / 216.0;
            let mut k = base * pin_k(pin);
            if authentic {
                // The head prints alternate lines in opposite directions.
                if line % 2 == 1 {
                    px += 0.6;
                }
                px += (hash(xi, yi, n * 7 + 1) - 0.5) * 2.0 * jitter;
                py += (hash(xi, yi, n * 7 + 2) - 0.5) * 2.0 * jitter;
                // Ink varies along the ribbon, which moves as the head does.
                let phase = x / 240.0 / 3.3 * std::f32::consts::TAU + line as f32 * 0.61 + n as f32 * 1.3;
                k *= 1.0 - band * 0.5 * (1.0 + phase.sin());
                k *= 1.0 - noise * hash(xi, yi, n * 7 + 3);
            }
            Ink { x: px, y: py, k: k.clamp(0.0, 1.0) }
        })
        .collect()
}

/// Dot radius in pixels at `DPI`.
fn dot_radius(opt: &Options) -> f32 {
    match (opt.style, opt.worn_ribbon) {
        (Style::Clean, _) => 1.4,
        (Style::Authentic, false) => 2.0,
        (Style::Authentic, true) => 1.85,
    }
}

/// Add soft round dots to an 8-bit coverage raster of `size` (w, h), scaled
/// by `scale` and shifted by `origin` pixels.
fn splat(buf: &mut [u8], size: (usize, usize), dots: &[Ink], scale: f32, origin: (f32, f32), r: f32) {
    let ((w, h), (ox, oy)) = (size, origin);
    let r = (r * scale).max(0.55);
    for d in dots {
        let (cx, cy) = (d.x * scale - ox, d.y * scale - oy);
        let (x0, x1) = ((cx - r - 1.0).floor().max(0.0) as usize, (cx + r + 1.0).ceil().min(w as f32 - 1.0));
        let (y0, y1) = ((cy - r - 1.0).floor().max(0.0) as usize, (cy + r + 1.0).ceil().min(h as f32 - 1.0));
        if x1 < 0.0 || y1 < 0.0 {
            continue;
        }
        for py in y0..=y1 as usize {
            for px in x0..=x1 as usize {
                let dx = px as f32 + 0.5 - cx;
                let dy = py as f32 + 0.5 - cy;
                let cov = (r + 0.5 - (dx * dx + dy * dy).sqrt()).clamp(0.0, 1.0) * d.k;
                if cov <= 0.0 {
                    continue;
                }
                let a = &mut buf[py * w + px];
                *a = (*a as f32 + cov * (255.0 - *a as f32)).min(255.0) as u8;
            }
        }
    }
}

fn ink_rgb(opt: &Options) -> [u8; 3] {
    match (opt.style, opt.worn_ribbon) {
        (Style::Clean, _) => [0, 0, 0],
        (Style::Authentic, false) => [18, 18, 26],
        (Style::Authentic, true) => [64, 62, 84],
    }
}

/// Paper colour at a point of the sheet (inches), with sprocket holes.
fn paper_at(opt: &Options, x: f32, y: f32) -> [u8; 3] {
    if opt.style == Style::Clean {
        return [255, 255, 255];
    }
    for hx in [0.25, 9.25] {
        let dy = (y - 0.25) - ((y - 0.25) / 0.5).round() * 0.5;
        let d = ((x - hx).powi(2) + dy.powi(2)).sqrt();
        if d < 0.078 {
            return [196, 196, 190];
        }
        if d < 0.088 {
            return [168, 168, 162];
        }
    }
    let perforation = ((x - 0.5).abs() < 0.006 || (x - 9.0).abs() < 0.006) && ((y * 36.0) as u32).is_multiple_of(2);
    if perforation {
        return [200, 200, 195];
    }
    if opt.green_bar && (0.62..=8.88).contains(&x) && ((y / 0.5) as u32).is_multiple_of(2) {
        return [214, 234, 212];
    }
    [252, 251, 246]
}

fn compress(data: &[u8]) -> Vec<u8> {
    compress_to_vec_zlib(data, 6)
}

/// A circle as four Bézier curves.
fn circle(c: &mut Content, x: f32, y: f32, r: f32) {
    let k = 0.552_284_8 * r;
    c.move_to(x + r, y);
    c.cubic_to(x + r, y + k, x + k, y + r, x, y + r);
    c.cubic_to(x - k, y + r, x - r, y + k, x - r, y);
    c.cubic_to(x - r, y - k, x - k, y - r, x, y - r);
    c.cubic_to(x + k, y - r, x + r, y - k, x + r, y);
    c.close_path();
}

/// Draw tractor-feed paper: background, green bars, perforations, holes.
fn draw_paper(c: &mut Content, opt: &Options, s: &Sheet) {
    let (w, h) = (s.w * 72.0, s.h * 72.0);
    c.set_fill_rgb(0.988, 0.984, 0.965);
    c.rect(0.0, 0.0, w, h).fill_nonzero();
    if opt.green_bar {
        c.set_fill_rgb(0.84, 0.918, 0.83);
        let mut y = 0.0;
        while y < s.h {
            c.rect(0.62 * 72.0, h - (y + 0.5) * 72.0, (8.88 - 0.62) * 72.0, 0.5 * 72.0);
            y += 1.0;
        }
        c.fill_nonzero();
    }
    c.set_stroke_rgb(0.72, 0.72, 0.70);
    c.set_line_width(0.5);
    c.set_dash_pattern([1.5, 1.5], 0.0);
    for x in [0.5, 9.0] {
        c.move_to(x * 72.0, 0.0).line_to(x * 72.0, h);
    }
    // The tear lines between sheets.
    c.move_to(0.0, h - 0.4).line_to(w, h - 0.4);
    c.move_to(0.0, 0.4).line_to(w, 0.4);
    c.stroke();
    c.set_dash_pattern([], 0.0);
    c.set_fill_rgb(0.80, 0.80, 0.78);
    c.set_stroke_rgb(0.62, 0.62, 0.60);
    c.set_line_width(0.6);
    let mut y = 0.25;
    while y < s.h {
        for x in [0.25, 9.25] {
            circle(c, x * 72.0, h - y * 72.0, 0.078 * 72.0);
        }
        y += 0.5;
    }
    c.fill_nonzero_and_stroke();
}

/// Courier from the PDF base fonts, by emphasis and slant.
const FONTS: [&[u8]; 4] = [b"Courier", b"Courier-Bold", b"Courier-Oblique", b"Courier-BoldOblique"];
const FONT_NAMES: [&[u8]; 4] = [b"F1", b"F2", b"F3", b"F4"];

/// A character in WinAnsi, the base fonts' encoding.
fn win_ansi(ch: char) -> u8 {
    match ch as u32 {
        0x20..=0x7E | 0xA0..=0xFF => ch as u8,
        _ => match ch {
            '₧' => b'P',
            _ => b'?',
        },
    }
}

/// A letter with an accent printed over it, as one character.
fn compose(accent: char, letter: char) -> Option<char> {
    let (from, to) = match accent {
        '\'' | '´' => ("aeiouyAEIOUY", "áéíóúýÁÉÍÓÚÝ"),
        '`' => ("aeiouAEIOU", "àèìòùÀÈÌÒÙ"),
        '^' => ("aeiouAEIOU", "âêîôûÂÊÎÔÛ"),
        '¨' | '"' => ("aeiouyAEIOU", "äëïöüÿÄËÏÖÜ"),
        '¸' | ',' => ("cC", "çÇ"),
        '~' => ("anoANO", "ãñõÃÑÕ"),
        _ => return None,
    };
    from.chars().position(|c| c == letter).and_then(|i| to.chars().nth(i))
}

/// The characters to write as text: ones printed over each other in the
/// same cell (the ICON prints é as ', backspace, e) become one accented letter.
fn text_chars(page: &Page) -> Vec<(&CharMark, char)> {
    let mut out: Vec<(&CharMark, char)> = Vec::new();
    for m in page.chars.iter() {
        let Some(ch) = m.ch else { continue };
        let same = out.iter().rposition(|(o, _)| (o.x - m.x).abs() < 0.5 && (o.y - m.y).abs() < 0.5);
        if let Some(i) = same {
            let prev = out[i].1;
            if let Some(c) = compose(prev, ch).or_else(|| compose(ch, prev)) {
                let base = if compose(prev, ch).is_some() { m } else { out[i].0 };
                out[i] = (base, c);
                continue;
            }
            if ch == ' ' {
                continue;
            }
        }
        out.push((m, ch));
    }
    out
}

fn draw_text(c: &mut Content, page: &Page, s: &Sheet) {
    let h = s.h * 72.0;
    let mut font = usize::MAX;
    let mut size = 0.0;
    let mut scale = f32::NAN;
    let mut lines = Vec::new();
    c.begin_text();
    for (m, ch) in text_chars(page) {
        let f = (m.emphasized || m.double_strike) as usize + 2 * m.italic as usize;
        let (sz, base) = match m.script {
            Script::Normal => (12.0, 20.0),
            Script::Super => (7.5, 11.0),
            Script::Sub => (7.5, 22.0),
        };
        if f != font || sz != size {
            c.set_font(Name(FONT_NAMES[f]), sz);
            (font, size) = (f, sz);
        }
        // Courier advances 0.6 em; stretch it to the printer's character cell.
        let tz = m.width * 0.3 / (0.6 * sz) * 100.0;
        if tz != scale {
            c.set_horizontal_scaling(tz);
            scale = tz;
        }
        let x = s.left * 72.0 + m.x * 0.3;
        let y = h - (s.top * 72.0 + (m.y + base) / 3.0);
        c.set_text_matrix([1.0, 0.0, 0.0, 1.0, x, y]);
        c.show(Str(&[win_ansi(ch)]));
        if m.underline {
            lines.push((x, y - 2.0, m.width * 0.3));
        }
    }
    c.end_text();
    if !lines.is_empty() {
        c.set_stroke_gray(0.0);
        c.set_line_width(0.6);
        for (x, y, w) in lines {
            c.move_to(x, y).line_to(x + w, y);
        }
        c.stroke();
    }
}

/// The ink raster of a page, cropped to its dots: (x, y, w, h, coverage) in pixels.
fn ink_raster(dots: &[Ink], opt: &Options) -> Option<(usize, usize, usize, usize, Vec<u8>)> {
    if dots.is_empty() {
        return None;
    }
    let pad = 4.0;
    let s = sheet(opt);
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for d in dots {
        x0 = x0.min(d.x);
        y0 = y0.min(d.y);
        x1 = x1.max(d.x);
        y1 = y1.max(d.y);
    }
    let x0 = (x0 - pad).floor().max(0.0) as usize;
    let y0 = (y0 - pad).floor().max(0.0) as usize;
    let x1 = ((x1 + pad).ceil() as usize).min((s.w * DPI) as usize);
    let y1 = ((y1 + pad).ceil() as usize).min((s.h * DPI) as usize);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let (w, h) = (x1 - x0, y1 - y0);
    let mut buf = vec![0u8; w * h];
    splat(&mut buf, (w, h), dots, 1.0, (x0 as f32, y0 as f32), dot_radius(opt));
    Some((x0, y0, w, h, buf))
}

/// The document as a PDF.
pub fn pdf(doc: &Document, opt: &Options) -> Vec<u8> {
    let s = sheet(opt);
    let mut next = 1;
    let mut alloc = || {
        next += 1;
        Ref::new(next - 1)
    };
    let catalog = alloc();
    let tree = alloc();
    let info = alloc();
    let fonts: Vec<Ref> = FONTS.iter().map(|_| alloc()).collect();
    let mut pdf = Pdf::new();
    let pages: Vec<Page> = if doc.pages.is_empty() { vec![Page::default()] } else { doc.pages.clone() };
    let mut page_refs = Vec::new();
    let mut objects = Vec::new();
    for (i, page) in pages.iter().enumerate() {
        let page_ref = alloc();
        let content_ref = alloc();
        let ink = ink_raster(&ink_dots(page, opt, i, opt.style == Style::Authentic), opt);
        let images = ink.as_ref().map(|_| (alloc(), alloc()));
        page_refs.push(page_ref);
        objects.push((page_ref, content_ref, ink, images));
    }
    pdf.catalog(catalog).pages(tree);
    pdf.pages(tree).kids(page_refs.iter().copied()).count(page_refs.len() as i32);
    pdf.document_info(info).title(pdf_writer::TextStr("ICON printout")).producer(pdf_writer::TextStr("icon-emu"));
    for (f, r) in FONTS.iter().zip(&fonts) {
        pdf.type1_font(*r).base_font(Name(f)).encoding_predefined(Name(b"WinAnsiEncoding"));
    }
    for (i, (page_ref, content_ref, ink, images)) in objects.into_iter().enumerate() {
        let mut c = Content::new();
        if opt.style == Style::Authentic {
            draw_paper(&mut c, opt, &s);
        }
        if let (Some((x, y, w, h, _)), Some(_)) = (&ink, &images) {
            let k = 72.0 / DPI;
            c.save_state();
            c.transform([*w as f32 * k, 0.0, 0.0, *h as f32 * k, *x as f32 * k, (s.h * 72.0) - (*y + *h) as f32 * k]);
            c.x_object(Name(b"Ink"));
            c.restore_state();
        }
        if opt.style == Style::Clean {
            draw_text(&mut c, &pages[i], &s);
        }
        let content = compress(&c.finish());
        pdf.stream(content_ref, &content).filter(Filter::FlateDecode);
        let mut p = pdf.page(page_ref);
        p.parent(tree).media_box(Rect::new(0.0, 0.0, s.w * 72.0, s.h * 72.0)).contents(content_ref);
        let mut res = p.resources();
        if opt.style == Style::Clean {
            let mut f = res.fonts();
            for (n, r) in FONT_NAMES.iter().zip(&fonts) {
                f.pair(Name(n), *r);
            }
            f.finish();
        }
        if let Some((img, _)) = images {
            res.x_objects().pair(Name(b"Ink"), img);
        }
        res.finish();
        p.finish();
        if let (Some((_, _, w, h, buf)), Some((img, mask))) = (ink, images) {
            let rgb = ink_rgb(opt);
            let mut im = pdf.image_xobject(img, &rgb);
            im.width(1).height(1).bits_per_component(8).s_mask(mask);
            im.color_space().device_rgb();
            im.finish();
            let data = compress(&buf);
            let mut m = pdf.image_xobject(mask, &data);
            m.filter(Filter::FlateDecode);
            m.width(w as i32).height(h as i32).bits_per_component(8);
            m.color_space().device_gray();
            m.finish();
        }
    }
    pdf.finish()
}

/// A page drawn small, as RGBA: (width, height, pixels). Text in the clean
/// look is drawn with the printer's dots too.
pub fn preview(doc: &Document, page: usize, opt: &Options, width: u32) -> (u32, u32, Vec<u8>) {
    let s = sheet(opt);
    let w = width.max(16) as usize;
    let scale = w as f32 / (s.w * DPI);
    let h = (s.h * DPI * scale).round() as usize;
    let mut cov = vec![0u8; w * h];
    if let Some(p) = doc.pages.get(page) {
        let dots = ink_dots(p, opt, page, true);
        splat(&mut cov, (w, h), &dots, scale, (0.0, 0.0), dot_radius(opt) * 1.2);
    }
    let ink = ink_rgb(opt);
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let paper = paper_at(opt, (x as f32 + 0.5) / (DPI * scale), (y as f32 + 0.5) / (DPI * scale));
            let a = cov[y * w + x] as f32 / 255.0;
            let o = (y * w + x) * 4;
            for k in 0..3 {
                out[o + k] = (paper[k] as f32 * (1.0 - a) + ink[k] as f32 * a) as u8;
            }
            out[o + 3] = 255;
        }
    }
    (w as u32, h as u32, out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::escp::interpret;

    #[test]
    fn overstruck_accents_become_letters() {
        let doc = interpret(b"'\x08e`\x08a ^\x08o");
        let t: String = text_chars(&doc.pages[0]).iter().map(|x| x.1).collect();
        assert_eq!(t, "éà ô");
    }

    #[test]
    fn pdf_and_preview() {
        let doc = interpret(b"Hello\r\n\x1b*\x04\x02\x00\xff\xff\x0c");
        for style in [Style::Clean, Style::Authentic] {
            let opt = Options { style, green_bar: true, worn_ribbon: false };
            let out = pdf(&doc, &opt);
            assert!(out.starts_with(b"%PDF") && out.len() > 500);
            let (w, h, px) = preview(&doc, 0, &opt, 100);
            assert_eq!(px.len(), (w * h * 4) as usize);
        }
    }
}
