/// Produce a safe, bounded diagnostic string from a raw upstream response body.
///
/// Callers should never include the full `body` in an error message or log line
/// because upstream providers may echo back forwarded auth headers or internal
/// detail.  This helper returns only the first `limit` bytes rendered as lossy
/// UTF-8, with a truncation marker appended when the input is longer.
pub fn truncate_for_log(body: &str) -> String {
    const LIMIT: usize = 200;
    if body.len() <= LIMIT {
        body.to_owned()
    } else {
        format!("{}…[truncated, {} bytes total]", &body[..LIMIT], body.len())
    }
}

#[cfg(test)]
mod tests {
    use super::truncate_for_log;

    #[test]
    fn short_string_passes_through() {
        let s = "hello";
        assert_eq!(truncate_for_log(s), "hello");
    }

    #[test]
    fn exactly_limit_is_not_truncated() {
        let s = "x".repeat(200);
        let out = truncate_for_log(&s);
        assert_eq!(out, s);
        assert!(!out.contains("truncated"));
    }

    #[test]
    fn over_limit_is_truncated_with_marker() {
        let s = "x".repeat(201);
        let out = truncate_for_log(&s);
        assert!(out.contains("[truncated, 201 bytes total]"));
        assert!(out.starts_with(&"x".repeat(200)));
    }

    #[test]
    fn empty_string_passes_through() {
        assert_eq!(truncate_for_log(""), "");
    }
}
