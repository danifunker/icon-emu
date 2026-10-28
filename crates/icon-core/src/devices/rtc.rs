//! OKI MSM5832-style real-time clock on the fileserver (docs/nvram_rtc_disk_install.md §2).
//!
//! Thirteen 4-bit BCD registers: S1 S10 MI1 MI10 H1 H10 W D1 D10 MO1 MO10 Y1 Y10.
//! H10 bit 3 = 24-hour mode, bit 2 = PM; D10 bit 2 = leap-year flag.
//!
//! The host's clock is the time source, but QNX 2 dates only reach 1995, so
//! the year is moved to 1991 (1992 when the host year is a leap year).
//! Writes from `set_rtc` are kept as an offset.

#[derive(Clone, Debug, Default)]
pub struct Msm5832 {
    /// Host time (Unix seconds) when the machine powered on.
    pub host_epoch: i64,
    /// Emulated time minus host time.
    offset: i64,
    regs: [u8; 13],
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Days since 1970-01-01 -> (year, month, day). Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 } as i64;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

impl Msm5832 {
    pub fn new(host_unix_secs: i64) -> Self {
        let (y, m, d) = civil_from_days(host_unix_secs.div_euclid(86_400));
        let target = if is_leap(y) { 1992 } else { 1991 };
        let shifted = days_from_civil(target, m, d) * 86_400 + host_unix_secs.rem_euclid(86_400);
        Self { host_epoch: host_unix_secs, offset: shifted - host_unix_secs, regs: [0; 13] }
    }

    fn now(&self, elapsed_secs: i64) -> i64 {
        self.host_epoch + elapsed_secs + self.offset
    }

    /// Recompute the register file from the clock.
    fn latch(&mut self, elapsed_secs: i64) {
        let t = self.now(elapsed_secs);
        let (y, mo, d) = civil_from_days(t.div_euclid(86_400));
        let secs = t.rem_euclid(86_400);
        let (h, mi, s) = (secs / 3600, (secs / 60) % 60, secs % 60);
        let wday = (t.div_euclid(86_400) + 4).rem_euclid(7); // 1970-01-01 was a Thursday
        let yy = y.rem_euclid(100);
        self.regs = [
            (s % 10) as u8,
            (s / 10) as u8,
            (mi % 10) as u8,
            (mi / 10) as u8,
            (h % 10) as u8,
            (h / 10) as u8 | 0x08, // 24-hour mode
            wday as u8,
            (d % 10) as u8,
            (d / 10) as u8 | if is_leap(y) { 0x04 } else { 0 },
            (mo % 10) as u8,
            (mo / 10) as u8,
            (yy % 10) as u8,
            (yy / 10) as u8,
        ];
    }

    pub fn read(&mut self, reg: u8, elapsed_secs: i64) -> u8 {
        self.latch(elapsed_secs);
        self.regs.get(reg as usize).copied().unwrap_or(0x0F)
    }

    pub fn write(&mut self, reg: u8, val: u8, elapsed_secs: i64) {
        self.latch(elapsed_secs);
        let Some(r) = self.regs.get_mut(reg as usize) else { return };
        *r = val & 0x0F;
        // Re-derive the clock from the registers as written.
        let g = &self.regs;
        let digit = |i: usize, mask: u8| (g[i] & mask) as i64;
        let s = digit(1, 7) * 10 + digit(0, 15);
        let mi = digit(3, 7) * 10 + digit(2, 15);
        let mut h = digit(5, 3) * 10 + digit(4, 15);
        if g[5] & 0x08 == 0 && g[5] & 0x04 != 0 && h < 12 {
            h += 12; // 12-hour mode, PM
        }
        let d = (digit(8, 3) * 10 + digit(7, 15)).clamp(1, 31) as u32;
        let mo = (digit(10, 1) * 10 + digit(9, 15)).clamp(1, 12) as u32;
        let yy = digit(12, 15) * 10 + digit(11, 15);
        let y = if yy >= 70 { 1900 + yy } else { 2000 + yy };
        let t = days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + s;
        self.offset = t - (self.host_epoch + elapsed_secs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_host_date_into_qnx_range() {
        // 2026-10-28 12:34:56 UTC
        let t = days_from_civil(2026, 10, 28) * 86_400 + 12 * 3600 + 34 * 60 + 56;
        let mut r = Msm5832::new(t);
        let regs: Vec<u8> = (0..13).map(|i| r.read(i, 0)).collect();
        assert_eq!(regs, vec![6, 5, 4, 3, 2, 1 | 8, 1, 8, 2, 0, 1, 1, 9]); // Mon 28-10-91
        r.write(11, 0, 0); // year 90
        assert_eq!(r.read(11, 10), 0);
        assert_eq!(r.read(0, 10), 6); // 56 + 10 s = 06
    }
}
