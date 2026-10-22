//! WizTree's file search syntax: terms separated by spaces are ANDed, `|`
//! between terms is OR, quotes group words and force a name match, `!`
//! negates. A term is a name pattern (substring, or a glob with `*` and
//! `?`), a `/regex`, or a comparison: `>100m`, `a<=1g` (allocated size),
//! `<2024/01/01`, `>=today-7`, `>now-2h`, `=13:00`, `namelen>40`,
//! `pathlen<100`, `=folder`, `=file`.
//!
//! Dates and times are local; the parser takes the current time and the
//! UTC offset so the result is plain unix timestamps.

use crate::glob::glob_match;
use crate::time::days_from_civil;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmp {
    Eq,
    Gt,
    Ge,
    Lt,
    Le,
}

impl Cmp {
    fn holds<T: PartialOrd>(self, a: T, b: T) -> bool {
        match self {
            Cmp::Eq => a == b,
            Cmp::Gt => a > b,
            Cmp::Ge => a >= b,
            Cmp::Lt => a < b,
            Cmp::Le => a <= b,
        }
    }
}

#[derive(Clone, Debug)]
enum Kind {
    /// Lowercased substring of the name or path.
    Substr(String),
    /// Lowercased glob over the name or path.
    Glob(String),
    Regex(regex_lite::Regex),
    Size(Cmp, u64),
    /// A whole local day: start and end as unix seconds.
    Day(Cmp, i64, i64),
    Time(Cmp, i64),
    NameLen(Cmp, usize),
    PathLen(Cmp, usize),
    IsDir(bool),
}

#[derive(Clone, Debug)]
struct Term {
    negate: bool,
    kind: Kind,
}

/// A parsed query: AND over groups, OR inside a group.
#[derive(Clone, Debug, Default)]
pub struct Query {
    groups: Vec<Vec<Term>>,
}

/// What a query is matched against. `path` may be empty when
/// `needs_path` is false.
pub struct Item<'a> {
    pub name: &'a str,
    pub path: &'a str,
    pub size: u64,
    pub mtime: i64,
    pub is_dir: bool,
}

impl Query {
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    /// Parse `text`. `now` and `utc_offset_secs` resolve `today` and `now`.
    pub fn parse(text: &str, now: i64, utc_offset_secs: i32) -> Result<Query, String> {
        let mut groups = Vec::new();
        for token in tokenize(text) {
            let mut group = Vec::new();
            for alt in split_alternatives(&token) {
                if alt.is_empty() {
                    continue;
                }
                group.push(parse_term(&alt, now, utc_offset_secs)?);
            }
            if !group.is_empty() {
                groups.push(group);
            }
        }
        Ok(Query { groups })
    }

    /// Whether matching needs the full path: a name term in path mode or a
    /// path length term.
    pub fn needs_path(&self, match_path: bool) -> bool {
        self.groups.iter().flatten().any(|t| match t.kind {
            Kind::Substr(_) | Kind::Glob(_) | Kind::Regex(_) => match_path,
            Kind::PathLen(..) => true,
            _ => false,
        })
    }

    pub fn matches(&self, item: &Item, match_path: bool) -> bool {
        self.groups.iter().all(|group| {
            group.iter().any(|t| {
                let hit = term_matches(&t.kind, item, match_path);
                hit != t.negate
            })
        })
    }

    /// Character range of the first positive substring term found in
    /// `text`, for highlighting. Globs and regexes are not highlighted.
    pub fn highlight(&self, text: &str) -> Option<(usize, usize)> {
        let lower: Vec<char> = text.to_lowercase().chars().collect();
        if lower.len() != text.chars().count() {
            return None;
        }
        for t in self.groups.iter().flatten() {
            if t.negate {
                continue;
            }
            if let Kind::Substr(s) = &t.kind {
                let needle: Vec<char> = s.chars().collect();
                if needle.is_empty() || needle.len() > lower.len() {
                    continue;
                }
                if let Some(pos) = lower
                    .windows(needle.len())
                    .position(|w| w == needle.as_slice())
                {
                    return Some((pos, pos + needle.len()));
                }
            }
        }
        None
    }
}

fn term_matches(kind: &Kind, item: &Item, match_path: bool) -> bool {
    let subject = if match_path { item.path } else { item.name };
    match kind {
        Kind::Substr(s) => subject.to_lowercase().contains(s.as_str()),
        Kind::Glob(g) => glob_match(g, subject, false),
        Kind::Regex(r) => r.is_match(subject),
        Kind::Size(c, v) => c.holds(item.size, *v),
        Kind::Day(c, start, end) => match c {
            Cmp::Eq => item.mtime >= *start && item.mtime < *end,
            Cmp::Gt => item.mtime >= *end,
            Cmp::Ge => item.mtime >= *start,
            Cmp::Lt => item.mtime < *start,
            Cmp::Le => item.mtime < *end,
        },
        Kind::Time(c, t) => c.holds(item.mtime, *t),
        Kind::NameLen(c, n) => c.holds(item.name.chars().count(), *n),
        Kind::PathLen(c, n) => c.holds(item.path.chars().count(), *n),
        Kind::IsDir(d) => item.is_dir == *d,
    }
}

/// Split on whitespace outside double quotes; quotes are kept so the term
/// parser can tell a quoted name from an operator.
fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in text.chars() {
        if c == '"' {
            quoted = !quoted;
            cur.push(c);
        } else if c.is_whitespace() && !quoted {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Split a token on `|` outside quotes.
fn split_alternatives(token: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in token.chars() {
        if c == '"' {
            quoted = !quoted;
            cur.push(c);
        } else if c == '|' && !quoted {
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    out.push(cur);
    out
}

fn parse_term(alt: &str, now: i64, offset: i32) -> Result<Term, String> {
    let (negate, rest) = match alt.strip_prefix('!') {
        Some(r) if !r.starts_with('=') => (true, r),
        _ => (false, alt),
    };
    let kind = if rest.len() >= 2 && rest.starts_with('"') && rest.ends_with('"') {
        name_kind(rest.get(1..rest.len() - 1).unwrap_or(""))
    } else if let Some(re) = rest.strip_prefix('/') {
        let re = re.trim_matches('"');
        Kind::Regex(
            regex_lite::RegexBuilder::new(re)
                .case_insensitive(true)
                .build()
                .map_err(|e| format!("Bad regular expression: {e}"))?,
        )
    } else if let Some((field, cmp, value)) = split_comparison(rest) {
        comparison_kind(field, cmp, value, now, offset)?
    } else {
        name_kind(&rest.replace('"', ""))
    };
    Ok(Term { negate, kind })
}

fn name_kind(s: &str) -> Kind {
    let lower = s.to_lowercase();
    if lower.contains('*') || lower.contains('?') {
        Kind::Glob(lower)
    } else {
        Kind::Substr(lower)
    }
}

/// `a>=10m` into (`a`, Ge, `10m`); `>100k` into (``, Gt, `100k`).
fn split_comparison(s: &str) -> Option<(&str, Cmp, &str)> {
    let i = s.find(['=', '>', '<'])?;
    let field = s.get(..i)?;
    if !field.is_empty() && !matches!(field.to_lowercase().as_str(), "a" | "namelen" | "pathlen") {
        return None;
    }
    let ops = s.get(i..)?;
    let (cmp, len) = if ops.starts_with(">=") {
        (Cmp::Ge, 2)
    } else if ops.starts_with("<=") {
        (Cmp::Le, 2)
    } else if ops.starts_with('>') {
        (Cmp::Gt, 1)
    } else if ops.starts_with('<') {
        (Cmp::Lt, 1)
    } else {
        (Cmp::Eq, 1)
    };
    Some((field, cmp, ops.get(len..)?))
}

fn comparison_kind(
    field: &str,
    cmp: Cmp,
    value: &str,
    now: i64,
    offset: i32,
) -> Result<Kind, String> {
    let field = field.to_lowercase();
    let v = value.trim().to_lowercase();
    if v.is_empty() {
        return Err("A comparison needs a value, like >100m or <2024/01/01".into());
    }
    match field.as_str() {
        "namelen" => return parse_len(&v).map(|n| Kind::NameLen(cmp, n)),
        "pathlen" => return parse_len(&v).map(|n| Kind::PathLen(cmp, n)),
        _ => {}
    }
    if cmp == Cmp::Eq {
        match v.as_str() {
            "folder" | "folders" | "dir" => return Ok(Kind::IsDir(true)),
            "file" | "files" => return Ok(Kind::IsDir(false)),
            _ => {}
        }
    }
    if let Some(n) = parse_size(&v) {
        return Ok(Kind::Size(cmp, n));
    }
    if let Some((start, end)) = parse_day(&v, now, offset) {
        return Ok(Kind::Day(cmp, start, end));
    }
    if let Some(t) = parse_time(&v, now, offset) {
        return Ok(Kind::Time(cmp, t));
    }
    Err(format!(
        "Cannot read \"{value}\": use a size like 100m, a date like 2024/01/01 or today-7, a time like 13:00 or now-2h, folder or file"
    ))
}

fn parse_len(v: &str) -> Result<usize, String> {
    v.parse()
        .map_err(|_| format!("Cannot read \"{v}\" as a length"))
}

/// `100`, `100k`, `1.5gb`: bytes.
fn parse_size(v: &str) -> Option<u64> {
    let digits_end = v
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(v.len());
    let (num, unit) = v.split_at(digits_end);
    let n: f64 = num.parse().ok()?;
    let mult: f64 = match unit {
        "" | "b" => 1.0,
        "k" | "kb" => 1024.0,
        "m" | "mb" => 1024.0 * 1024.0,
        "g" | "gb" => 1024.0 * 1024.0 * 1024.0,
        "t" | "tb" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((n * mult) as u64)
}

/// `2024/01/01` or `today`, `today-7`, `today+1`: local day as unix start
/// and end.
fn parse_day(v: &str, now: i64, offset: i32) -> Option<(i64, i64)> {
    let days = if let Some(rest) = v.strip_prefix("today") {
        let delta: i64 = if rest.is_empty() {
            0
        } else {
            rest.parse().ok()?
        };
        (now + offset as i64).div_euclid(86_400) + delta
    } else {
        let mut parts = v.split(['/', '-']);
        let y: i64 = parts.next()?.parse().ok()?;
        let m: u32 = parts.next()?.parse().ok()?;
        let d: u32 = parts.next()?.parse().ok()?;
        if parts.next().is_some() || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
            return None;
        }
        days_from_civil(y, m, d)
    };
    let start = days * 86_400 - offset as i64;
    Some((start, start + 86_400))
}

/// `13:00`, `1:00pm`, `13:00:00`, `now`, `now-30m`, `now+2h`: unix seconds.
fn parse_time(v: &str, now: i64, offset: i32) -> Option<i64> {
    if let Some(rest) = v.strip_prefix("now") {
        if rest.is_empty() {
            return Some(now);
        }
        let unit = rest.chars().last()?;
        let (num, mult) = match unit {
            's' => (rest.get(..rest.len() - 1)?, 1),
            'm' => (rest.get(..rest.len() - 1)?, 60),
            'h' => (rest.get(..rest.len() - 1)?, 3600),
            _ => (rest, 1),
        };
        let n: i64 = num.parse().ok()?;
        return Some(now + n * mult);
    }
    let (clock, pm) = if let Some(c) = v.strip_suffix("pm") {
        (c, Some(true))
    } else if let Some(c) = v.strip_suffix("am") {
        (c, Some(false))
    } else {
        (v, None)
    };
    let mut parts = clock.split(':');
    let mut h: i64 = parts.next()?.parse().ok()?;
    let m: i64 = parts.next()?.parse().ok()?;
    let s: i64 = parts.next().map(|p| p.parse().ok()).unwrap_or(Some(0))?;
    if parts.next().is_some() || h > 23 || m > 59 || s > 59 {
        return None;
    }
    match pm {
        Some(true) if h < 12 => h += 12,
        Some(false) if h == 12 => h = 0,
        _ => {}
    }
    let local_now = now + offset as i64;
    let day_start = local_now.div_euclid(86_400) * 86_400;
    Some(day_start + h * 3600 + m * 60 + s - offset as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000; // 2023-11-14 22:13:20 UTC
    const OFF: i32 = 3600;

    fn q(s: &str) -> Query {
        Query::parse(s, NOW, OFF).unwrap()
    }

    fn file(name: &'static str, size: u64, mtime: i64) -> Item<'static> {
        Item {
            name,
            path: "",
            size,
            mtime,
            is_dir: false,
        }
    }

    #[test]
    fn names_globs_and_or() {
        assert!(q("foo").matches(&file("MyFooFile.txt", 1, 0), false));
        assert!(!q("foo").matches(&file("bar.txt", 1, 0), false));
        assert!(q("*.mp3").matches(&file("song.MP3", 1, 0), false));
        assert!(!q("*.mp3").matches(&file("song.mp3.bak", 1, 0), false));
        assert!(q("*.mp3|*.ogg").matches(&file("a.ogg", 1, 0), false));
        assert!(q("a b").matches(&file("abba", 1, 0), false));
        assert!(!q("a c").matches(&file("abba", 1, 0), false));
        assert!(q("!foo").matches(&file("bar", 1, 0), false));
        assert!(q("\"my file\"").matches(&file("My File.txt", 1, 0), false));
        assert!(q("\">weird\"").matches(&file("a>weird name", 1, 0), false));
    }

    #[test]
    fn sizes_and_kinds() {
        assert!(q(">100m").matches(&file("a", 200 << 20, 0), false));
        assert!(!q(">100m").matches(&file("a", 50 << 20, 0), false));
        assert!(q("a>=1k").matches(&file("a", 1024, 0), false));
        assert!(q("<=1.5gb").matches(&file("a", 1 << 30, 0), false));
        assert!(q("=folder").matches(
            &Item {
                name: "d",
                path: "",
                size: 0,
                mtime: 0,
                is_dir: true
            },
            false
        ));
        assert!(!q("=file").matches(
            &Item {
                name: "d",
                path: "",
                size: 0,
                mtime: 0,
                is_dir: true
            },
            false
        ));
        assert!(q("namelen>3").matches(&file("abcd", 1, 0), false));
        assert!(Query::parse(">banana", NOW, OFF).is_err());
        assert!(Query::parse(">", NOW, OFF).is_err());
    }

    #[test]
    fn dates_and_times_are_local() {
        // 2023-11-14 local (UTC+1) runs from 2023-11-13 23:00 UTC
        let day_start = NOW - (NOW + OFF as i64).rem_euclid(86_400);
        assert!(q("=today").matches(&file("a", 1, day_start), false));
        assert!(!q("=today").matches(&file("a", 1, day_start - 1), false));
        assert!(q("<today").matches(&file("a", 1, day_start - 1), false));
        assert!(q(">=today-1").matches(&file("a", 1, day_start - 1), false));
        assert!(q("=2023/11/14").matches(&file("a", 1, NOW), false));
        assert!(!q("=2023/11/15").matches(&file("a", 1, NOW), false));
        assert!(q(">2020/01/01").matches(&file("a", 1, NOW), false));
        assert!(q("<=now").matches(&file("a", 1, NOW), false));
        assert!(q(">now-1h").matches(&file("a", 1, NOW - 1800), false));
        assert!(!q(">now-1h").matches(&file("a", 1, NOW - 7200), false));
        // local 13:00 today is 12:00 UTC on 2023-11-14
        let local_day = (NOW + OFF as i64).div_euclid(86_400) * 86_400;
        let one_pm = local_day + 13 * 3600 - OFF as i64;
        assert!(q("=1:00pm").matches(&file("a", 1, one_pm), false));
        assert!(q("=13:00:00").matches(&file("a", 1, one_pm), false));
    }

    #[test]
    fn path_mode_regex_and_highlight() {
        let item = Item {
            name: "x.log",
            path: "/var/log/x.log",
            size: 1,
            mtime: 0,
            is_dir: false,
        };
        assert!(!q("var").matches(&item, false));
        assert!(q("var").matches(&item, true));
        assert!(q("var").needs_path(true));
        assert!(!q("var").needs_path(false));
        assert!(q("pathlen>5").needs_path(false));
        assert!(q("/^x\\.l.g$").matches(&item, false));
        assert!(q("/VAR/").matches(&item, true));
        assert!(Query::parse("/(", NOW, OFF).is_err());
        assert_eq!(q("Log").highlight("x.LOG"), Some((2, 5)));
        assert_eq!(q("*.log").highlight("x.log"), None);
        assert!(q("").is_empty());
    }
}
