//! The Git commit a relay was built from, for its operator: the metrics,
//! the dashboard, and the startup log name it, so a deployment can be told
//! apart from the next. `/v1/info` never does: anyone may read it, and an
//! exact build is what one would look flaws up by.

/// The commit `RELAY_COMMIT` named at compile time, which the image build
/// passes (`scripts/relay-image.sh`); `None` for other builds, or a value
/// that is not an abbreviated or full commit id.
#[must_use]
pub fn commit() -> Option<&'static str> {
    option_env!("RELAY_COMMIT").filter(|text| valid(text))
}

fn valid(text: &str) -> bool {
    (7..=40).contains(&text.len()) && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::valid;

    #[test]
    fn only_a_commit_id_is_named() {
        assert!(valid("e01bada13"));
        assert!(valid(&"a".repeat(40)));
        assert!(!valid("abc"));
        assert!(!valid(&"a".repeat(41)));
        assert!(!valid("e01bada\"} 1\nfake 1"));
    }
}
