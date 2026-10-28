//! SMC COM9026 ARCNET controller.
//!
//! Register-level model from the SMC datasheet; see
//! docs/com9026.md in this repo. Phase 1 covers reset, status,
//! interrupt mask and the flag commands. Packet transfer between nodes
//! (token passing, ACK/NAK) arrives in phase 3.

pub const ST_TA: u8 = 0x01; // transmitter available
pub const ST_TMA: u8 = 0x02; // transmitted message acknowledged
pub const ST_RECON: u8 = 0x04; // network reconfiguration
pub const ST_POR: u8 = 0x10; // power-on reset (always interrupts)
pub const ST_RI: u8 = 0x80; // receiver inhibited

/// The power-on reconfiguration burst ends about 2.8 ms after reset.
const RECON_DELAY_CYCLES: u64 = 20_000;

#[derive(Clone, Debug)]
pub struct Com9026 {
    /// 2 KB buffer RAM: four 512-byte pages.
    pub ram: Box<[u8; 2048]>,
    pub status: u8,
    pub mask: u8,
    pub node_id: u8,
    /// Receive enabled into this page; also accept broadcasts.
    pub rx_page: Option<(u8, bool)>,
    /// Transmit pending from this page.
    pub tx_page: Option<u8>,
    in_reset: bool,
    recon_at: Option<u64>,
    /// When a pending transmit finishes (no other nodes yet: it completes
    /// unacknowledged, i.e. TA set and TMA clear, as with an absent target).
    tx_done_at: Option<u64>,
    now: u64,
    /// On a network the `Lan` delivers packets and completes transmits; a
    /// standalone chip completes them unacknowledged by itself.
    pub networked: bool,
    /// Set when the chip comes out of reset (the `Lan` broadcasts RECON).
    pub joined: bool,
}

impl Default for Com9026 {
    fn default() -> Self {
        Self {
            ram: Box::new([0; 2048]),
            status: ST_RI | ST_POR | ST_TA,
            mask: 0,
            node_id: 0,
            rx_page: None,
            tx_page: None,
            in_reset: false,
            recon_at: None,
            tx_done_at: None,
            now: 0,
            networked: false,
            joined: false,
        }
    }
}

impl Com9026 {
    /// Drive the RESET pin at time `now` (CPU cycles). The node ID is latched
    /// from the external bus while reset is held. On release the chip writes
    /// D1h and its ID to RAM[0..2], and the status reads 91h (RI|POR|TA).
    pub fn set_reset(&mut self, asserted: bool, id_bus: u8, now: u64) {
        if asserted {
            self.in_reset = true;
            self.node_id = id_bus;
        } else if self.in_reset {
            self.in_reset = false;
            self.ram[0] = 0xD1;
            self.ram[1] = self.node_id;
            self.status = ST_RI | ST_POR | ST_TA;
            self.mask = 0;
            self.rx_page = None;
            self.tx_page = None;
            self.recon_at = Some(now + RECON_DELAY_CYCLES);
            self.joined = true;
        }
    }

    pub fn tick(&mut self, now: u64) {
        self.now = now;
        if let Some(t) = self.tx_done_at {
            if now >= t {
                self.tx_done_at = None;
                self.tx_page = None;
                self.status |= ST_TA;
            }
        }
        if let Some(t) = self.recon_at {
            if now >= t {
                self.status |= ST_RECON;
                self.recon_at = None;
            }
        }
    }

    pub fn read_status(&self) -> u8 {
        self.status
    }

    pub fn write_mask(&mut self, v: u8) {
        self.mask = v & (ST_RI | ST_RECON | ST_TA);
    }

    pub fn command(&mut self, v: u8) {
        match v & 0x07 {
            0x01 => {
                // Disable transmitter (takes effect at the next token; immediate here).
                self.tx_page = None;
                self.status |= ST_TA;
            }
            0x02 => {
                self.rx_page = None;
                self.status |= ST_RI;
            }
            0x03 => {
                // Enable transmit from page nn.
                self.tx_page = Some((v >> 3) & 3);
                self.status &= !(ST_TA | ST_TMA);
                // Standalone: free-buffer enquiry with no reply, then give up (~1 ms).
                if !self.networked {
                    self.tx_done_at = Some(self.now + 7_000);
                }
            }
            0x04 => {
                self.rx_page = Some(((v >> 3) & 3, v & 0x80 != 0));
                self.status &= !ST_RI;
            }
            0x05 => {} // define configuration (long packets): no state needed yet
            0x06 => {
                if v & 0x08 != 0 {
                    self.status &= !ST_POR;
                }
                if v & 0x10 != 0 {
                    self.status &= !ST_RECON;
                }
            }
            _ => {}
        }
    }

    /// The packet waiting to go out: (destination, page bytes with SID filled in).
    pub fn pending_tx(&self) -> Option<(u8, [u8; 512])> {
        let page = self.tx_page? as usize;
        let mut buf = [0u8; 512];
        buf.copy_from_slice(&self.ram[page * 512..page * 512 + 512]);
        buf[0] = self.node_id;
        Some((buf[1], buf))
    }

    /// Finish the pending transmit. `acked` sets TMA (never for broadcasts).
    pub fn complete_tx(&mut self, acked: bool) {
        self.tx_page = None;
        self.status |= ST_TA | if acked { ST_TMA } else { 0 };
    }

    /// Can this node take a packet right now (receiver enabled, broadcast flag)?
    pub fn rx_ready(&self, broadcast: bool) -> bool {
        matches!(self.rx_page, Some((_, b)) if !broadcast || b)
    }

    /// Store a received packet and inhibit the receiver (one packet per enable).
    pub fn deliver(&mut self, packet: &[u8; 512]) {
        if let Some((page, _)) = self.rx_page.take() {
            let p = page as usize * 512;
            self.ram[p..p + 512].copy_from_slice(packet);
            self.status |= ST_RI;
        }
    }

    /// Another node joined the network: reconfiguration after a short burst.
    pub fn network_reconfigured(&mut self) {
        self.recon_at = Some(self.now + RECON_DELAY_CYCLES);
    }

    /// INTR output, seen by the ICON on PPI A port C bit 5.
    pub fn irq(&self) -> bool {
        self.status & ST_POR != 0 || self.status & self.mask != 0
    }
}
