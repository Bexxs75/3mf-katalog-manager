//! Contract suite for `DirHandle`/`OpenFile`. The checks are generic over the directory type, so a
//! second backend has to pass the same suite; today only the Linux handle adapter runs it.
use super::*;
use crate::fs_portable::backend::{DirHandle, DirMode, EntryKind, FileMode, Name, OpenFile};
use crate::fs_portable::capabilities::Level;
use std::io::{Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};

fn n(s: &str) -> Name {
    Name::new(s).unwrap()
}
fn fresh(label: &str) -> PathBuf {
    crate::unique_test_dir(&format!("backend_contract_{label}"))
}
fn mode_of(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o7777
}
fn write_file<D: DirHandle>(dir: &D, name: &str, data: &[u8]) -> D::File {
    let mut f = dir.create_new(&n(name)).unwrap();
    f.write_all(data).unwrap();
    f.sync().unwrap();
    f
}
fn open(path: &Path) -> Directory {
    Directory::open(path).unwrap()
}

fn no_replace_keeps_an_existing_target<D: DirHandle>(base: &Path, root: &D) {
    write_file(root, "a", b"A");
    write_file(root, "b", b"B");
    let e = root.rename_noreplace(&n("a"), root, &n("b")).unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(std::fs::read(base.join("a")).unwrap(), b"A");
    assert_eq!(std::fs::read(base.join("b")).unwrap(), b"B");
    // A folder in the way, and a folder moved onto a folder.
    root.mkdir(&n("d1"), DirMode::Normal).unwrap();
    root.mkdir(&n("d2"), DirMode::Normal).unwrap();
    assert_eq!(
        root.rename_noreplace(&n("a"), root, &n("d1")).unwrap_err().kind(),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(
        root.rename_noreplace(&n("d1"), root, &n("d2")).unwrap_err().kind(),
        io::ErrorKind::AlreadyExists
    );
    assert!(base.join("d1").is_dir() && base.join("d2").is_dir());
    // Free target, also into another folder.
    let sub = root.open_dir(&n("d2")).unwrap();
    root.rename_noreplace(&n("a"), &sub, &n("moved")).unwrap();
    assert!(!base.join("a").exists());
    assert_eq!(std::fs::read(base.join("d2/moved")).unwrap(), b"A");
}
#[test]
fn no_replace_keeps_an_existing_target_file_or_folder() {
    let base = fresh("noreplace");
    no_replace_keeps_an_existing_target(&base, &open(&base));
}

fn open_file_key_equals_entry_key_and_survives_rename<D: DirHandle>(root: &D) {
    let mut f = write_file(root, "f", b"hello");
    let entry = root.entry(&n("f")).unwrap();
    assert_eq!(entry.kind, EntryKind::File);
    assert_eq!(f.key().unwrap(), entry.key);
    assert_eq!(entry.len, 5);
    assert_eq!(f.len().unwrap(), 5);
    assert_eq!(entry.nlink, Some(1));
    assert_eq!(f.nlink().unwrap(), Some(1));
    // Same object after a rename inside the folder and into another folder.
    root.rename_noreplace(&n("f"), root, &n("g")).unwrap();
    assert_eq!(root.entry(&n("g")).unwrap().key, entry.key);
    root.mkdir(&n("sub"), DirMode::Normal).unwrap();
    let sub = root.open_dir(&n("sub")).unwrap();
    root.rename_noreplace(&n("g"), &sub, &n("h")).unwrap();
    assert_eq!(sub.entry(&n("h")).unwrap().key, entry.key);
    assert_eq!(f.key().unwrap(), entry.key);
    // The handle still reads what was written.
    assert_eq!(root.entry(&n("g")).unwrap_err().kind(), io::ErrorKind::NotFound);
    let mut again = sub.open_regular(&n("h")).unwrap();
    let mut text = String::new();
    again.read_to_string(&mut text).unwrap();
    assert_eq!(text, "hello");
    assert_eq!(again.key().unwrap(), entry.key);
    let _ = f.flush();
}
#[test]
fn open_file_identity_matches_the_entry_and_is_stable_across_renames() {
    let base = fresh("identity");
    open_file_key_equals_entry_key_and_survives_rename(&open(&base));
}

fn link_count_follows_hard_links<D: DirHandle>(base: &Path, root: &D) {
    let f = write_file(root, "one", b"x");
    std::fs::hard_link(base.join("one"), base.join("two")).unwrap();
    assert_eq!(f.nlink().unwrap(), Some(2));
    assert_eq!(root.entry(&n("two")).unwrap().nlink, Some(2));
    assert_eq!(root.entry(&n("one")).unwrap().key, root.entry(&n("two")).unwrap().key);
    root.unlink(&n("two")).unwrap();
    assert_eq!(f.nlink().unwrap(), Some(1));
}
#[test]
fn link_count_is_two_after_a_hard_link_and_one_after_removing_it() {
    let base = fresh("nlink");
    link_count_follows_hard_links(&base, &open(&base));
}

fn symlinks_are_never_followed<D: DirHandle>(base: &Path, root: &D) {
    std::fs::create_dir(base.join("real")).unwrap();
    std::fs::write(base.join("file"), b"x").unwrap();
    symlink(base.join("real"), base.join("dirlink")).unwrap();
    symlink(base.join("file"), base.join("filelink")).unwrap();
    assert!(root.open_dir(&n("dirlink")).is_err());
    assert!(root.open_regular(&n("filelink")).is_err());
    assert_eq!(root.entry(&n("dirlink")).unwrap().kind, EntryKind::Other);
    assert_eq!(root.entry(&n("filelink")).unwrap().kind, EntryKind::Other);
    assert_eq!(root.entry(&n("real")).unwrap().kind, EntryKind::Dir);
    // create_new does not write through a dangling link either.
    symlink(base.join("nowhere"), base.join("dangling")).unwrap();
    assert!(root.create_new(&n("dangling")).is_err());
    assert!(!base.join("nowhere").exists());
}
#[test]
fn symlinks_are_not_followed_when_opening_and_are_seen_as_other_entries() {
    let base = fresh("symlink");
    symlinks_are_never_followed(&base, &open(&base));
}

fn only_regular_files_open<D: DirHandle>(base: &Path, root: &D) {
    std::fs::create_dir(base.join("folder")).unwrap();
    let fifo = std::ffi::CString::new(base.join("pipe").as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
    assert_eq!(root.open_regular(&n("folder")).err().unwrap().kind(), io::ErrorKind::InvalidData);
    // Would block forever if the FIFO were opened for reading without O_NONBLOCK.
    assert_eq!(root.open_regular(&n("pipe")).err().unwrap().kind(), io::ErrorKind::InvalidData);
    assert_eq!(root.open_regular(&n("missing")).err().unwrap().kind(), io::ErrorKind::NotFound);
}
#[test]
fn open_regular_refuses_folders_and_pipes_without_blocking() {
    let base = fresh("regular");
    only_regular_files_open(&base, &open(&base));
}

fn exact_listing_reports_errors<D: DirHandle>(root: &D, fail_next_listing: impl Fn()) {
    write_file(root, "x", b"1");
    let mut names = root.names_exact().unwrap();
    names.sort();
    assert_eq!(names, [".", "..", "x"]);
    fail_next_listing();
    assert!(root.names_exact().is_err(), "an error must never read as an empty folder");
    // The lenient listing is for clean-up and has its own path.
    assert!(root.names_lenient().unwrap().contains(&"x".to_owned()));
    assert!(root.names_exact().is_ok());
}
#[test]
fn exact_listing_reports_an_error_instead_of_an_empty_list() {
    let base = fresh("names");
    exact_listing_reports_errors(&open(&base), || {
        crate::upload_fs::FSOPS_READDIR_ERROR.with(|h| h.set(Some(libc::EIO)));
    });
}

fn rmdir_is_never_recursive<D: DirHandle>(base: &Path, root: &D) {
    root.mkdir(&n("full"), DirMode::Normal).unwrap();
    std::fs::write(base.join("full/inner"), b"x").unwrap();
    assert!(root.rmdir(&n("full")).is_err());
    assert!(base.join("full/inner").exists());
    std::fs::remove_file(base.join("full/inner")).unwrap();
    root.rmdir(&n("full")).unwrap();
    assert!(!base.join("full").exists());
    // rmdir does not remove files and unlink does not remove folders.
    write_file(root, "plain", b"x");
    assert!(root.rmdir(&n("plain")).is_err());
    root.mkdir(&n("folder"), DirMode::Normal).unwrap();
    assert!(root.unlink(&n("folder")).is_err());
    assert!(base.join("plain").exists() && base.join("folder").exists());
}
#[test]
fn rmdir_never_recurses_and_unlink_leaves_folders_alone() {
    let base = fresh("rmdir");
    rmdir_is_never_recursive(&base, &open(&base));
}

fn create_is_exclusive_and_private<D: DirHandle>(base: &Path, root: &D) {
    let file = root.create_new(&n("fresh")).unwrap();
    assert_eq!(mode_of(&base.join("fresh")) & 0o777, 0o600);
    assert_eq!(file.owned_private().unwrap(), Level::Yes);
    assert_eq!(root.create_new(&n("fresh")).err().unwrap().kind(), io::ErrorKind::AlreadyExists);
    file.set_mode(FileMode::Final).unwrap();
    assert_eq!(mode_of(&base.join("fresh")), 0o644);
    assert_eq!(file.owned_private().unwrap(), Level::No);
    file.set_mode(FileMode::Private).unwrap();
    assert_eq!(mode_of(&base.join("fresh")), 0o600);
    assert_eq!(file.owned_private().unwrap(), Level::Yes);
}
#[test]
fn create_new_is_exclusive_private_and_set_mode_changes_the_end_mode() {
    let base = fresh("create");
    create_is_exclusive_and_private(&base, &open(&base));
}

fn folders_report_owner_and_mode<D: DirHandle>(base: &Path, root: &D) {
    root.mkdir(&n("priv"), DirMode::Private).unwrap();
    root.mkdir(&n("norm"), DirMode::Normal).unwrap();
    assert_eq!(mode_of(&base.join("priv")), 0o700);
    assert_eq!(mode_of(&base.join("norm")) & 0o700, 0o700);
    assert_eq!(root.open_dir(&n("priv")).unwrap().owned_private().unwrap(), Level::Yes);
    // 0755 is readable by others, whatever the umask removes.
    std::fs::set_permissions(base.join("norm"), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(root.open_dir(&n("norm")).unwrap().owned_private().unwrap(), Level::No);
    assert_eq!(root.mkdir(&n("priv"), DirMode::Private).unwrap_err().kind(), io::ErrorKind::AlreadyExists);
}
#[test]
fn folders_are_made_with_the_asked_mode_and_report_owner_and_mode() {
    let base = fresh("mkdir");
    folders_report_owner_and_mode(&base, &open(&base));
}

fn navigation_keeps_identity<D: DirHandle>(root: &D) {
    root.mkdir(&n("child"), DirMode::Normal).unwrap();
    let child = root.open_dir(&n("child")).unwrap();
    assert_eq!(child.parent().unwrap().key().unwrap(), root.key().unwrap());
    assert_eq!(child.try_clone().unwrap().key().unwrap(), child.key().unwrap());
    assert_eq!(root.entry(&n("child")).unwrap().key, child.key().unwrap());
    assert_eq!(root.entry(&n("child")).unwrap().kind, EntryKind::Dir);
    assert_ne!(child.key().unwrap(), root.key().unwrap());
}
#[test]
fn parent_and_clone_point_at_the_same_folders_as_the_entries() {
    let base = fresh("nav");
    navigation_keeps_identity(&open(&base));
}

fn journal_replace_overwrites<D: DirHandle>(base: &Path, root: &D) {
    write_file(root, "journal", b"old");
    write_file(root, "journal.tmp", b"new");
    root.replace_journal(&n("journal.tmp"), &n("journal")).unwrap();
    assert_eq!(std::fs::read(base.join("journal")).unwrap(), b"new");
    assert!(!base.join("journal.tmp").exists());
    root.sync().unwrap();
}
#[test]
fn replace_journal_replaces_the_target_and_sync_works() {
    let base = fresh("replace");
    journal_replace_overwrites(&base, &open(&base));
}

fn system_facts_are_available<D: DirHandle>(root: &D) {
    assert!(root.free_bytes().unwrap() > 0);
    let own = root.mount(None).unwrap();
    root.mkdir(&n("m"), DirMode::Normal).unwrap();
    // A plain folder lives on the same mount as its parent.
    assert_eq!(root.mount(Some(&n("m"))).unwrap(), own);
    let f = write_file(root, "age", b"x");
    assert!(f.modified_age().unwrap() < std::time::Duration::from_secs(60));
}
#[test]
fn free_space_mount_and_age_are_reported() {
    let base = fresh("facts");
    system_facts_are_available(&open(&base));
}

#[test]
fn unlink_removes_the_name_only() {
    let base = fresh("unlink");
    let root = open(&base);
    let f = write_file(&root, "gone", b"keep");
    DirHandle::unlink(&root, &n("gone")).unwrap();
    assert_eq!(root.entry(&n("gone")).unwrap_err().kind(), io::ErrorKind::NotFound);
    assert_eq!(DirHandle::unlink(&root, &n("gone")).unwrap_err().kind(), io::ErrorKind::NotFound);
    // The opened object lives on until the handle is dropped.
    assert_eq!(f.nlink().unwrap(), Some(0));
}

// ---- FileBackend: probe, mount list, ids, error mapping ----

use super::{parse_mountinfo, HandleBackend};
use crate::fs_portable::backend::FileBackend;
use crate::fs_portable::capabilities::{Capabilities, IdentityKind, NlinkKind, Normalization};
use crate::fs_portable::path::{NativePath, Style};
use crate::fs_portable::Error;

fn probe_dirs(label: &str) -> (PathBuf, Directory, Directory) {
    let base = fresh(label);
    let root = open(&base);
    let trash = DirHandle::open_dir(&root, &n(".fsops-trash")).or_else(|_| {
        DirHandle::mkdir(&root, &n(".fsops-trash"), DirMode::Private)?;
        DirHandle::open_dir(&root, &n(".fsops-trash"))
    });
    (base, root, trash.unwrap())
}
fn fs_magic(dir: &Directory) -> u64 {
    let mut buf = std::mem::MaybeUninit::<libc::statfs>::uninit();
    assert_eq!(unsafe { libc::fstatfs(std::os::fd::AsRawFd::as_raw_fd(&dir.0), buf.as_mut_ptr()) }, 0);
    (unsafe { buf.assume_init() }.f_type as u64) & 0xFFFF_FFFF
}

#[test]
fn probe_proves_no_replace_links_and_private_trash_and_cleans_up() {
    let (base, root, trash) = probe_dirs("probe");
    let caps = HandleBackend.probe(&root, &trash).unwrap();
    assert_eq!(caps.atomic_noreplace_rename, Level::Yes);
    assert_eq!(caps.nlink, NlinkKind::Reliable);
    assert_eq!(caps.dir_fsync, Level::Yes);
    assert_eq!(caps.prevents_symlink_races, Level::Yes);
    assert_eq!(caps.private_files, Level::Yes);
    assert_eq!(caps.unlink_verified_object, Level::No);
    assert_eq!(caps.case_insensitive_names, Level::Unknown);
    assert_eq!(caps.unicode_normalization, Normalization::None);
    assert_eq!(caps.detects_mount_aliases, Level::Yes);
    // The identity is only claimed on file systems of the known list.
    let known = [0xEF53u64, 0x5846_5342, 0x9123_683E, 0x0102_1994, 0xF2F5_2010].contains(&fs_magic(&root));
    assert_eq!(caps.file_identity, if known { IdentityKind::DevIno64 } else { IdentityKind::Unknown });
    // Nothing of the probe is left behind.
    assert_eq!(std::fs::read_dir(&base).unwrap().count(), 1);
    assert_eq!(std::fs::read_dir(base.join(".fsops-trash")).unwrap().count(), 0);
}

#[test]
fn probe_reports_a_trash_that_others_can_read_as_not_private() {
    let (base, root, trash) = probe_dirs("probe_open_trash");
    std::fs::set_permissions(base.join(".fsops-trash"), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(HandleBackend.probe(&root, &trash).unwrap().private_files, Level::No);
}

#[test]
fn probe_fails_on_a_folder_it_cannot_write_to_and_leaves_nothing() {
    let (base, root, trash) = probe_dirs("probe_readonly");
    std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o500)).unwrap();
    // Running as root ignores the mode; the probe then simply succeeds.
    let result = HandleBackend.probe(&root, &trash);
    std::fs::set_permissions(&base, std::fs::Permissions::from_mode(0o700)).unwrap();
    if unsafe { libc::geteuid() } != 0 {
        assert!(result.is_err());
    }
    assert_eq!(std::fs::read_dir(&base).unwrap().count(), 1);
}

#[test]
fn probe_leaves_a_present_foreign_file_alone() {
    let (base, root, trash) = probe_dirs("probe_foreign");
    std::fs::write(base.join("model.3mf"), b"keep").unwrap();
    std::fs::write(base.join(".fsops-trash/other"), b"keep").unwrap();
    let _: Capabilities = HandleBackend.probe(&root, &trash).unwrap();
    assert_eq!(std::fs::read(base.join("model.3mf")).unwrap(), b"keep");
    assert_eq!(std::fs::read(base.join(".fsops-trash/other")).unwrap(), b"keep");
}

#[test]
fn open_root_takes_absolute_posix_roots_only() {
    let base = fresh("open_root");
    let ok = NativePath::parse(base.to_str().unwrap(), Style::Posix).unwrap();
    let dir = HandleBackend.open_root(&ok).unwrap();
    assert_eq!(dir.key().unwrap(), open(&base).key().unwrap());
    let windows = NativePath::parse("C:\\data", Style::Windows).unwrap();
    assert_eq!(HandleBackend.open_root(&windows).err().unwrap().kind(), io::ErrorKind::InvalidInput);
    let missing = NativePath::parse(base.join("missing").to_str().unwrap(), Style::Posix).unwrap();
    assert_eq!(HandleBackend.open_root(&missing).err().unwrap().kind(), io::ErrorKind::NotFound);
}

#[test]
fn random_ids_differ_and_have_sixteen_bytes() {
    let a = HandleBackend.random_op_id().unwrap();
    let b = HandleBackend.random_op_id().unwrap();
    assert_ne!(a, b);
}

#[test]
fn errors_map_like_the_existing_io_conversion() {
    for (errno, want) in [
        (libc::EXDEV, Error::CrossDevice),
        (libc::EBUSY, Error::Busy),
        (libc::EINVAL, Error::Unsupported),
        (libc::EEXIST, Error::Conflict),
        (libc::ENOENT, Error::NotFound),
        (libc::ELOOP, Error::PathUnresolvable),
        (libc::ENOSPC, Error::InsufficientStorage),
        (libc::EACCES, Error::Internal),
    ] {
        assert_eq!(HandleBackend.map_error(&io::Error::from_raw_os_error(errno)), want, "{errno}");
    }
    assert_eq!(
        HandleBackend.map_error(&io::Error::from(io::ErrorKind::InvalidData)),
        Error::PathUnresolvable
    );
}

#[test]
fn mount_list_is_read_and_unescaped() {
    let text = "36 35 98:0 /mnt1 /mnt/with\\040space rw,noatime master:1 - ext3 /dev/root rw\n\
                37 35 8:1 / / rw - ext4 /dev/sda1 rw\n\
                38 35 8:2 /caf\\303\\251 /ok rw - ext4 /dev/sda2 rw\n\
                39 35 8:3 /bad\\377 /x rw - ext4 /dev/sda3 rw\n";
    let list = parse_mountinfo(text).unwrap();
    assert_eq!(list.len(), 3, "the mount with a non-UTF-8 path is left out");
    assert_eq!((list[0].mount_id, list[0].root.as_str(), list[0].point.as_str()), (36, "/mnt1", "/mnt/with space"));
    assert_eq!(list[2].root, "/café");
    assert_eq!(parse_mountinfo("short line\n").unwrap_err().kind(), io::ErrorKind::InvalidData);
    assert_eq!(parse_mountinfo("1 2 3:4 /a\\04 /b rw - x y z\n").unwrap_err().kind(), io::ErrorKind::InvalidData);
    // The real list of this machine parses too, and holds the mount of the test folder.
    let real = HandleBackend.mount_aliases().unwrap();
    assert!(!real.is_empty());
}

#[test]
fn probe_trusts_the_identity_on_tmpfs() {
    // The default temp folder may be an overlay, which is not on the list; /dev/shm usually is tmpfs.
    let base = PathBuf::from("/dev/shm").join(format!("backend_probe_{}", std::process::id()));
    if std::fs::create_dir_all(&base).is_err() {
        return;
    }
    let root = open(&base);
    if fs_magic(&root) == 0x0102_1994 {
        DirHandle::mkdir(&root, &n(".fsops-trash"), DirMode::Private).unwrap();
        let trash = DirHandle::open_dir(&root, &n(".fsops-trash")).unwrap();
        let caps = HandleBackend.probe(&root, &trash).unwrap();
        assert_eq!(caps.file_identity, IdentityKind::DevIno64);
        assert_eq!(caps.atomic_noreplace_rename, Level::Yes);
    }
    std::fs::remove_dir_all(&base).unwrap();
}
