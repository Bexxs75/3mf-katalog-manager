//! The user's own API key. The type has no `Debug`, `Display` or `Serialize` on
//! purpose: an accidental `{:?}`, log line or DTO cannot print it.

/// Shortest and longest accepted key. The form of a real key is not confirmed; the
/// character set below is a cautious superset of what API tokens usually use.
const MIN_LEN: usize = 8;
const MAX_LEN: usize = 256;

#[derive(Clone, PartialEq, Eq)]
pub struct Token(String);

/// Why a key was refused. Carries no part of the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenFormatError;

fn allowed(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '~' | '+' | '/' | '=' | '-')
}

impl Token {
    /// Trims surrounding whitespace (a pasted key often has a trailing newline) and
    /// checks length and characters.
    pub fn parse(raw: &str) -> Result<Token, TokenFormatError> {
        let t = raw.trim();
        if !(MIN_LEN..=MAX_LEN).contains(&t.len()) || !t.chars().all(allowed) {
            return Err(TokenFormatError);
        }
        Ok(Token(t.to_string()))
    }

    /// For the database layer and the request header only.
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }

    /// Four bullets plus the last four characters, or only bullets for a short key.
    /// Never the key itself.
    pub fn mask(&self) -> String {
        if self.0.len() >= 16 {
            format!("\u{2022}\u{2022}\u{2022}\u{2022}{}", &self.0[self.0.len() - 4..])
        } else {
            "\u{2022}".repeat(8)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_keys_and_trims() {
        assert!(Token::parse("  abcdef0123456789\n").is_ok());
        assert!(Token::parse("a.b_c~d+e/f=g-h").is_ok());
    }

    #[test]
    fn refuses_empty_short_long_and_odd_characters() {
        assert!(Token::parse("").is_err());
        assert!(Token::parse("short").is_err());
        assert!(Token::parse(&"a".repeat(257)).is_err());
        assert!(Token::parse("abcdefgh ijkl").is_err());
        assert!(Token::parse("abcdefgh\u{e4}ijkl").is_err());
        assert!(Token::parse("abcdefgh;ijkl").is_err());
    }

    #[test]
    fn mask_shows_only_the_last_four_of_a_long_key() {
        let long = Token::parse("abcd1234efgh5678a3f9").unwrap();
        assert_eq!(long.mask(), "\u{2022}\u{2022}\u{2022}\u{2022}a3f9");
        let short = Token::parse("abcd1234efgh").unwrap();
        assert_eq!(short.mask(), "\u{2022}".repeat(8));
        assert!(!short.mask().contains('a'));
    }
}
