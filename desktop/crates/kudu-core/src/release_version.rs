//! Release versions ("0.1.3") and tags ("v0.1.3"), compared numerically, for the update check.

/// "v0.1.4" → "0.1.4"; `None` if it isn't a three-part version.
pub fn from_tag(tag: &str) -> Option<String> {
    let version = tag.strip_prefix('v').unwrap_or(tag);
    parts(version).map(|_| version.to_string())
}

/// Whether `candidate` is a later release than `current` (0.1.10 is later than 0.1.9).
pub fn is_newer(candidate: &str, current: &str) -> bool {
    matches!((parts(candidate), parts(current)), (Some(a), Some(b)) if a > b)
}

fn parts(version: &str) -> Option<[u32; 3]> {
    let mut numbers = version.split('.').map(|part| part.parse::<u32>().ok());
    let parts = [numbers.next()??, numbers.next()??, numbers.next()??];
    numbers.next().is_none().then_some(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_tags() {
        assert_eq!(from_tag("v0.1.4").as_deref(), Some("0.1.4"));
        assert_eq!(from_tag("0.2.10").as_deref(), Some("0.2.10"));
        assert_eq!(from_tag("latest"), None);
        assert_eq!(from_tag("v1.0"), None);
    }

    #[test]
    fn compares_numerically() {
        assert!(is_newer("0.1.4", "0.1.3"));
        assert!(is_newer("0.1.10", "0.1.9"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("0.1.3", "0.1.3"));
        assert!(!is_newer("0.1.2", "0.1.3"));
        assert!(!is_newer("garbage", "0.1.3"));
    }
}
