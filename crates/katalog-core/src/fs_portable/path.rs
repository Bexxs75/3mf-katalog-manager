//! Path model for the file operations.
//!
//! `NativePath` is an absolute path as it is stored in the database. Its text is kept exactly as
//! given: no separator unification, no prefix rewriting, no lower-casing, no dropping of empty
//! segments. Equality of places is a separate comparison key (`PathKey`). `RelPath` is a portable
//! path below an anchor: `/` only, never empty, never `.` or `..`.
//!
//! Pure text logic, no I/O, so every host can test it (a Windows path parses on Linux too).
use super::{Error, Result};
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Style {
    Posix,
    Windows,
}

/// How an absolute path starts. Ranges point into the text of the owning `NativePath`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Prefix {
    PosixRoot,
    Drive(u8),
    Unc { server: Range<usize>, share: Range<usize> },
    VerbatimDrive(u8),
    VerbatimUnc { server: Range<usize>, share: Range<usize> },
}

/// Anchor of a portable path. Only a type for now: the journal stores no anchor field yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    Root(usize),
    Trash(TrashKind),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrashKind {
    InRoot,
    DesktopAppData,
}

/// Comparison key of a place: equal keys mean the same place by name. Posix: the text itself.
/// Windows: prefix class (drive letter without case, `\\?\C:` equal to `C:`, verbatim UNC equal to
/// UNC) plus the segments compared byte for byte; `/` and `\` are the same separator.
/// Server and share names are compared exactly.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PathKey(String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativePath {
    text: String,
    style: Style,
    prefix: Prefix,
    segments: Vec<Range<usize>>,
}

/// Portable path relative to an anchor: `/`-separated, UTF-8, no empty segment, no `.` or `..`,
/// no `\`, no NUL.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RelPath(String);

fn bad<T>() -> Result<T> {
    Err(Error::PathUnresolvable)
}

/// The segment rules every host shares (and the only ones on Posix).
fn portable_segment(s: &str) -> bool {
    !(s.is_empty() || s == "." || s == ".." || s.contains(['/', '\\', '\0']))
}

fn windows_segment(s: &str) -> bool {
    portable_segment(s)
        && !s.ends_with(['.', ' '])
        && !s.chars().any(|c| c.is_control() || ":<>\"|?*".contains(c))
}

fn is_sep(b: u8) -> bool {
    b == b'\\' || b == b'/'
}

fn windows_unit(s: &str) -> bool {
    !s.is_empty() && s != "." && s != ".." && !s.chars().any(|c| c.is_control() || ":/\\".contains(c))
}

impl RelPath {
    pub fn parse(text: &str) -> Result<Self> {
        if text.split('/').all(portable_segment) {
            Ok(Self(text.to_owned()))
        } else {
            bad()
        }
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.0.split('/')
    }
    pub fn file_name(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or("")
    }
    /// The path without its last segment; `None` for a single segment.
    pub fn parent(&self) -> Option<RelPath> {
        self.0.rsplit_once('/').map(|(p, _)| Self(p.to_owned()))
    }
    /// This path with one more segment.
    pub fn child(&self, name: &str) -> Result<RelPath> {
        if portable_segment(name) {
            Ok(Self(format!("{}/{name}", self.0)))
        } else {
            bad()
        }
    }
    /// The same folder with another last segment.
    pub fn with_file_name(&self, name: &str) -> Result<RelPath> {
        if !portable_segment(name) {
            return bad();
        }
        Ok(match self.0.rsplit_once('/') {
            Some((p, _)) => Self(format!("{p}/{name}")),
            None => Self(name.to_owned()),
        })
    }
}

impl NativePath {
    pub fn parse(text: &str, style: Style) -> Result<Self> {
        match style {
            Style::Posix => Self::parse_posix(text),
            Style::Windows => Self::parse_windows(text),
        }
    }

    fn parse_posix(text: &str) -> Result<Self> {
        let rest = text.strip_prefix('/').ok_or(Error::PathUnresolvable)?;
        let mut segments = Vec::new();
        let mut at = 1;
        for s in rest.split('/') {
            if !portable_segment(s) {
                return bad();
            }
            segments.push(at..at + s.len());
            at += s.len() + 1;
        }
        Ok(Self { text: text.to_owned(), style: Style::Posix, prefix: Prefix::PosixRoot, segments })
    }

    fn parse_windows(text: &str) -> Result<Self> {
        let b = text.as_bytes();
        let at = |i: usize| b.get(i).copied();
        // (prefix, index just after the prefix and its own separator, if any)
        let (prefix, mut pos) = if at(0).is_some_and(is_sep) && at(1).is_some_and(is_sep) {
            if at(2) == Some(b'?') && at(3).is_some_and(is_sep) {
                // Verbatim: only drive and UNC forms; GLOBALROOT, Volume{..} and so on are refused.
                let rest = text.get(4..).ok_or(Error::PathUnresolvable)?;
                if rest.len() >= 4
                    && rest.is_char_boundary(3)
                    && rest[..3].eq_ignore_ascii_case("UNC")
                    && is_sep(rest.as_bytes()[3])
                {
                    let (server, share, end) = Self::unc_units(text, 8)?;
                    (Prefix::VerbatimUnc { server, share }, end)
                } else if let Some((letter, end)) = Self::drive_at(text, 4) {
                    (Prefix::VerbatimDrive(letter), end)
                } else {
                    return bad();
                }
            } else if at(2) == Some(b'.') && at(3).is_some_and(is_sep) {
                return bad(); // device namespace
            } else {
                let (server, share, end) = Self::unc_units(text, 2)?;
                (Prefix::Unc { server, share }, end)
            }
        } else if let Some((letter, end)) = Self::drive_at(text, 0) {
            (Prefix::Drive(letter), end)
        } else {
            return bad(); // `C:x`, `\x`, relative
        };
        let mut segments = Vec::new();
        if pos < text.len() {
            for s in text[pos..].split(['\\', '/']) {
                if !windows_segment(s) {
                    return bad();
                }
                segments.push(pos..pos + s.len());
                pos += s.len() + 1;
            }
        }
        Ok(Self { text: text.to_owned(), style: Style::Windows, prefix, segments })
    }

    /// `X:` followed by a separator at `from`; returns the letter and the index after the separator.
    fn drive_at(text: &str, from: usize) -> Option<(u8, usize)> {
        let b = text.as_bytes();
        (b.get(from)?.is_ascii_alphabetic() && *b.get(from + 1)? == b':' && is_sep(*b.get(from + 2)?))
            .then(|| (b[from], from + 3))
    }

    /// `server` and `share` starting at `from`; returns both ranges and the index after the
    /// separator that follows the share (or the end of the text).
    fn unc_units(text: &str, from: usize) -> Result<(Range<usize>, Range<usize>, usize)> {
        let tail = text.get(from..).ok_or(Error::PathUnresolvable)?;
        let mut parts = tail.splitn(3, ['\\', '/']);
        let server = parts.next().unwrap_or("");
        let share = parts.next().ok_or(Error::PathUnresolvable)?;
        if !windows_unit(server) || !windows_unit(share) {
            return bad();
        }
        let s = from..from + server.len();
        let h = s.end + 1..s.end + 1 + share.len();
        let end = if h.end < text.len() { h.end + 1 } else { h.end };
        Ok((s, h, end))
    }

    /// The text exactly as it was given.
    pub fn as_str(&self) -> &str {
        &self.text
    }
    pub fn style(&self) -> Style {
        self.style
    }
    pub fn prefix(&self) -> &Prefix {
        &self.prefix
    }
    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.segments.iter().map(|r| &self.text[r.clone()])
    }
    pub fn file_name(&self) -> Option<&str> {
        self.segments.last().map(|r| &self.text[r.clone()])
    }

    /// Whether two paths name the same place by name (not by file identity).
    pub fn same_place(&self, other: &NativePath) -> bool {
        self.key() == other.key()
    }

    fn head_key(&self) -> String {
        match &self.prefix {
            Prefix::PosixRoot => "P:".to_owned(),
            Prefix::Drive(l) | Prefix::VerbatimDrive(l) => {
                format!("D:{}", l.to_ascii_uppercase() as char)
            }
            Prefix::Unc { server, share } | Prefix::VerbatimUnc { server, share } => {
                format!("U:{}/{}", &self.text[server.clone()], &self.text[share.clone()])
            }
        }
    }

    pub fn key(&self) -> PathKey {
        let mut key = self.head_key();
        for s in self.segments() {
            key.push('/');
            key.push_str(s);
        }
        PathKey(key)
    }

    /// Whether `base` is this path or one of its ancestors (compared by key, segment by segment).
    pub fn starts_with(&self, base: &NativePath) -> bool {
        self.style == base.style
            && self.head_key() == base.head_key()
            && self.segments.len() >= base.segments.len()
            && self.segments().zip(base.segments()).all(|(a, b)| a == b)
    }

    /// The part below `root`. Fails when `root` is not a proper ancestor (the same place gives an
    /// error, as `FsContext::locate` does today).
    pub fn relative_to(&self, root: &NativePath) -> Result<RelPath> {
        if !self.starts_with(root) || self.segments.len() == root.segments.len() {
            return bad();
        }
        let rest: Vec<&str> = self.segments().skip(root.segments.len()).collect();
        Ok(RelPath(rest.join("/")))
    }

    fn separator(&self) -> char {
        match self.style {
            Style::Posix => '/',
            Style::Windows => '\\',
        }
    }

    /// This path with `rel` appended. The root text stays untouched; every added segment is
    /// preceded by the separator of the root's style, except when the text already ends with one.
    /// Fails only for Windows paths when a segment is not valid there (`a:b`, trailing dot, ...).
    pub fn join(&self, rel: &RelPath) -> Result<NativePath> {
        let mut text = self.text.clone();
        for s in rel.segments() {
            if !text.ends_with(['/', '\\']) {
                text.push(self.separator());
            }
            text.push_str(s);
        }
        Self::parse(&text, self.style)
    }

    fn name_ok(&self, name: &str) -> bool {
        match self.style {
            Style::Posix => portable_segment(name),
            Style::Windows => windows_segment(name),
        }
    }

    /// The folder part of the path. `None` when the path has no segment, and for a single Posix
    /// segment, whose parent would be the bare `/` that is not a valid path here.
    pub fn parent(&self) -> Option<NativePath> {
        let last = self.segments.last()?;
        let end = if self.segments.len() > 1 {
            last.start - 1
        } else if self.style == Style::Posix {
            return None;
        } else {
            last.start
        };
        Self::parse(&self.text[..end], self.style).ok()
    }

    /// The same folder with another last segment (one segment, no separator inside).
    pub fn with_file_name(&self, name: &str) -> Result<NativePath> {
        let last = self.segments.last().ok_or(Error::PathUnresolvable)?;
        if !self.name_ok(name) {
            return bad();
        }
        Self::parse(&format!("{}{name}", &self.text[..last.start]), self.style)
    }

    /// This path with one more segment (one segment, no separator inside).
    pub fn child(&self, name: &str) -> Result<NativePath> {
        if !self.name_ok(name) {
            return bad();
        }
        self.join(&RelPath(name.to_owned()))
    }

    /// Replace the leading `from` by `to`, keeping the part below it. The same place as `from`
    /// becomes `to`.
    pub fn rebase(&self, from: &NativePath, to: &NativePath) -> Result<NativePath> {
        if self.same_place(from) {
            return Ok(to.clone());
        }
        to.join(&self.relative_to(from)?)
    }
}
