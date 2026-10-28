//! WebAssembly front end: one emulated machine per `Emu` instance.

use icon_core::devices::nvram::presets;
use icon_core::machine::{Config, Machine, CPU_HZ, STRAPS_NORMAL};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Emu {
    m: Machine,
}

#[wasm_bindgen]
impl Emu {
    /// Power on a machine. `preset` is "ws" (workstation) or "fs" (fileserver).
    #[wasm_bindgen(constructor)]
    pub fn new(rom: &[u8], preset: &str) -> Result<Emu, JsError> {
        if rom.len() != 0x8000 {
            return Err(JsError::new("expected the 32 KB interleaved ICON ROM"));
        }
        let nvram = match preset {
            "fs" => presets::fileserver(),
            _ => presets::workstation(),
        };
        Ok(Emu {
            m: Machine::new(Config { rom: rom.to_vec(), ram_kb: 512, nvram, straps: STRAPS_NORMAL }),
        })
    }

    /// Run one 60 Hz video frame of CPU time.
    pub fn run_frame(&mut self) {
        self.m.run((CPU_HZ / 60) as u64);
    }

    /// The display as 640x240 RGBA.
    pub fn screen(&self) -> Vec<u8> {
        self.m.board.render_rgba()
    }

    /// Queue a raw ICON key code (key number; bit 7 set = release).
    pub fn key(&mut self, code: u8) {
        self.m.board.key(code);
    }

    /// Beeper/trouble-LED output (FA88 bit 0).
    pub fn led(&self) -> bool {
        self.m.board.fa88 & 1 != 0
    }

    /// One-line status for the debug readout.
    pub fn status(&self) -> String {
        let r = self.m.cpu.regs();
        format!(
            "{:04X}:{:04X}  {:.2} s  beeper edges {}",
            r.cs,
            r.ip,
            self.m.board.cycles as f64 / CPU_HZ as f64,
            self.m.board.fa88_edges
        )
    }
}
