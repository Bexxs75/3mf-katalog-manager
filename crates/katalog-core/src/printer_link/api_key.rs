//! The API key of a printer. The type has no `Debug`, `Display` or `Serialize` on
//! purpose: an accidental `{:?}`, log line, error text or DTO cannot print it.

/// Moonraker's own keys are 32 characters; the range leaves room for other front ends.
const MAX_LEN: usize = 256;

#[derive(Clone, PartialEq, Eq)]
pub struct ApiKey(String);

/// Why a key was refused. Carries no part of the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApiKeyFormatError;

impl ApiKey {
    /// Trims surrounding whitespace (a pasted key often has a trailing newline). Only visible
    /// ASCII is accepted, so the value is always a valid header value and can't smuggle a
    /// second header line.
    pub fn parse(raw: &str) -> Result<ApiKey, ApiKeyFormatError> {
        let t = raw.trim();
        if t.is_empty() || t.len() > MAX_LEN || !t.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(ApiKeyFormatError);
        }
        Ok(ApiKey(t.to_string()))
    }

    /// For the database layer and the request header only.
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_moonraker_key_and_trims() {
        let key = ApiKey::parse("  0123456789abcdef0123456789abcdef\n").unwrap();
        assert_eq!(key.expose(), "0123456789abcdef0123456789abcdef");
    }

    #[test]
    fn refuses_empty_long_and_unprintable_values() {
        assert!(ApiKey::parse("").is_err());
        assert!(ApiKey::parse("   \n").is_err());
        assert!(ApiKey::parse(&"a".repeat(MAX_LEN + 1)).is_err());
        assert!(ApiKey::parse(&"a".repeat(MAX_LEN)).is_ok());
        for bad in ["abc def", "abc\r\nX-Evil: 1", "abc\u{0}def", "abc\u{e4}def", "abc\tdef"] {
            assert!(ApiKey::parse(bad).is_err(), "{bad:?}");
        }
    }
}
