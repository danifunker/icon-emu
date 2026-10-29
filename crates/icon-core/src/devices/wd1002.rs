//! Western Digital WD1002-05 Winchester/floppy controller, modelled at the
//! host task-file level (the WD1010 command set). Drive selects 0-2 in SDH
//! address hard disks; select 3 is the floppy, reached through the board's
//! WD2797. Sector data moves straight to and from in-memory disk images.
//!
//! ICON ports (even addresses): FAA0 data, FAA2 error / write-precomp,
//! FAA4 sector count, FAA6 sector number, FAA8 cylinder low, FAAA cylinder
//! high, FAAC SDH, FAAE status / command. INTRQ is PPI A port A bit 5.

/// Drive geometries (cylinders, heads, sectors per track).
pub const HD_10MB: (u16, u8, u8) = (306, 4, 17);
pub const HD_64MB: (u16, u8, u8) = (1024, 8, 17);

pub const ST_BSY: u8 = 0x80;
pub const ST_RDY: u8 = 0x40;
pub const ST_SC: u8 = 0x10;
pub const ST_DRQ: u8 = 0x08;
pub const ST_ERR: u8 = 0x01;

pub const ER_ID_NOT_FOUND: u8 = 0x10;
pub const ER_ABORT: u8 = 0x04;

/// A disk image with a fixed geometry. Sectors are stored in cylinder, head,
/// sector order; `first_sector` is 0 for hard disks and 1 for floppies.
#[derive(Clone, Debug)]
pub struct Disk {
    pub data: Vec<u8>,
    pub cylinders: u16,
    pub heads: u8,
    pub sectors: u8,
    pub first_sector: u8,
    pub write_protected: bool,
    /// Set on every write, so a front end knows to save the image.
    pub dirty: bool,
}

const FD640_BLANK: &[u8] = include_bytes!("fd640_blank.bin");

impl Disk {
    /// A blank hard disk (e.g. 306/4/17 for 10 MB, 1024/8/17 for 64 MB).
    pub fn blank_hard(cylinders: u16, heads: u8, sectors: u8) -> Self {
        let size = cylinders as usize * heads as usize * sectors as usize * 512;
        Self { data: vec![0; size], cylinders, heads, sectors, first_sector: 0, write_protected: false, dirty: true }
    }

    /// A 640K (80x2x8) QNX 2 floppy, already initialised with `dinit 2`
    /// (empty root directory and bitmap, 1280 blocks). It's QNX's default
    /// floppy format on the ICON. Only the first three blocks are non-zero;
    /// they were captured from `dinit` run on the emulated LEXICON.
    pub fn formatted_floppy_640k() -> Vec<u8> {
        let mut data = vec![0; 655_360];
        data[..FD640_BLANK.len()].copy_from_slice(FD640_BLANK);
        data
    }

    /// A hard-disk image whose geometry is taken from its block-1 geometry
    /// record when present, otherwise from its size: 10 MB = 306x4x17,
    /// 64 MB (Micropolis 1335 class) = 1024x8x17.
    pub fn hard_auto(data: Vec<u8>) -> Self {
        let (heads, spt) = if data.len() >= 512 && data[0x10] == 2 && data[0x2F] == 3 {
            let (heads, spt) = (data[0x13].max(1), u16::from_le_bytes([data[0x1C], data[0x1D]]).max(1) as u8);
            return Self::hard_from_image(Self::drop_padded_heads(data, heads, spt), heads, spt);
        } else if data.len() == HD_64MB.0 as usize * HD_64MB.1 as usize * HD_64MB.2 as usize * 512 {
            (HD_64MB.1, HD_64MB.2)
        } else {
            (4, 17)
        };
        Self::hard_from_image(data, heads, spt)
    }

    /// MFM drive readers can extract a drive as if it had 8 heads (e.g. a
    /// 6-head ST-251 read as 820 x 8 x 17), leaving the missing heads as zero
    /// sectors in every cylinder. When the geometry record says fewer heads
    /// and the image is exactly that 8-head size, keep only the real heads.
    fn drop_padded_heads(data: Vec<u8>, heads: u8, spt: u8) -> Vec<u8> {
        let total = u32::from_le_bytes([data[0x14], data[0x15], data[0x16], data[0x17]]) as usize;
        let (real, padded) = (heads as usize * spt as usize * 512, 8 * spt as usize * 512);
        if heads >= 8 || total == 0 || data.len() != total / (heads as usize * spt as usize) * padded {
            return data;
        }
        data.chunks(padded).flat_map(|cyl| &cyl[..real]).copied().collect()
    }

    pub fn hard_from_image(data: Vec<u8>, heads: u8, sectors: u8) -> Self {
        let cylinders = (data.len() / (heads as usize * sectors as usize * 512)) as u16;
        Self { data, cylinders, heads, sectors, first_sector: 0, write_protected: false, dirty: false }
    }

    /// A QNX floppy image; geometry is inferred from the size
    /// (655,360 bytes = 80x2x8, 819,200 = 80x2x10, 327,680 = 40x2x8 ...).
    pub fn floppy_from_image(data: Vec<u8>) -> Option<Self> {
        let (c, h, s) = match data.len() {
            819_200 => (80, 2, 10),
            737_280 => (80, 2, 9),
            655_360 => (80, 2, 8),
            409_600 => (40, 2, 10),
            368_640 => (40, 2, 9),
            327_680 => (40, 2, 8),
            163_840 => (40, 1, 8),
            _ => return None,
        };
        Some(Self { data, cylinders: c, heads: h, sectors: s, first_sector: 1, write_protected: false, dirty: false })
    }

    fn offset(&self, cyl: u16, head: u8, sector: u8) -> Option<usize> {
        if cyl >= self.cylinders || head >= self.heads {
            return None;
        }
        let s = sector.checked_sub(self.first_sector)?;
        if s >= self.sectors {
            return None;
        }
        let lba = (cyl as usize * self.heads as usize + head as usize) * self.sectors as usize + s as usize;
        Some(lba * 512)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Xfer {
    None,
    Read,
    Write,
    Format,
}

#[derive(Debug)]
pub struct Wd1002 {
    /// Hard disks on drive selects 0-2 and the floppy on select 3.
    pub drives: [Option<Disk>; 4],
    pub error: u8,
    pub count: u8,
    pub sector: u8,
    pub cyl: u16,
    pub sdh: u8,
    pub status: u8,
    pub irq: bool,
    buffer: Box<[u8; 512]>,
    ptr: usize,
    xfer: Xfer,
    /// Read/write with the M (multiple sector) bit: continue while the
    /// sector count is non-zero. Without it the WD1010 moves one sector.
    multi: bool,
    /// Cycles left before a busy command completes.
    busy_for: u32,
    /// Drive activity, for front-panel LEDs: 0-2 hard disk, 3 floppy.
    pub activity: [u32; 4],
    /// Debug: commands issued, by high nibble; and the last few (cmd, cyl, sdh, sector, count).
    pub cmd_counts: [u32; 16],
    pub last_cmds: std::collections::VecDeque<(u8, u16, u8, u8, u8)>,
}

impl Default for Wd1002 {
    fn default() -> Self {
        Self {
            drives: Default::default(),
            error: 0, // power-on diagnostics passed
            count: 1,
            sector: 1,
            cyl: 0,
            sdh: 0,
            status: ST_RDY | ST_SC,
            irq: false,
            buffer: Box::new([0; 512]),
            ptr: 0,
            xfer: Xfer::None,
            multi: false,
            busy_for: 0,
            activity: [0; 4],
            cmd_counts: [0; 16],
            last_cmds: Default::default(),
        }
    }
}

impl Wd1002 {
    fn drive(&self) -> usize {
        ((self.sdh >> 3) & 3) as usize
    }
    fn head(&self) -> u8 {
        if self.drive() == 3 { self.sdh & 1 } else { self.sdh & 7 }
    }

    pub fn read(&mut self, reg: u8) -> u8 {
        match reg {
            0 => self.read_data(),
            1 => self.error,
            2 => self.count,
            3 => self.sector,
            4 => self.cyl as u8,
            5 => (self.cyl >> 8) as u8,
            6 => self.sdh,
            _ => {
                self.irq = false;
                self.status
            }
        }
    }

    pub fn write(&mut self, reg: u8, v: u8) {
        match reg {
            0 => self.write_data(v),
            1 => {} // write precompensation
            2 => self.count = v,
            3 => self.sector = v,
            4 => self.cyl = (self.cyl & 0xFF00) | v as u16,
            5 => self.cyl = (self.cyl & 0x00FF) | ((v as u16) << 8),
            6 => self.sdh = v,
            _ => self.command(v),
        }
    }

    /// Advance time; finishes commands that are still busy.
    pub fn tick(&mut self, cycles: u32) {
        if self.busy_for > 0 {
            self.busy_for = self.busy_for.saturating_sub(cycles);
            if self.busy_for == 0 {
                self.finish();
            }
        }
        for a in &mut self.activity {
            *a = a.saturating_sub(cycles);
        }
    }

    fn command(&mut self, v: u8) {
        self.cmd_counts[(v >> 4) as usize] += 1;
        if self.last_cmds.len() == 12 {
            self.last_cmds.pop_front();
        }
        self.last_cmds.push_back((v, self.cyl, self.sdh, self.sector, self.count));
        self.error = 0;
        self.irq = false;
        self.status = ST_BSY | ST_RDY | ST_SC;
        self.busy_for = 200; // a short busy period, so polling code sees BSY
        self.xfer = match v >> 4 {
            0x2 => Xfer::Read,
            0x3 => Xfer::Write,
            0x5 => Xfer::Format,
            _ => Xfer::None,
        };
        self.multi = v & 0x04 != 0;
        if v >> 4 == 1 {
            self.cyl = 0; // restore
        }
        self.ptr = 0;
    }

    /// End of the busy period: start the data phase or complete the command.
    fn finish(&mut self) {
        let d = self.drive();
        if self.drives[d].is_none() && self.xfer != Xfer::None {
            return self.fail(ER_ABORT);
        }
        match self.xfer {
            Xfer::Read => {
                if !self.load_sector() {
                    return self.fail(ER_ID_NOT_FOUND);
                }
                self.status = ST_RDY | ST_SC | ST_DRQ;
                self.irq = true;
            }
            Xfer::Write | Xfer::Format => {
                if self.drives[d].as_ref().is_some_and(|x| x.write_protected) {
                    return self.fail(ER_ABORT);
                }
                self.status = ST_RDY | ST_SC | ST_DRQ;
            }
            Xfer::None => {
                self.status = ST_RDY | ST_SC;
                self.irq = true;
            }
        }
    }

    fn fail(&mut self, err: u8) {
        self.error = err;
        self.status = ST_RDY | ST_SC | ST_ERR;
        self.xfer = Xfer::None;
        self.irq = true;
    }

    /// The cylinder to address. For the floppy, QNX sets bit 8 of the
    /// cylinder register when it reads a 40-track (360K) disk in the 80-track
    /// drive, so the board can double-step; the image is indexed by the
    /// disk's own track number, so the flag is dropped.
    fn cylinder(&self) -> u16 {
        if self.drive() == 3 { self.cyl & 0xFF } else { self.cyl }
    }

    fn location(&self) -> Option<usize> {
        let disk = self.drives[self.drive()].as_ref()?;
        disk.offset(self.cylinder(), self.head(), self.sector)
    }

    fn load_sector(&mut self) -> bool {
        let d = self.drive();
        match self.location() {
            Some(o) => {
                let disk = self.drives[d].as_ref().unwrap();
                self.buffer.copy_from_slice(&disk.data[o..o + 512]);
                self.activity[d] = 200_000;
                true
            }
            None => false,
        }
    }

    fn read_data(&mut self) -> u8 {
        let v = self.buffer[self.ptr];
        if self.xfer == Xfer::Read && self.status & ST_DRQ != 0 {
            self.ptr += 1;
            if self.ptr == 512 {
                self.ptr = 0;
                self.next_sector_or_done();
            }
        }
        v
    }

    fn write_data(&mut self, v: u8) {
        self.buffer[self.ptr] = v;
        if self.status & ST_DRQ == 0 || !matches!(self.xfer, Xfer::Write | Xfer::Format) {
            self.ptr = (self.ptr + 1) % 512;
            return;
        }
        self.ptr += 1;
        if self.ptr < 512 {
            return;
        }
        self.ptr = 0;
        let d = self.drive();
        if self.xfer == Xfer::Format {
            // Fill every sector of the addressed track.
            let (head, cyl) = (self.head(), self.cylinder());
            if let Some(disk) = self.drives[d].as_mut() {
                let fill = if disk.first_sector == 1 { 0xE5 } else { 0x00 };
                for s in 0..disk.sectors {
                    if let Some(o) = disk.offset(cyl, head, s + disk.first_sector) {
                        disk.data[o..o + 512].fill(fill);
                        disk.dirty = true;
                    }
                }
            }
            self.status = ST_RDY | ST_SC;
            self.xfer = Xfer::None;
            self.irq = true;
            return;
        }
        match self.location() {
            Some(o) => {
                let disk = self.drives[d].as_mut().unwrap();
                disk.data[o..o + 512].copy_from_slice(&self.buffer[..]);
                disk.dirty = true;
                self.activity[d] = 200_000;
                self.next_sector_or_done();
            }
            None => self.fail(ER_ID_NOT_FOUND),
        }
    }

    /// Multi-sector transfers (M bit) continue while the sector count is non-zero.
    fn next_sector_or_done(&mut self) {
        self.count = self.count.wrapping_sub(1);
        if self.multi && self.count != 0 {
            self.sector = self.sector.wrapping_add(1);
            if self.xfer == Xfer::Read && !self.load_sector() {
                return self.fail(ER_ID_NOT_FOUND);
            }
            self.irq = self.xfer == Xfer::Read;
            return;
        }
        self.status = ST_RDY | ST_SC;
        if self.xfer == Xfer::Write {
            self.irq = true;
        }
        self.xfer = Xfer::None;
    }
}
