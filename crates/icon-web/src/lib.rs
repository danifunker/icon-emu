//! WebAssembly front end: one emulated machine per `Emu` instance.

use icon_core::devices::nvram::presets;
use icon_core::devices::wd1002::Disk;
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
    pub fn new(rom: &[u8], preset: &str, host_time: f64) -> Result<Emu, JsError> {
        if rom.len() != 0x8000 {
            return Err(JsError::new("expected the 32 KB interleaved ICON ROM"));
        }
        let nvram = match preset {
            "fs" => presets::fileserver(),
            _ => presets::workstation(),
        };
        Ok(Emu {
            m: Machine::new(Config { rom: rom.to_vec(), patch_800k_floppy: false, ram_kb: 512, nvram, straps: STRAPS_NORMAL, host_time: host_time as i64 }),
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

    /// Insert a floppy image into the fileserver's drive (drive 2:).
    pub fn insert_floppy(&mut self, data: &[u8]) -> Result<(), JsError> {
        let disk = Disk::floppy_from_image(data.to_vec())
            .ok_or_else(|| JsError::new("unrecognised floppy image size"))?;
        self.m.board.insert_floppy(Some(disk));
        Ok(())
    }

    pub fn eject_floppy(&mut self) {
        self.m.board.insert_floppy(None);
    }

    /// Queue a raw ICON key code (key number; bit 7 set = release).
    pub fn key(&mut self, code: u8) {
        self.m.board.key(code);
    }

    /// Beeper/trouble-LED output (FA88 bit 0).
    pub fn led(&self) -> bool {
        self.m.board.fa88 & 1 != 0
    }

    /// Speaker audio produced since the last call, as mono f32 samples.
    pub fn audio(&mut self, rate: u32) -> Vec<f32> {
        self.m.board.take_audio(rate)
    }

    /// Hard-disk / floppy activity, for the front-panel LEDs.
    pub fn disk_activity(&self) -> u8 {
        let a = &self.m.board.wd.activity;
        (a[0] > 0) as u8 | (((a[3] > 0) as u8) << 1)
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
