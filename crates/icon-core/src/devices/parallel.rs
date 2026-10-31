//! The parallel printer port, and a printer that takes every byte at once.
//!
//! How the 4.x kernel drives it (traced in the emulator; kernel code at
//! 1E7F2 and the INT 61h handler at 1E897 in the 4.10.01 `hd_boot`):
//! - PPI B is set to `8C`: port B is a **mode 1 (strobed) output**, the data.
//! - The kernel writes the byte to port B (FAC3), then pulses the printer's
//!   STROBE on **PPI A port C bit 2** (`out F987,04 / out F987,05`).
//! - The printer's BUSY is **PPI B port C bit 6** (FAC5), high while busy.
//!   The kernel polls it after each byte and from its timer.
//! - The printer's ACK sets the 8255's INTR_B (**PPI B port C bit 0**) when
//!   INTE_B (port C bit 2, set with `out FAC7,05`) is on. INTR_B raises INT2,
//!   whose handler polls FAC5 bit 0 and calls INT 61h. Writing port B clears it.
//!
//! Jobs: the bytes since the printer last went idle. A front end takes a
//! finished job with [`Parallel::take_job`] and renders it (crate `icon-printer`).

#[derive(Clone, Debug)]
pub struct Parallel {
    /// A printer is connected and online. When false, BUSY stays high and
    /// the ICON waits, as with a printer switched off-line.
    pub online: bool,
    /// Bytes of the job being printed.
    pub job: Vec<u8>,
    /// Cycle count of the last byte received.
    pub last_byte: u64,
    /// Bytes printed since power-on.
    pub total: u64,
    /// INTR_B: the printer acknowledged a byte.
    pub intr: bool,
    /// Last level of the STROBE line (PPI A PC2).
    strobe: bool,
}

impl Default for Parallel {
    fn default() -> Self {
        Self { online: true, job: Vec::new(), last_byte: 0, total: 0, intr: false, strobe: true }
    }
}

impl Parallel {
    /// PPI A port C bit 2 changed. The printer latches `data` as STROBE
    /// rises, then acknowledges, which sets INTR_B if `inte`.
    pub fn set_strobe(&mut self, level: bool, data: u8, inte: bool, cycles: u64) {
        if level && !self.strobe && self.online {
            self.job.push(data);
            self.total += 1;
            self.last_byte = cycles;
            self.intr = inte;
        }
        self.strobe = level;
    }

    /// The finished job, once the printer has received nothing for `idle`
    /// cycles; it is then taken from the port. Jobs of only NULs are dropped.
    pub fn take_job(&mut self, now: u64, idle: u64) -> Option<Vec<u8>> {
        if self.job.is_empty() || now.saturating_sub(self.last_byte) < idle {
            return None;
        }
        let job = std::mem::take(&mut self.job);
        // The ICON strobes a NUL out while it starts up; that is no printout.
        job.iter().any(|&b| b != 0).then_some(job)
    }

    /// Bytes have arrived within the last `window` cycles (the printer is working).
    pub fn active(&self, now: u64, window: u64) -> bool {
        !self.job.is_empty() && now.saturating_sub(self.last_byte) < window
    }
}
