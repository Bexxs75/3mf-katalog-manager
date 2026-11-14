//! Tests of the path model on every host: Windows text is parsed as text, so Linux runs them too.
use super::path::{NativePath, Prefix, RelPath, Style};
use super::Error;

fn win(t: &str) -> NativePath {
    NativePath::parse(t, Style::Windows).unwrap_or_else(|e| panic!("{t:?}: {e}"))
}
fn posix(t: &str) -> NativePath {
    NativePath::parse(t, Style::Posix).unwrap_or_else(|e| panic!("{t:?}: {e}"))
}
fn rel(t: &str) -> RelPath {
    RelPath::parse(t).unwrap()
}

#[test]
fn posix_accepts_what_the_current_checks_accept() {
    for ok in ["/a", "/a/b", "/a b/c.d", "/é/x", "/.hidden/x", "/a../b"] {
        assert_eq!(posix(ok).as_str(), ok);
    }
    for bad in ["", "/", "a", "a/b", "//a", "/a//b", "/a/", "/./a", "/a/..", "/a\\b", "/a\0b", "/a/./b"] {
        assert_eq!(NativePath::parse(bad, Style::Posix), Err(Error::PathUnresolvable), "{bad:?}");
    }
    let p = posix("/m/models/a");
    assert_eq!(p.segments().collect::<Vec<_>>(), ["m", "models", "a"]);
    assert_eq!(p.file_name(), Some("a"));
}

#[test]
fn windows_prefixes_are_classified() {
    assert_eq!(win(r"C:\Data").prefix(), &Prefix::Drive(b'C'));
    assert_eq!(win("c:/Data").prefix(), &Prefix::Drive(b'c'));
    assert_eq!(win(r"\\?\C:\Data").prefix(), &Prefix::VerbatimDrive(b'C'));
    let unc = win(r"\\nas\share\x");
    let Prefix::Unc { server, share } = unc.prefix().clone() else { panic!("not unc") };
    assert_eq!((&unc.as_str()[server], &unc.as_str()[share]), ("nas", "share"));
    let v = win(r"\\?\UNC\nas\share\x");
    let Prefix::VerbatimUnc { server, share } = v.prefix().clone() else { panic!("not verbatim unc") };
    assert_eq!((&v.as_str()[server], &v.as_str()[share]), ("nas", "share"));
    assert!(matches!(win(r"\\?\unc\nas\share").prefix(), Prefix::VerbatimUnc { .. }));
    assert_eq!(win(r"\\nas\share\x").segments().collect::<Vec<_>>(), ["x"]);
    assert_eq!(win(r"C:\").segment_count(), 0);
    assert_eq!(win(r"\\nas\share").segment_count(), 0);
    assert_eq!(win(r"\\nas\share\").segment_count(), 0);
}

#[test]
fn windows_rejects_unsafe_or_unclear_paths() {
    for bad in [
        "", "C:", "C:x", r"C:data\x", r"\x", "x", r"\\", r"\\nas", r"\\nas\", r"\\\x\y", r"\\.\C:\x", r"\\.\x",
        r"\\?\GLOBALROOT\Device\x", r"\\?\Volume{1}\x", r"\\?\C:x", r"\\?\UNC\nas", r"\\?\",
        r"C:\a\\b", r"C:\a\", r"C:\a\.\b", r"C:\a\..\b", r"C:\a \b", r"C:\a.\b", r"C:\a. \b",
        r"C:\a\b ", r"C:\a:b", r"C:\a\b:stream", "C:\\a\0b", "C:\\a\u{1}b", r"C:\a*b", r"C:\a?b",
        r"\\nas\share\..\x", r"\\nas\share\.. \x", r"\\nas:1\share\x", r"\\..\share\x",
    ] {
        assert_eq!(NativePath::parse(bad, Style::Windows), Err(Error::PathUnresolvable), "{bad:?}");
    }
}

#[test]
fn text_is_kept_exactly_and_equality_uses_the_key() {
    let a = win(r"\\?\c:\Data\Models");
    assert_eq!(a.as_str(), r"\\?\c:\Data\Models");
    let b = win("C:/Data\\Models");
    assert!(a.same_place(&b));
    assert!(win(r"\\?\UNC\nas\share\x").same_place(&win(r"\\nas\share/x")));
    assert!(!win(r"C:\Data").same_place(&win(r"C:\data")), "segments are compared exactly");
    assert!(!win(r"\\NAS\share").same_place(&win(r"\\nas\share")), "server names are compared exactly");
    assert!(!win(r"C:\a").same_place(&win(r"D:\a")));
    assert!(!win(r"C:\a").same_place(&win(r"\\c\a")));
    assert!(!posix("/a").same_place(&win(r"C:\a")));
    assert!(posix("/a/b").same_place(&posix("/a/b")));
    assert!(!posix("/a/b").same_place(&posix("/a/B")));
}

#[test]
fn starts_with_is_segmentwise() {
    assert!(posix("/m/a/b").starts_with(&posix("/m/a")));
    assert!(posix("/m/a").starts_with(&posix("/m/a")));
    assert!(!posix("/m/ab").starts_with(&posix("/m/a")));
    assert!(!posix("/m").starts_with(&posix("/m/a")));
    assert!(win(r"c:\Data\x").starts_with(&win(r"\\?\C:\Data")));
    assert!(!win(r"C:\Data\x").starts_with(&win(r"C:\data")));
    assert!(!win(r"D:\Data\x").starts_with(&win(r"C:\Data")));
    assert!(win(r"C:\x").starts_with(&win(r"C:\")));
    assert!(!posix("/a").starts_with(&win(r"C:\a")));
}

#[test]
fn relative_to_and_join_round_trip_textually() {
    let cases = [
        (posix("/m/models"), "a/b.3mf"),
        (win(r"C:\Data"), "a/b.3mf"),
        (win(r"C:/Data"), "x"),
        (win(r"\\?\C:\Data"), "a/b"),
        (win(r"\\nas\share"), "a/b"),
        (win(r"\\?\UNC\nas\share\root"), "a"),
        (win(r"C:\"), "a/b"),
    ];
    for (root, r) in cases {
        let p = rel(r);
        let full = root.join(&p).unwrap();
        assert_eq!(full.relative_to(&root).unwrap(), p, "{root:?}");
        assert!(full.as_str().starts_with(root.as_str()), "root text stays untouched");
    }
    assert_eq!(posix("/m/models").join(&rel("a/b")).unwrap().as_str(), "/m/models/a/b");
    assert_eq!(win(r"C:\Data").join(&rel("a/b")).unwrap().as_str(), r"C:\Data\a\b");
    assert_eq!(win("C:/Data").join(&rel("a")).unwrap().as_str(), r"C:/Data\a");
    assert_eq!(win(r"C:\").join(&rel("a")).unwrap().as_str(), r"C:\a");
    assert_eq!(win(r"\\nas\share").join(&rel("a")).unwrap().as_str(), r"\\nas\share\a");
    assert_eq!(win(r"\\?\C:\Data").join(&rel("a")).unwrap().as_str(), r"\\?\C:\Data\a");
}

#[test]
fn relative_to_refuses_what_is_not_below() {
    let root = posix("/m/a");
    assert_eq!(posix("/m/a").relative_to(&root), Err(Error::PathUnresolvable), "the root itself");
    assert_eq!(posix("/m/ab/x").relative_to(&root), Err(Error::PathUnresolvable));
    assert_eq!(posix("/m").relative_to(&root), Err(Error::PathUnresolvable));
    assert_eq!(win(r"C:\a\x").relative_to(&win(r"D:\a")), Err(Error::PathUnresolvable));
    assert_eq!(win(r"C:\A\x").relative_to(&win(r"C:\a")), Err(Error::PathUnresolvable));
    assert_eq!(posix("/a/b").relative_to(&win(r"C:\a")), Err(Error::PathUnresolvable));
    assert_eq!(win(r"\\?\c:\Data\x\y").relative_to(&win(r"C:\Data")).unwrap().as_str(), "x/y");
}

#[test]
fn windows_join_refuses_segments_that_are_invalid_there() {
    let root = win(r"C:\Data");
    for name in ["a:b", "a.", "a ", "a*", "CON:"] {
        assert_eq!(root.join(&rel(name)), Err(Error::PathUnresolvable), "{name:?}");
    }
    assert!(posix("/m").join(&rel("a:b")).is_ok());
    assert!(root.join(&rel("CON")).is_ok(), "device names are the name rules' job, not the path model's");
}

#[test]
fn rebase_keeps_the_part_below() {
    let (from, to) = (posix("/m/a"), posix("/m/z"));
    assert_eq!(posix("/m/a/b/c").rebase(&from, &to).unwrap().as_str(), "/m/z/b/c");
    assert_eq!(posix("/m/a").rebase(&from, &to).unwrap().as_str(), "/m/z");
    assert_eq!(posix("/m/ab").rebase(&from, &to), Err(Error::PathUnresolvable));
    assert_eq!(win(r"c:\Data\x").rebase(&win(r"C:\Data"), &win(r"D:\New")).unwrap().as_str(), r"D:\New\x");
}

#[test]
fn rel_path_rules() {
    for bad in ["", "/a", "a/", "a//b", "a/./b", "a/../b", "..", ".", "a\\b", "a\0"] {
        assert_eq!(RelPath::parse(bad), Err(Error::PathUnresolvable), "{bad:?}");
    }
    let p = rel("a/b/c.3mf");
    assert_eq!(p.file_name(), "c.3mf");
    assert_eq!(p.parent().unwrap().as_str(), "a/b");
    assert_eq!(rel("a").parent(), None);
    assert_eq!(p.child("d").unwrap().as_str(), "a/b/c.3mf/d");
    assert_eq!(p.with_file_name("x.3mf").unwrap().as_str(), "a/b/x.3mf");
    assert_eq!(rel("a").with_file_name("b").unwrap().as_str(), "b");
    assert!(p.child("a/b").is_err() && p.child("").is_err() && p.child("..").is_err());
    assert_eq!(p.segments().collect::<Vec<_>>(), ["a", "b", "c.3mf"]);
}

#[test]
fn byte_limits_and_unicode_pass_through_unchanged() {
    let long = "é".repeat(127); // 254 bytes
    assert_eq!(rel(&long).as_str(), long);
    let nfc = "\u{e9}";
    let nfd = "e\u{301}";
    let a = posix(&format!("/m/{nfc}"));
    let b = posix(&format!("/m/{nfd}"));
    assert!(!a.same_place(&b), "no normalization: the text is what the database holds");
}

#[test]
fn parent_file_name_and_child() {
    let p = posix("/m/models/a.3mf");
    assert_eq!(p.parent().unwrap().as_str(), "/m/models");
    assert_eq!(posix("/m").parent(), None, "the bare / is not a path of this model");
    assert_eq!(p.with_file_name("b.3mf").unwrap().as_str(), "/m/models/b.3mf");
    assert_eq!(posix("/m").with_file_name("n").unwrap().as_str(), "/n");
    assert_eq!(p.child("x").unwrap().as_str(), "/m/models/a.3mf/x");
    for bad in ["", ".", "..", "a/b", "a\\b", "a\0"] {
        assert!(p.with_file_name(bad).is_err() && p.child(bad).is_err(), "{bad:?}");
    }
    let w = win(r"C:\Data\x");
    assert_eq!(w.parent().unwrap().as_str(), r"C:\Data");
    assert_eq!(win(r"C:\Data").parent().unwrap().as_str(), r"C:\");
    assert_eq!(win(r"\\nas\share\x").parent().unwrap().as_str(), r"\\nas\share\");
    assert_eq!(win(r"C:\").parent(), None);
    assert_eq!(w.with_file_name("y").unwrap().as_str(), r"C:\Data\y");
    assert_eq!(win("C:/Data/x").with_file_name("y").unwrap().as_str(), "C:/Data/y");
    assert!(w.child("a:b").is_err() && w.with_file_name("a.").is_err());
}
