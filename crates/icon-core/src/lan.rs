//! An ICONET segment: several machines stepped in lockstep, with their
//! COM9026 controllers linked. Token passing is not modelled; a transmit is
//! delivered after a short delay once the chip has queued it:
//! - broadcast (DID 0): copied to every node receiving with broadcasts on;
//!   the sender gets TA without TMA;
//! - unicast to a node that is receiving: delivered, sender gets TA + TMA;
//! - unicast to a node that is not receiving yet: NAK, retried later;
//! - unicast to an absent node: TA without TMA.

use crate::machine::Machine;

/// CPU cycles between a transmit being queued and its delivery (~140 us).
const DELIVERY_DELAY: u64 = 1_000;
/// Lockstep slice length in CPU cycles.
const SLICE: u64 = 500;

pub struct Lan {
    pub nodes: Vec<Option<Machine>>,
    /// Per node: when its current transmit was first seen.
    tx_seen: Vec<Option<u64>>,
    pub packets: u64,
    /// Per node: packets it has sent or received (for activity lights).
    pub activity: Vec<u64>,
}

impl Lan {
    pub fn new(slots: usize) -> Self {
        Self { nodes: (0..slots).map(|_| None).collect(), tx_seen: vec![None; slots], packets: 0, activity: vec![0; slots] }
    }

    /// Keep internal per-slot state in step after `nodes` grows.
    pub fn grow(&mut self) {
        self.tx_seen.resize(self.nodes.len(), None);
        self.activity.resize(self.nodes.len(), 0);
    }

    pub fn power_on(&mut self, slot: usize, mut m: Machine) {
        m.board.arcnet.networked = true;
        // Line up the new machine's clock with the others so delivery timing works.
        let now = self.now();
        m.board.cycles = now;
        self.nodes[slot] = Some(m);
        self.tx_seen[slot] = None;
    }

    pub fn power_off(&mut self, slot: usize) -> Option<Machine> {
        self.tx_seen[slot] = None;
        self.nodes[slot].take()
    }

    fn now(&self) -> u64 {
        self.nodes.iter().flatten().map(|m| m.board.cycles).max().unwrap_or(0)
    }

    /// Run every powered machine for `cycles` CPU clocks.
    pub fn run(&mut self, cycles: u64) {
        let mut left = cycles;
        while left > 0 {
            let n = left.min(SLICE);
            for m in self.nodes.iter_mut().flatten() {
                m.run(n);
            }
            self.network();
            left -= n;
        }
    }

    fn network(&mut self) {
        // A node that came out of reset makes everyone reconfigure.
        let mut joined = false;
        for m in self.nodes.iter_mut().flatten() {
            joined |= std::mem::take(&mut m.board.arcnet.joined);
        }
        if joined {
            for m in self.nodes.iter_mut().flatten() {
                m.board.arcnet.network_reconfigured();
            }
        }
        for i in 0..self.nodes.len() {
            let Some((did, packet)) = self.nodes[i].as_ref().and_then(|m| m.board.arcnet.pending_tx()) else {
                self.tx_seen[i] = None;
                continue;
            };
            let now = self.nodes[i].as_ref().unwrap().board.cycles;
            let seen = *self.tx_seen[i].get_or_insert(now);
            if now < seen + DELIVERY_DELAY {
                continue;
            }
            let sid = packet[0];
            if std::env::var_os("ICON_NET_LOG").is_some() {
                // Short packets: data from offset packet[2] to 256; long: packet[3] (packet[2] = 0) to 512.
                let (start, end) = if packet[2] != 0 { (packet[2] as usize, 256) } else { (packet[3] as usize, 512) };
                let d = &packet[start..end];
                let hex: String = d.iter().take(24).map(|b| format!("{b:02x}")).collect();
                eprintln!("NET t={:.3}s {sid:>3} -> {did:>3} len {:>3}: {hex}", now as f64 / crate::machine::CPU_HZ as f64, d.len());
            }
            if did == 0 {
                for (j, m) in self.nodes.iter_mut().enumerate() {
                    if let Some(m) = m.as_mut().filter(|_| j != i) {
                        if m.board.arcnet.rx_ready(true) {
                            m.board.arcnet.deliver(&packet);
                            self.activity[j] += 1;
                        }
                    }
                }
                self.finish(i, false);
                continue;
            }
            let target = self.nodes.iter().position(|m| {
                m.as_ref().is_some_and(|m| m.board.arcnet.node_id == did && m.board.arcnet.node_id != sid)
            });
            match target {
                Some(j) if self.nodes[j].as_ref().unwrap().board.arcnet.rx_ready(false) => {
                    self.nodes[j].as_mut().unwrap().board.arcnet.deliver(&packet);
                    self.activity[j] += 1;
                    self.finish(i, true);
                }
                Some(_) => {} // NAK: the receiver is busy; retry at the next pass
                None => self.finish(i, false),
            }
        }
    }

    fn finish(&mut self, i: usize, acked: bool) {
        if let Some(m) = self.nodes[i].as_mut() {
            m.board.arcnet.complete_tx(acked);
        }
        self.tx_seen[i] = None;
        self.packets += 1;
        self.activity[i] += 1;
    }
}
