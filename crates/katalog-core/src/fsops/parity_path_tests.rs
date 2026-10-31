//! Parity evidence for the path checks of `context.rs` and `folders.rs`, recorded on
//! commit b49dbc1b; golden files may only be regenerated in step S8.
//!
//! 1. Accept/reject tables: input to result (error variant included) for `components`,
//!    `lexical`, `valid_path`, `locate`, `resolve_relative` and `FsContext::new`.
//! 2. A reference copy of `components`, `lexical` and `valid_path` as they are today, and a
//!    differential test over every string of a small alphabet up to length 6. After the
//!    parsers are replaced, the same test compares old against new.
use super::*;
use crate::fsops::parity_golden_tests::assert_golden;
use std::{fs, os::unix::fs::symlink};

// ---- Reference copies (the code of commit b49dbc1b, not to be edited) ----

fn components_reference(path: &str) -> Result<()> {
    if path.contains('\\')
        || path.contains('\0')
        || path.split('/').any(|p| matches!(p, "" | "." | ".."))
    {
        return Err(Error::PathUnresolvable);
    }
    Ok(())
}
fn lexical_reference(path: &str) -> Result<()> {
    components_reference(path.strip_prefix('/').ok_or(Error::PathUnresolvable)?)
}
fn valid_path_reference(path: &str) -> Result<&std::path::Path> {
    if !path.starts_with('/')
        || path[1..].split('/').any(|p| matches!(p, "" | "." | ".."))
        || path.contains(['\\', '\0'])
    {
        return Err(Error::Inconsistent);
    }
    Ok(std::path::Path::new(path))
}

// ---- Tables ----

const INPUTS: &[&str] = &[
    "", "/", "//", "a", "/a", "a/", "/a/", "a/b", "/a/b", "/a//b", "/a/./b", "/a/../b", "/a/.", "/a/..",
    ".", "..", "./a", "../a", "/.", "/..", "/a\\b", "\\a", "a\\", "/a\0b", "\0", "/\0", "/ a", "/a ", "/a/ ",
    "/a.", "/.a", "/a/b.", "/é", "/e\u{301}", "/a b/c", "/...", "/a/b/c/d/e/f", "C:\\a", "/C:/a", "\\\\?\\C:\\a",
    "//server/share", "/a/b/", "/ /", "/a/ /b", "/%2e%2e/a", "/a%2fb",
];

fn show<T: std::fmt::Debug>(r: &Result<T>) -> String {
    match r {
        Ok(v) => format!("Ok {v:?}"),
        Err(e) => format!("Err {e}"),
    }
}

#[test]
fn lexical_checks_accept_and_reject_as_the_golden_table() {
    let mut text = String::new();
    for input in INPUTS {
        text += &format!(
            "{input:?}: components {} | lexical {} | valid_path {}\n",
            show(&components(input)),
            show(&lexical(input)),
            show(&super::super::folders::valid_path_for_parity_tests(input)),
        );
    }
    assert_golden("paths/lexical_table.txt", text.as_bytes());
}

fn fnv(hash: &mut u64, bytes: &[u8]) {
    for b in bytes {
        *hash = (*hash ^ u64::from(*b)).wrapping_mul(0x100000001b3);
    }
}

/// Every string over the alphabet up to length 6, shortest first.
fn all_inputs() -> impl Iterator<Item = String> {
    const ALPHABET: [char; 7] = ['/', '\\', '.', ' ', 'a', '\0', 'é'];
    let mut length = 0usize;
    let mut counter = 0u64;
    std::iter::from_fn(move || {
        if counter >= (ALPHABET.len() as u64).pow(length as u32) {
            length += 1;
            counter = 0;
            if length > 6 {
                return None;
            }
        }
        let mut rest = counter;
        counter += 1;
        let mut s = String::new();
        for _ in 0..length {
            s.push(ALPHABET[(rest % ALPHABET.len() as u64) as usize]);
            rest /= ALPHABET.len() as u64;
        }
        Some(s)
    })
}

/// Compares the reference copies with the code in use, for every generated input.
/// Returns how many inputs there were and a digest of all the verdicts.
fn differential() -> (u64, [u64; 3], u64) {
    let (mut count, mut accepted, mut hash) = (0u64, [0u64; 3], 0xcbf29ce484222325u64);
    for input in all_inputs() {
        count += 1;
        let current = [
            components(&input).is_ok(),
            lexical(&input).is_ok(),
            super::super::folders::valid_path_for_parity_tests(&input).is_ok(),
        ];
        assert_eq!(components(&input), components_reference(&input), "components {input:?}");
        assert_eq!(lexical(&input), lexical_reference(&input), "lexical {input:?}");
        assert_eq!(
            super::super::folders::valid_path_for_parity_tests(&input),
            valid_path_reference(&input).map(|_| ()),
            "valid_path {input:?}"
        );
        for (i, ok) in current.iter().enumerate() {
            accepted[i] += u64::from(*ok);
        }
        fnv(&mut hash, input.as_bytes());
        fnv(&mut hash, &[0xff, current[0] as u8, current[1] as u8, current[2] as u8]);
    }
    (count, accepted, hash)
}

#[test]
fn the_code_in_use_equals_the_reference_copy_for_every_generated_input() {
    let (count, accepted, hash) = differential();
    assert_eq!(count, (0..=6).map(|k| 7u64.pow(k)).sum::<u64>());
    let text = format!(
        "inputs {count}\naccepted components {} lexical {} valid_path {}\nfnv1a64 {hash:016x}\n",
        accepted[0], accepted[1], accepted[2]
    );
    assert_golden("paths/differential_digest.txt", text.as_bytes());
}

struct World {
    dir: std::path::PathBuf,
    root: std::path::PathBuf,
    other: std::path::PathBuf,
}
impl Drop for World {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}
fn world() -> World {
    let dir = crate::unique_test_dir("fsops_parity_paths");
    let root = dir.join("models");
    let other = dir.join("other");
    for d in ["models/sub/deep", "models/upload", "models/a b", "other"] {
        fs::create_dir_all(dir.join(d)).unwrap();
    }
    fs::write(dir.join("models/file.stl"), b"x").unwrap();
    fs::write(dir.join("models/sub/f.stl"), b"x").unwrap();
    symlink("/tmp", dir.join("models/link")).unwrap();
    symlink("sub", dir.join("models/sublink")).unwrap();
    World { dir, root, other }
}
fn mask(text: &str, w: &World) -> String {
    let mut text = text.to_owned();
    for d in [fs::canonicalize(&w.dir).ok(), Some(w.dir.clone())].into_iter().flatten() {
        text = text.replace(d.to_str().unwrap(), "<DIR>");
    }
    text
}

#[test]
fn locate_and_resolve_relative_accept_and_reject_as_the_golden_table() {
    let w = world();
    let ctx = FsContext::new(
        &[w.root.clone(), w.other.clone()],
        &[(w.root.join("upload"), Protection::Upload)],
    )
    .unwrap();
    let mut text = String::new();
    let r = w.root.to_str().unwrap();
    let o = w.other.to_str().unwrap();
    let located = [
        format!("{r}/sub/f.stl"), format!("{r}/file.stl"), format!("{r}/"), r.to_owned(), format!("{r}x/f"),
        format!("{r}/sub/../file.stl"), format!("{r}//file.stl"), format!("{r}/./file.stl"), format!("{r}/a\\b"),
        format!("{o}/z"), o.to_owned(), "/".into(), "/nonexistent/x".into(), "relative/x".into(), String::new(),
        format!("{r}/upload/x"), format!("{r}/link/x"), format!("{r}/a b/x"), format!("{r}/é"),
    ];
    for input in &located {
        text += &mask(&format!("locate {input:?}: {}\n", show(&ctx.locate(input))), &w);
    }
    let relatives = [
        "file.stl", "sub/f.stl", "sub", "sub/deep", "sub/deep/x", "missing", "missing/x", "", "/file.stl", "sub//f.stl",
        "./file.stl", "sub/../file.stl", "sub/.", "a b", "a b/x", "link", "link/x", "sublink", "sublink/f.stl", "upload",
        "upload/x", "upload/sub/x", "sub\\f", "sub/\0", "file.stl/x", ".katalog-papierkorb", ".katalog-papierkorb/x",
    ];
    for access in [Access::Source, Access::MoveTarget, Access::RenameTarget, Access::RestoreTarget, Access::Trash, Access::Directory] {
        for root in [0usize, 1, 2] {
            for rel in relatives {
                let got = ctx.resolve_relative(root, rel, access).map(|x| format!("root={} relative={:?} name={:?}", x.root, x.relative, x.name));
                text += &mask(&format!("resolve_relative root={root} {access:?} {rel:?}: {}\n", show(&got)), &w);
            }
        }
    }
    assert_golden("paths/locate_resolve_relative_table.txt", text.as_bytes());
}

#[test]
fn fs_context_new_accepts_and_rejects_as_the_golden_table() {
    let w = world();
    fs::create_dir(w.dir.join("back\\slash")).unwrap();
    fs::create_dir(w.dir.join("dot.")).unwrap();
    let d = |s: &str| w.dir.join(s);
    type Case<'a> = (&'a str, Vec<std::path::PathBuf>, Vec<(std::path::PathBuf, Protection)>);
    let cases: Vec<Case> = vec![
        ("one root", vec![d("models")], vec![]),
        ("two roots", vec![d("models"), d("other")], vec![]),
        ("no roots", vec![], vec![]),
        ("same root twice", vec![d("models"), d("models")], vec![]),
        ("nested roots", vec![d("models"), d("models/sub")], vec![]),
        ("nested roots reversed", vec![d("models/sub"), d("models")], vec![]),
        ("missing root", vec![d("nope")], vec![]),
        ("root is a file", vec![d("models/file.stl")], vec![]),
        ("root through symlink", vec![d("models/sublink")], vec![]),
        ("root with backslash", vec![d("back\\slash")], vec![]),
        ("root with trailing dot", vec![d("dot.")], vec![]),
        ("root with dot segments", vec![d("models/sub/../sub")], vec![]),
        ("upload folder inside root", vec![d("models")], vec![(d("models/upload"), Protection::Upload)]),
        ("forbidden folder inside root", vec![d("models")], vec![(d("models/sub"), Protection::Forbidden)]),
        ("forbidden folder is the root", vec![d("models")], vec![(d("models"), Protection::Forbidden)]),
        ("forbidden folder above root", vec![d("models/sub")], vec![(d("models"), Protection::Forbidden)]),
        ("upload folder above root", vec![d("models/sub")], vec![(d("models"), Protection::Upload)]),
        ("catalog base above root", vec![d("models/sub")], vec![(d("models"), Protection::CatalogBase)]),
        ("missing protected folder", vec![d("models")], vec![(d("nope"), Protection::Upload)]),
        ("protected folder through symlink", vec![d("models")], vec![(d("models/sublink"), Protection::Forbidden)]),
    ];
    let mut text = String::new();
    for (label, roots, protected) in cases {
        let result = FsContext::new(&roots, &protected).map(|c| c.roots.len());
        text += &mask(&format!("{label}: {}\n", show(&result)), &w);
    }
    assert_golden("paths/fs_context_new_table.txt", text.as_bytes());
}
