//! Text formatting with QDirStat's conventions: `1.5 MB`, `739 Bytes`,
//! `12.3%`, `rwxr-xr-x`.

use std::time::Duration;

const UNITS: [&str; 9] = ["Bytes", "kB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB"];

/// `739 Bytes`, `1.5 MB`: base 1024, one decimal above a kilobyte.
pub fn size(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} Bytes");
    }
    let mut v = bytes as f64;
    let mut unit = 0;
    while v >= 1024.0 && unit < UNITS.len() - 1 {
        v /= 1024.0;
        unit += 1;
    }
    format!("{v:.1} {}", UNITS.get(unit).copied().unwrap_or("?"))
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
        assert_eq!(size(0), "0 Bytes");
        assert_eq!(size(1023), "1023 Bytes");
        assert_eq!(size(1024), "1.0 kB");
        assert_eq!(size(1536), "1.5 kB");
        assert_eq!(size(14_900_000_000), "13.9 GB");
        assert_eq!(bytes(1_234_567), "1 234 567 Bytes");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1000), "1 000");
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
