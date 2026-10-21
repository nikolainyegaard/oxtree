//! Shell-style wildcard matching with `*` and `?`, enough for QDirStat's
//! category patterns such as `lib*.so.*` and `moc_*.cpp`.

pub fn glob_match(pattern: &str, text: &str, case_sensitive: bool) -> bool {
    let p: Vec<char> = if case_sensitive {
        pattern.chars().collect()
    } else {
        pattern.to_lowercase().chars().collect()
    };
    let t: Vec<char> = if case_sensitive {
        text.chars().collect()
    } else {
        text.to_lowercase().chars().collect()
    };
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        match p.get(pi) {
            Some('*') => {
                star = Some((pi, ti));
                pi += 1;
            }
            Some('?') => {
                pi += 1;
                ti += 1;
            }
            Some(c) if Some(c) == t.get(ti) => {
                pi += 1;
                ti += 1;
            }
            _ => match star {
                Some((sp, st)) => {
                    pi = sp + 1;
                    ti = st + 1;
                    star = Some((sp, st + 1));
                }
                None => return false,
            },
        }
    }
    while p.get(pi) == Some(&'*') {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::glob_match;

    #[test]
    fn patterns() {
        assert!(glob_match("lib*.so.*", "libfoo.so.1.2", true));
        assert!(!glob_match("lib*.so.*", "libfoo.so", true));
        assert!(glob_match("lib*.so", "libfoo.so", true));
        assert!(glob_match("moc_*.cpp", "moc_Main.cpp", true));
        assert!(!glob_match("moc_*.cpp", "Moc_Main.cpp", true));
        assert!(glob_match("pack-*.pack", "PACK-abc.PACK", false));
        assert!(glob_match("core", "core", true));
        assert!(!glob_match("core", "core.1", true));
        assert!(glob_match("a?c", "abc", true));
        assert!(glob_match("*", "", true));
    }
}
