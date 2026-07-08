//! Secret redaction helpers for logs and diagnostics bundles.

use crate::SecretRedactor;
use regex::Regex;

/// Redacts common secret patterns from strings before logging or exporting them.
#[derive(Clone)]
pub struct DefaultSecretRedactor {
    patterns: Vec<(Regex, &'static str)>,
}

impl DefaultSecretRedactor {
    /// Create a new redactor with built-in secret patterns.
    ///
    /// # Panics
    ///
    /// Panics if any of the built-in regex patterns are invalid.
    #[must_use]
    pub fn new() -> Self {
        let patterns = vec![
            // api_key, api-key, apikey followed by a value.
            (
                Regex::new(r"(?i)(api[_-]?key)\s*[:=]\s*\S+").expect("valid regex"),
                "${1} [REDACTED]",
            ),
            // Generic token followed by a value.
            (
                Regex::new(r"(?i)(token)\s*[:=]\s*\S+").expect("valid regex"),
                "${1} [REDACTED]",
            ),
            // Password followed by a value.
            (
                Regex::new(r"(?i)(password)\s*[:=]\s*\S+").expect("valid regex"),
                "${1} [REDACTED]",
            ),
            // Secret followed by a value.
            (
                Regex::new(r"(?i)(secret)\s*[:=]\s*\S+").expect("valid regex"),
                "${1} [REDACTED]",
            ),
            // OpenAI-style secret keys.
            (
                Regex::new(r"sk-[a-zA-Z0-9]{10,}").expect("valid regex"),
                "[REDACTED]",
            ),
            // Authorization header value (e.g. "Authorization: Bearer <token>").
            (
                Regex::new(r"(?i)(Authorization:)\s*\S+(?:\s+\S+)*").expect("valid regex"),
                "${1} [REDACTED]",
            ),
            // Bearer token.
            (
                Regex::new(r"(?i)(Bearer\s+)\S+").expect("valid regex"),
                "${1}[REDACTED]",
            ),
        ];
        Self { patterns }
    }
}

impl Default for DefaultSecretRedactor {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretRedactor for DefaultSecretRedactor {
    fn redact(&self, text: &str) -> String {
        let mut result = text.to_owned();
        for (pattern, replacement) in &self.patterns {
            result = pattern.replace_all(&result, *replacement).into_owned();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_api_key() {
        let redactor = DefaultSecretRedactor::new();
        let out = redactor.redact("api_key=super-secret-value");
        assert!(!out.contains("super-secret-value"));
        assert!(out.contains("[REDACTED]"));
    }

    #[test]
    fn redacts_bearer_token() {
        let redactor = DefaultSecretRedactor::new();
        let out = redactor.redact("Authorization: Bearer abc123xyz");
        assert!(!out.contains("abc123xyz"));
        assert!(out.contains("[REDACTED]"));
    }

    #[test]
    fn redacts_sk_key() {
        let redactor = DefaultSecretRedactor::new();
        let out = redactor.redact("sk-abcdefghijklmnopqrstuvwxyz");
        assert!(!out.contains("abcdefghijklmnopqrstuvwxyz"));
        assert!(out.contains("[REDACTED]"));
    }

    #[test]
    fn leaves_innocent_text_unchanged() {
        let redactor = DefaultSecretRedactor::new();
        let text = "hello world";
        assert_eq!(redactor.redact(text), text);
    }
}
