//! WebAssembly front end: an ICONET segment with a LEXICON fileserver
//! (slot 0) and one or more ICON workstations (slots 1.., node 15 + slot).
//!
//! Each slot has a machine kind, and each kind its own boot ROM, so a
//! fileserver ROM and a workstation ROM can differ (later: ICON2, ICON3).

use icon_core::devices::nvram::presets;
use icon_core::devices::wd1002::Disk;
use icon_core::lan::Lan;
use icon_core::machine::{Config, Machine, CPU_HZ, STRAPS_NORMAL};
use wasm_bindgen::prelude::*;

pub const FILESERVER: usize = 0;

/// A blank 10 MB (306 x 4 x 17) hard disk image.
#[wasm_bindgen]
pub fn blank_hard_disk_10mb() -> Vec<u8> {
    let (c, h, s) = icon_core::devices::wd1002::HD_10MB;
    Disk::blank_hard(c, h, s).data
}

/// A blank 64 MB-class (1024 x 8 x 17, Micropolis 1335) hard disk image.
#[wasm_bindgen]
pub fn blank_hard_disk_64mb() -> Vec<u8> {
    let (c, h, s) = icon_core::devices::wd1002::HD_64MB;
    Disk::blank_hard(c, h, s).data
}

fn check_rom(rom: &[u8]) -> Result<(), JsError> {
    if rom.len() != 0x8000 {
        return Err(JsError::new("expected a 32 KB ROM image (the two 16 KB EPROMs interleaved, even bytes first)"));
    }
    Ok(())
}

/// Machine kinds a slot can hold. Only the original ICON hardware is emulated so far.
pub const LEXICON: &str = "lexicon";
pub const ICON: &str = "icon";
const NODE_KINDS: [&str; 1] = [ICON];

#[wasm_bindgen]
pub struct Network {
    lan: Lan,
    /// Kind of machine in each slot.
    kinds: Vec<String>,
    /// Boot ROM per machine kind.
    roms: std::collections::HashMap<String, Vec<u8>>,
}

#[wasm_bindgen]
impl Network {
    /// A segment with the fileserver and one ICON workstation, and no ROMs yet.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Network {
        Network { lan: Lan::new(2), kinds: vec![LEXICON.into(), ICON.into()], roms: Default::default() }
    }

    /// Set the boot ROM for one machine kind ("lexicon", "icon"). Machines
    /// pick it up at their next power-on.
    pub fn set_rom(&mut self, kind: &str, rom: &[u8]) -> Result<(), JsError> {
        check_rom(rom)?;
        self.roms.insert(kind.to_string(), rom.to_vec());
        Ok(())
    }

    pub fn has_rom(&self, kind: &str) -> bool {
        self.roms.contains_key(kind)
    }

    /// Number of slots (fileserver plus workstations).
    pub fn slots(&self) -> usize {
        self.lan.nodes.len()
    }

    /// The machine kind in a slot.
    pub fn kind(&self, slot: usize) -> String {
        self.kinds.get(slot).cloned().unwrap_or_default()
    }

    /// Add a powered-off node of the given kind; returns its slot number.
    pub fn add_node(&mut self, kind: &str) -> Result<usize, JsError> {
        if !NODE_KINDS.contains(&kind) {
            return Err(JsError::new(&format!("{kind} nodes are not emulated yet")));
        }
        self.lan.nodes.push(None);
        self.lan.grow();
        self.kinds.push(kind.to_string());
        Ok(self.lan.nodes.len() - 1)
    }

    /// Remove the last node if it is off and not the only workstation.
    pub fn remove_last_node(&mut self) -> bool {
        if self.lan.nodes.len() <= 2 || self.lan.nodes.last().is_some_and(|n| n.is_some()) {
            return false;
        }
        self.lan.nodes.pop();
        self.lan.grow();
        self.kinds.pop();
        true
    }

    /// Packets this node has sent or received so far (for its coax light).
    pub fn activity(&self, slot: usize) -> f64 {
        self.lan.activity.get(slot).copied().unwrap_or(0) as f64
    }

    /// Power a machine on. `host_time` is local wall-clock seconds (for the
    /// fileserver's battery clock); disks are attached separately. Fails if
    /// there is no ROM for the slot's kind.
    pub fn power_on(&mut self, slot: usize, host_time: f64) -> Result<(), JsError> {
        let Some(kind) = self.kinds.get(slot) else { return Err(JsError::new("no such node")) };
        let rom = self.roms.get(kind).ok_or_else(|| JsError::new(&format!("no {kind} ROM loaded")))?.clone();
        let fs = slot == FILESERVER;
        let m = Machine::new(Config {
            rom,
            patch_800k_floppy: false,
            ram_kb: 512,
            nvram: if fs {
                presets::fileserver()
            } else {
                let mut nv = presets::workstation();
                nv[10] = (((15 + slot) as u16) << 8) | 1; // own node ID, boot server 1
                nv
            },
            straps: STRAPS_NORMAL,
            host_time: host_time as i64,
            disk_controller: fs,
        });
        self.lan.power_on(slot, m);
        Ok(())
    }

    pub fn power_off(&mut self, slot: usize) {
        self.lan.power_off(slot);
    }

    pub fn is_on(&self, slot: usize) -> bool {
        self.lan.nodes.get(slot).is_some_and(|n| n.is_some())
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

    /// Type ASCII text on a machine's keyboard (`\r` = Enter).
    pub fn type_text(&mut self, slot: usize, text: &str) {
        if let Some(m) = self.m_mut(slot) {
            for k in icon_core::devices::keyboard::type_text(text) {
                m.board.key(k);
            }
        }
    }

    /// Key events still waiting to be sent to a machine's keyboard.
    pub fn keys_pending(&self, slot: usize) -> usize {
        self.m(slot).map(|m| m.board.kbd_queue.len()).unwrap_or(0)
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
        if let Some(m) = self.m_mut(FILESERVER) {
            m.board.attach_hard_disk(Some(Disk::hard_auto(data.to_vec())));
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
