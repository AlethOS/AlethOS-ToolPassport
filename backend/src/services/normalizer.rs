use crate::domain::ExternalIdentifier;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedIdentifier {
    pub namespace: String,
    pub value: String,
    pub canonical_url: String,
}

impl NormalizedIdentifier {
    pub fn key(&self) -> String {
        format!("{}:{}", self.namespace, self.value)
    }

    pub fn to_external_identifier(&self) -> ExternalIdentifier {
        ExternalIdentifier {
            namespace: self.namespace.clone(),
            value: self.value.clone(),
            canonical_url: self.canonical_url.clone(),
        }
    }
}

/// Normalize one strong URL identifier without network access.
///
/// Returns `None` for invalid or ambiguous URLs. This is a direct port of
/// `scripts/validate_tool_identity.py::normalize_url`.
pub fn normalize_url(raw_url: &str) -> Option<NormalizedIdentifier> {
    // Reject leading/trailing whitespace, non-ASCII, embedded whitespace, backslash.
    if raw_url.trim_start() != raw_url
        || raw_url.trim_end() != raw_url
        || !raw_url.is_ascii()
        || raw_url.chars().any(|c| c.is_whitespace())
        || raw_url.contains('\\')
    {
        return None;
    }

    // Parse scheme.
    let scheme_end = raw_url.find("://")?;
    let scheme = &raw_url[..scheme_end];
    if !scheme.eq_ignore_ascii_case("https") {
        return None;
    }
    let rest = &raw_url[scheme_end + 3..];

    // Reject credentials (@ in authority).
    if rest.contains('@') {
        return None;
    }

    // Split authority from path at first '/'.
    let (authority, path_and_more) = if let Some(slash_pos) = rest.find('/') {
        (&rest[..slash_pos], &rest[slash_pos..])
    } else {
        (rest, "")
    };

    // Reject query (?), fragment (#), or extra slashes in path_and_more.
    if path_and_more.contains('?') || path_and_more.contains('#') {
        return None;
    }

    // Parse host and port from authority.
    let host;
    if let Some(colon_pos) = authority.rfind(':') {
        // Port present: ensure no colon in host (IPv6 not supported).
        if authority[..colon_pos].contains(':') {
            return None;
        }
        host = &authority[..colon_pos];
        let port_str = &authority[colon_pos + 1..];
        let port = port_str.parse::<u16>().ok()?;
        if port != 443 {
            return None;
        }
    } else {
        host = authority;
    }

    if host.is_empty() {
        return None;
    }

    // Reject percent-encoding, double-slashes, dot segments in path.
    let path = if path_and_more.is_empty() {
        ""
    } else {
        if path_and_more.contains('%')
            || path_and_more.contains("//")
            || path_and_more.contains("/./")
            || path_and_more.contains("/../")
            || path_and_more.ends_with("/.")
            || path_and_more.ends_with("/..")
        {
            return None;
        }
        path_and_more
    };

    let host_lower = host.to_ascii_lowercase();

    // Reject trailing dot on host.
    if host_lower.ends_with('.') {
        return None;
    }

    let path_stripped = path.trim_end_matches('/');

    // GitHub-specific normalization.
    if host_lower == "github.com" {
        let segments: Vec<&str> = path_stripped.split('/').filter(|s| !s.is_empty()).collect();
        if segments.len() != 2 {
            return None;
        }
        let owner = segments[0];
        let mut repo = segments[1];

        // Strip .git suffix (case-insensitive).
        if repo.to_ascii_lowercase().ends_with(".git") {
            repo = &repo[..repo.len() - 4];
        }

        if owner.is_empty() || repo.is_empty() {
            return None;
        }

        let value = format!(
            "{}/{}",
            owner.to_ascii_lowercase(),
            repo.to_ascii_lowercase()
        );
        let canonical_url = format!("https://github.com/{value}");
        return Some(NormalizedIdentifier {
            namespace: "github".to_owned(),
            value,
            canonical_url,
        });
    }

    // General HTTPS normalization: lowercase host, preserve path case.
    let value = if path_stripped.is_empty() {
        host_lower.clone()
    } else {
        format!("{host_lower}{path_stripped}")
    };
    let canonical_url = format!("https://{value}");
    Some(NormalizedIdentifier {
        namespace: "url".to_owned(),
        value,
        canonical_url,
    })
}

/// Validate tool_id format: must be `github:` or `url:` followed by non-empty,
/// non-whitespace content.
pub fn validate_tool_id_format(tool_id: &str) -> bool {
    let rest = if let Some(rest) = tool_id.strip_prefix("github:") {
        rest
    } else if let Some(rest) = tool_id.strip_prefix("url:") {
        rest
    } else {
        return false;
    };
    !rest.is_empty() && !rest.chars().any(|c| c.is_whitespace())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_normalization() {
        let cases = [
            ("https://github.com/Owner/Repo", "github:owner/repo"),
            ("https://github.com/Owner/Repo.git", "github:owner/repo"),
            ("https://github.com/Owner/Repo/", "github:owner/repo"),
            ("https://GitHub.com/Owner/Repo.git/", "github:owner/repo"),
            ("https://github.com/owner/repo", "github:owner/repo"),
        ];
        for (url, expected_key) in cases {
            let result = normalize_url(url).expect("{url} should normalize");
            assert_eq!(result.key(), expected_key, "URL: {url}");
        }
    }

    #[test]
    fn github_rejects_subpath() {
        assert!(normalize_url("https://github.com/owner/repo/tree/main").is_none());
        assert!(normalize_url("https://github.com/owner").is_none());
        assert!(normalize_url("https://github.com/owner/repo/extra").is_none());
    }

    #[test]
    fn https_normalization() {
        let cases = [
            ("https://example.com/Tool", "url:example.com/Tool"),
            ("https://Example.com:443/Tool/", "url:example.com/Tool"),
            ("https://example.com", "url:example.com"),
        ];
        for (url, expected_key) in cases {
            let result = normalize_url(url).expect("{url} should normalize");
            assert_eq!(result.key(), expected_key, "URL: {url}");
        }
    }

    #[test]
    fn rejects_invalid_urls() {
        let invalid = [
            "http://example.com/tool",
            "https://example.com/tool?query=1",
            "https://example.com/tool#fragment",
            "https://user:pass@example.com/tool",
            "https://example.com:8080/tool",
            "ftp://example.com/tool",
            " https://example.com/tool",
            "https://example.com/../tool",
            "https://example.com/tool/../other",
            "https://example.com/%2Ftool",
            "not a url",
        ];
        for url in invalid {
            assert!(
                normalize_url(url).is_none(),
                "URL should be rejected: {url}"
            );
        }
    }

    #[test]
    fn tool_id_format() {
        assert!(validate_tool_id_format("github:owner/repo"));
        assert!(validate_tool_id_format("url:example.com/tool"));
        assert!(!validate_tool_id_format("invalid:id"));
        assert!(!validate_tool_id_format("github:"));
        assert!(!validate_tool_id_format("url:"));
        assert!(!validate_tool_id_format("github:has space"));
        assert!(!validate_tool_id_format("no-namespace"));
    }
}
