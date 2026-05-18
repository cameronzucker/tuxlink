//! Pure helpers used by tuxlink's build.rs. Lives in src/ (not in build.rs
//! itself) so cargo test discovers the inline tests via lib.rs's
//! `#[cfg(test)] mod build_support;` line. build.rs picks up the same file
//! via `#[path = "src/build_support.rs"] mod build_support;` — one source
//! of truth, two consumers.

/// Parse Go version output like "go version go1.24.3 linux/amd64" -> (1, 24).
/// Returns None on malformed input.
pub fn parse_go_version(s: &str) -> Option<(u32, u32)> {
    let after_go_version = s.split_whitespace().nth(2)?; // "go1.24.3"
    let trimmed = after_go_version.strip_prefix("go")?;
    let mut parts = trimmed.split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next()?.parse().ok()?;
    Some((major, minor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_go_version_typical() {
        assert_eq!(parse_go_version("go version go1.24.3 linux/amd64\n"), Some((1, 24)));
    }

    #[test]
    fn parse_go_version_no_patch() {
        assert_eq!(parse_go_version("go version go1.25 darwin/arm64\n"), Some((1, 25)));
    }

    #[test]
    fn parse_go_version_malformed_returns_none() {
        assert_eq!(parse_go_version("not a go version string"), None);
        assert_eq!(parse_go_version(""), None);
        assert_eq!(parse_go_version("go version goABC linux/amd64"), None);
    }
}
