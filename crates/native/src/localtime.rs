//! The local UTC offset, from `date +%z` (PowerShell on Windows) once at
//! startup. Dates are then
//! formatted with that one offset, so a timestamp on the other side of a
//! daylight saving change shows an hour off. Good enough for a file list;
//! a tz database parser is the upgrade if anyone minds.

use std::process::Command;

pub fn utc_offset_secs() -> i32 {
    if cfg!(windows) {
        return Command::new("powershell")
            .args(["-NoProfile", "-Command"])
            .arg("[int][TimeZoneInfo]::Local.GetUtcOffset([datetime]::Now).TotalSeconds")
            .output()
            .ok()
            .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
            .unwrap_or(0);
    }
    Command::new("date")
        .arg("+%z")
        .output()
        .ok()
        .and_then(|o| oxtree_core::time::parse_offset(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    #[test]
    fn offset_is_a_whole_number_of_minutes_within_a_day() {
        let off = super::utc_offset_secs();
        assert!(off.abs() <= 14 * 3600);
        assert_eq!(off % 60, 0);
    }
}
