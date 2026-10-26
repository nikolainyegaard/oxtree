//! Text formatting with QDirStat's conventions: `1.5 MB`, `739 Bytes`,
//! `12.3%`, `rwxr-xr-x`.

use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

/// How `size` divides and labels. Binary is the IEC form, decimal the SI
/// form; Windows is Explorer's and WizTree's, 1024 per step but labelled
/// as if it were 1000, kept so a Windows user sees Explorer's number next
/// to Explorer's label.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeUnits {
    Binary,
    Decimal,
    Windows,
}

impl Default for SizeUnits {
    /// What the platform's own file manager shows: Dolphin and `du -h`
    /// binary, Finder decimal, Explorer the Windows form.
    fn default() -> Self {
        if cfg!(windows) {
            SizeUnits::Windows
        } else if cfg!(target_os = "macos") {
            SizeUnits::Decimal
        } else {
            SizeUnits::Binary
        }
    }
}

impl SizeUnits {
    fn from_u8(n: u8) -> Self {
        match n {
            1 => SizeUnits::Decimal,
            2 => SizeUnits::Windows,
            _ => SizeUnits::Binary,
        }
    }

    fn step(self) -> f64 {
        match self {
            SizeUnits::Decimal => 1000.0,
            _ => 1024.0,
        }
    }

    fn labels(self) -> &'static [&'static str; 9] {
        match self {
            SizeUnits::Binary => &[
                "Bytes", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB", "ZiB", "YiB",
            ],
            SizeUnits::Decimal => &["Bytes", "kB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB"],
            SizeUnits::Windows => &["Bytes", "KB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB"],
        }
    }
}

/// Decimals in `size`, 0 to 3, and its units; settings, applied once at
/// startup and from the Preferences dialog.
static SIZE_DECIMALS: AtomicU8 = AtomicU8::new(1);
static SIZE_UNITS: AtomicU8 = AtomicU8::new(u8::MAX);

pub fn set_size_decimals(n: u8) {
    SIZE_DECIMALS.store(n.min(3), Ordering::Relaxed);
}

pub fn set_size_units(u: SizeUnits) {
    SIZE_UNITS.store(u as u8, Ordering::Relaxed);
}

fn size_units() -> SizeUnits {
    match SIZE_UNITS.load(Ordering::Relaxed) {
        u8::MAX => SizeUnits::default(),
        n => SizeUnits::from_u8(n),
    }
}

/// `739 Bytes`, `1.5 MiB`: one decimal above the first unit unless the
/// setting says otherwise.
pub fn size(bytes: u64) -> String {
    size_in(
        bytes,
        size_units(),
        SIZE_DECIMALS.load(Ordering::Relaxed) as usize,
    )
}

fn size_in(bytes: u64, units: SizeUnits, decimals: usize) -> String {
    let step = units.step();
    let labels = units.labels();
    let mut v = bytes as f64;
    if v < step {
        return format!("{bytes} Bytes");
    }
    let mut unit = 0;
    while v >= step && unit < labels.len() - 1 {
        v /= step;
        unit += 1;
    }
    format!(
        "{v:.decimals$} {}",
        labels.get(unit).copied().unwrap_or("?")
    )
}

/// `1 234 567 Bytes`, with thin thousands groups like QDirStat.
pub fn bytes(bytes: u64) -> String {
    format!("{} Bytes", grouped(bytes))
}

/// `1 234 567`.
pub fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(ch);
    }
    out
}

pub fn percent(p: f64) -> String {
    if p < 0.0 {
        return String::new();
    }
    format!("{p:.1}%")
}

/// `rwxr-xr-x` from a mode, type bits ignored.
pub fn permissions(mode: u32) -> String {
    let mut s = String::with_capacity(9);
    for (bit, ch) in [
        (0o400, 'r'),
        (0o200, 'w'),
        (0o100, 'x'),
        (0o040, 'r'),
        (0o020, 'w'),
        (0o010, 'x'),
        (0o004, 'r'),
        (0o002, 'w'),
        (0o001, 'x'),
    ] {
        s.push(if mode & bit != 0 { ch } else { '-' });
    }
    if mode & 0o4000 != 0 {
        s.replace_range(2..3, if mode & 0o100 != 0 { "s" } else { "S" });
    }
    if mode & 0o2000 != 0 {
        s.replace_range(5..6, if mode & 0o010 != 0 { "s" } else { "S" });
    }
    if mode & 0o1000 != 0 {
        s.replace_range(8..9, if mode & 0o001 != 0 { "t" } else { "T" });
    }
    s
}

/// `0755`.
pub fn octal_permissions(mode: u32) -> String {
    format!("{:04o}", mode & 0o7777)
}

/// `41.584 sec` or `2:05 min` like QDirStat's elapsed time display.
pub fn elapsed(d: Duration) -> String {
    let secs = d.as_secs_f64();
    if secs < 60.0 {
        format!("{secs:.3} sec")
    } else if secs < 3600.0 {
        format!("{}:{:02} min", (secs / 60.0) as u64, (secs % 60.0) as u64)
    } else {
        format!(
            "{}:{:02}:{:02} h",
            (secs / 3600.0) as u64,
            ((secs % 3600.0) / 60.0) as u64,
            (secs % 60.0) as u64
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        set_size_units(SizeUnits::Binary);
        assert_eq!(size(0), "0 Bytes");
        assert_eq!(size(1023), "1023 Bytes");
        assert_eq!(size(1024), "1.0 KiB");
        assert_eq!(size(1536), "1.5 KiB");
        assert_eq!(size(14_900_000_000), "13.9 GiB");
        assert_eq!(bytes(1_234_567), "1 234 567 Bytes");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1000), "1 000");
    }

    #[test]
    fn unit_forms() {
        let n = 14_900_000_000;
        assert_eq!(size_in(n, SizeUnits::Binary, 1), "13.9 GiB");
        assert_eq!(size_in(n, SizeUnits::Decimal, 1), "14.9 GB");
        assert_eq!(size_in(n, SizeUnits::Windows, 1), "13.9 GB");
        assert_eq!(size_in(1000, SizeUnits::Decimal, 0), "1 kB");
        assert_eq!(size_in(1000, SizeUnits::Binary, 0), "1000 Bytes");
        for u in [SizeUnits::Binary, SizeUnits::Decimal, SizeUnits::Windows] {
            assert_eq!(SizeUnits::from_u8(u as u8), u);
        }
    }

    #[test]
    fn perms_and_percent() {
        assert_eq!(permissions(0o755), "rwxr-xr-x");
        assert_eq!(permissions(0o4755), "rwsr-xr-x");
        assert_eq!(permissions(0o1777), "rwxrwxrwt");
        assert_eq!(octal_permissions(0o100644), "0644");
        assert_eq!(percent(12.345), "12.3%");
        assert_eq!(percent(-1.0), "");
        assert_eq!(elapsed(Duration::from_millis(41584)), "41.584 sec");
        assert_eq!(elapsed(Duration::from_secs(125)), "2:05 min");
    }
}
