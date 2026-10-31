//! A dot-matrix printer for icon-emu: the Epson FX-80 the ICON's `print`
//! drives on the parallel port, turning a print job into PDF pages.
//!
//! ```
//! let job = b"Hello from the ICON\r\n\x0c";
//! let doc = icon_printer::interpret(job);
//! let pdf = icon_printer::pdf(&doc, &icon_printer::Options::default());
//! assert!(pdf.starts_with(b"%PDF"));
//! ```

pub mod escp;
mod font;
mod render;

pub use escp::{interpret, Document, Page};
pub use render::{pdf, preview, Options, Style};

impl Options {
    /// Options from their names: `clean` or `authentic`, plus `green-bar`
    /// and `worn` for the authentic look.
    pub fn parse(style: &str, green_bar: bool, worn: bool) -> Result<Options, String> {
        let style = match style {
            "clean" => Style::Clean,
            "authentic" => Style::Authentic,
            s => return Err(format!("unknown printout style {s:?} (clean or authentic)")),
        };
        Ok(Options { style, green_bar, worn_ribbon: worn })
    }
}
