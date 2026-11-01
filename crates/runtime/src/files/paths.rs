//! Case-insensitive Windows paths rooted at the selected game directory.
pub(super) fn normalize(cwd: &str, path: &str) -> String {
    preserve_case(cwd, path).to_lowercase()
}
/// Resolve separators and relative components without losing the stored spelling.
pub(super) fn preserve_case(cwd: &str, path: &str) -> String {
    let path = path.replace('\\', "/");
    let absolute = path.starts_with('/') || path.as_bytes().get(1) == Some(&b':');
    let path = if path.as_bytes().get(1) == Some(&b':') {
        &path[2..]
    } else {
        &path
    };
    let mut parts: Vec<&str> = if absolute {
        Vec::new()
    } else {
        cwd.split('/').filter(|s| !s.is_empty()).collect()
    };
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            value => parts.push(value),
        }
    }
    if absolute
        && parts
            .first()
            .is_some_and(|part| part.eq_ignore_ascii_case("sims"))
    {
        parts.remove(0);
    }
    parts.join("/")
}
pub(super) fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(directory, _)| directory)
}
pub(super) fn absolute(path: &str) -> String {
    if path.is_empty() {
        "C:\\Sims".into()
    } else {
        format!("C:\\Sims\\{}", path.replace('/', "\\"))
    }
}
pub(super) fn wildcard(pattern: &str, name: &str) -> bool {
    let pattern = if !name.contains('.') {
        pattern
            .strip_suffix(".*")
            .or_else(|| pattern.strip_suffix('.'))
            .unwrap_or(pattern)
    } else {
        pattern
    };
    let (pattern, name) = (pattern.as_bytes(), name.as_bytes());
    let (mut p, mut n, mut star, mut retry) = (0, 0, None, 0);
    while n < name.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p] == name[n]) {
            p += 1;
            n += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            p += 1;
            retry = n;
        } else if let Some(s) = star {
            retry += 1;
            n = retry;
            p = s + 1;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}
