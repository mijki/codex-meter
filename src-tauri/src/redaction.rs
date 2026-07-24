pub fn redact_diagnostic(input: &str) -> String {
    input
        .split_whitespace()
        .map(|part| {
            let lower = part.to_ascii_lowercase();
            if lower.starts_with("bearer")
                || lower.contains("token=")
                || lower.contains("api_key")
                || lower.contains("authorization:")
            {
                "[REDACTED]"
            } else if part.contains(":\\Users\\") || part.contains("/home/") {
                "[LOCAL_PATH]"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_common_secret_and_path_shapes() {
        let redacted =
            redact_diagnostic("authorization: Bearer-secret token=abc C:\\Users\\person\\repo");
        assert!(!redacted.contains("abc"));
        assert!(!redacted.contains("person"));
    }
}
