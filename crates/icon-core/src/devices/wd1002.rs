//! Western Digital WD1002-05 Winchester/floppy controller, modelled at the
//! host task-file level (the WD1010 command set). Drive selects 0-2 in SDH
//! address hard disks; select 3 is the floppy, reached through the board's
//! WD2797. Sector data moves straight to and from in-memory disk images.
//!
//! ICON ports (even addresses): FAA0 data, FAA2 error / write-precomp,
//! FAA4 sector count, FAA6 sector number, FAA8 cylinder low, FAAA cylinder
//! high, FAAC SDH, FAAE status / command. INTRQ is PPI A port A bit 5.

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

impl Disk {
    /// A blank hard disk (e.g. 306/4/17 for 10 MB, 1024/8/17 for 64 MB).
    pub fn blank_hard(cylinders: u16, heads: u8, sectors: u8) -> Self {
        let size = cylinders as usize * heads as usize * sectors as usize * 512;
        Self { data: vec![0; size], cylinders, heads, sectors, first_sector: 0, write_protected: false, dirty: true }
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
    /// Cycles left before a busy command completes.
    busy_for: u32,
    /// Drive activity, for front-panel LEDs: 0-2 hard disk, 3 floppy.
    pub activity: [u32; 4],
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
            busy_for: 0,
            activity: [0; 4],
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

    fn location(&self) -> Option<usize> {
        let disk = self.drives[self.drive()].as_ref()?;
        disk.offset(self.cyl, self.head(), self.sector)
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
            let head = self.head();
            if let Some(disk) = self.drives[d].as_mut() {
                let fill = if disk.first_sector == 1 { 0xE5 } else { 0x00 };
                for s in 0..disk.sectors {
                    if let Some(o) = disk.offset(self.cyl, head, s + disk.first_sector) {
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

    /// Multi-sector transfers continue while the sector count is non-zero.
    fn next_sector_or_done(&mut self) {
        self.count = self.count.wrapping_sub(1);
        if self.count != 0 {
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
