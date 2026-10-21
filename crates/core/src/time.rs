//! Unix time to a `YYYY-MM-DD HH:MM` string without a time zone library.
//! The caller supplies the local UTC offset in seconds; the days-to-civil
//! arithmetic is Howard Hinnant's.

pub fn format(unix: i64, utc_offset_secs: i32) -> String {
    if unix == 0 {
        return String::new();
    }
    let t = unix.saturating_add(utc_offset_secs as i64);
    let days = t.div_euclid(86_400);
    let rem = t.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}",
        rem / 3600,
        (rem % 3600) / 60
    )
}

/// Year, month and day for a count of days since 1970-01-01.
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Parse a `+0200` / `-0530` style offset as printed by `date +%z`.
pub fn parse_offset(s: &str) -> Option<i32> {
    let s = s.trim();
    let (sign, digits) = match s.chars().next()? {
        '+' => (1, s.get(1..)?),
        '-' => (-1, s.get(1..)?),
        _ => return None,
    };
    if digits.len() != 4 {
        return None;
    }
    let h: i32 = digits.get(0..2)?.parse().ok()?;
    let m: i32 = digits.get(2..4)?.parse().ok()?;
    Some(sign * (h * 3600 + m * 60))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
        assert_eq!(format(0x65e0_ce47, 0), "2024-02-29 18:34");
        assert_eq!(format(0x65e0_ce47, 3600), "2024-02-29 19:34");
        assert_eq!(format(0, 0), "");
        assert_eq!(parse_offset("+0200"), Some(7200));
        assert_eq!(parse_offset("-0530"), Some(-19_800));
        assert_eq!(parse_offset("nope"), None);
    }
}
