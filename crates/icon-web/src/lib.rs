//! WebAssembly front end: an ICONET segment with a LEXICON fileserver
//! (slot 0) and an ICON workstation (slot 1).

use icon_core::devices::nvram::presets;
use icon_core::devices::wd1002::Disk;
use icon_core::lan::Lan;
use icon_core::machine::{Config, Machine, CPU_HZ, STRAPS_NORMAL};
use wasm_bindgen::prelude::*;

pub const FILESERVER: usize = 0;
pub const WORKSTATION: usize = 1;

/// A blank 10 MB (306 x 4 x 17) hard disk image.
#[wasm_bindgen]
pub fn blank_hard_disk_10mb() -> Vec<u8> {
    Disk::blank_hard(306, 4, 17).data
}

#[wasm_bindgen]
pub struct Network {
    lan: Lan,
    rom: Vec<u8>,
}

#[wasm_bindgen]
impl Network {
    #[wasm_bindgen(constructor)]
    pub fn new(rom: &[u8]) -> Result<Network, JsError> {
        if rom.len() != 0x8000 {
            return Err(JsError::new("expected the 32 KB interleaved ICON ROM"));
        }
        Ok(Network { lan: Lan::new(2), rom: rom.to_vec() })
    }

    /// Power a machine on. `host_time` is local wall-clock seconds (for the
    /// fileserver's battery clock); disks are attached separately.
    pub fn power_on(&mut self, slot: usize, host_time: f64) {
        let fs = slot == FILESERVER;
        let m = Machine::new(Config {
            rom: self.rom.clone(),
            patch_800k_floppy: false,
            ram_kb: 512,
            nvram: if fs { presets::fileserver() } else { presets::workstation() },
            straps: STRAPS_NORMAL,
            host_time: host_time as i64,
            disk_controller: fs,
        });
        self.lan.power_on(slot, m);
    }

    pub fn power_off(&mut self, slot: usize) {
        self.lan.power_off(slot);
    }

    pub fn is_on(&self, slot: usize) -> bool {
        self.lan.nodes[slot].is_some()
    }

    /// Run every powered machine for one 60 Hz frame.
    pub fn run_frame(&mut self) {
        self.lan.run((CPU_HZ / 60) as u64);
    }

    fn m(&self, slot: usize) -> Option<&Machine> {
        self.lan.nodes.get(slot)?.as_ref()
    }
    fn m_mut(&mut self, slot: usize) -> Option<&mut Machine> {
        self.lan.nodes.get_mut(slot)?.as_mut()
    }

    /// The display as 640x240 RGBA (empty when off).
    pub fn screen(&self, slot: usize) -> Vec<u8> {
        self.m(slot).map(|m| m.board.render_rgba()).unwrap_or_default()
    }

    /// Queue a raw ICON key code (key number; bit 7 set = release).
    pub fn key(&mut self, slot: usize, code: u8) {
        if let Some(m) = self.m_mut(slot) {
            m.board.key(code);
        }
    }

    /// Trackball motion in mouse counts (+x right, +y down). The ICON2's
    /// ICON emulator halves host counts; the X chip is select 01, Y is 00.
    pub fn trackball(&mut self, slot: usize, dx: i32, dy: i32) {
        if let Some(m) = self.m_mut(slot) {
            m.board.counters[1].motion(dx / 2);
            m.board.counters[0].motion(dy / 2);
        }
    }

    /// Speaker audio from all machines, mixed, since the last call.
    pub fn audio(&mut self, rate: u32) -> Vec<f32> {
        let mut mix: Vec<f32> = Vec::new();
        for m in self.lan.nodes.iter_mut().flatten() {
            let a = m.board.take_audio(rate);
            if mix.len() < a.len() {
                mix.resize(a.len(), 0.0);
            }
            for (o, x) in mix.iter_mut().zip(a) {
                *o += x;
            }
        }
        mix
    }

    /// Front-panel lights: bit 0 trouble LED, bit 1 hard disk, bit 2 floppy,
    /// bit 3 receiver armed (network ready).
    pub fn lights(&self, slot: usize) -> u8 {
        let Some(m) = self.m(slot) else { return 0 };
        let a = &m.board.wd.activity;
        (m.board.fa88 & 1)
            | ((a[0] > 0) as u8) << 1
            | ((a[3] > 0) as u8) << 2
            | ((m.board.arcnet.status & 0x80 == 0) as u8) << 3
    }

    pub fn status(&self, slot: usize) -> String {
        let Some(m) = self.m(slot) else { return "off".into() };
        let r = m.cpu.regs();
        format!("node {} · {:04X}:{:04X}", m.board.arcnet.node_id, r.cs, r.ip)
    }

    pub fn packets(&self) -> f64 {
        self.lan.packets as f64
    }

    /// Insert a floppy image into the fileserver's drive.
    pub fn insert_floppy(&mut self, data: &[u8]) -> Result<(), JsError> {
        let disk = Disk::floppy_from_image(data.to_vec())
            .ok_or_else(|| JsError::new("unrecognised floppy image size"))?;
        if let Some(m) = self.m_mut(FILESERVER) {
            m.board.insert_floppy(Some(disk));
        }
        Ok(())
    }

    pub fn eject_floppy(&mut self) {
        if let Some(m) = self.m_mut(FILESERVER) {
            m.board.insert_floppy(None);
        }
    }

    /// Attach the fileserver's hard disk. Geometry comes from a geometry
    /// record in block 1 when present, otherwise 4 heads x 17 sectors.
    pub fn attach_hard_disk(&mut self, data: &[u8]) {
        let (heads, spt) = if data.len() >= 512 && data[0x10] == 2 && data[0x2F] == 3 {
            (data[0x13].max(1), u16::from_le_bytes([data[0x1C], data[0x1D]]).max(1) as u8)
        } else {
            (4, 17)
        };
        if let Some(m) = self.m_mut(FILESERVER) {
            m.board.attach_hard_disk(Some(Disk::hard_from_image(data.to_vec(), heads, spt)));
        }
    }

    /// Current hard disk contents (empty if none).
    pub fn hard_disk_image(&self) -> Vec<u8> {
        self.m(FILESERVER)
            .and_then(|m| m.board.wd.drives[0].as_ref())
            .map(|d| d.data.clone())
            .unwrap_or_default()
    }

    /// True if the hard disk changed since the last call (then clears the flag).
    pub fn take_hard_disk_dirty(&mut self) -> bool {
        match self.m_mut(FILESERVER).and_then(|m| m.board.wd.drives[0].as_mut()) {
            Some(d) if d.dirty => {
                d.dirty = false;
                true
            }
            _ => false,
        }
    }
}
