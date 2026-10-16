use super::*;
use std::{
    collections::HashSet,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};
pub const TRASH: &str = ".katalog-papierkorb";
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Identity {
    pub dev: u64,
    pub ino: u64,
}
impl Identity {
    pub fn of(d: &Directory) -> Result<Self> {
        let m = d.metadata()?;
        Ok(Self {
            dev: m.dev(),
            ino: m.ino(),
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protection {
    Upload,
    CatalogBase,
    Forbidden,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Source,
    MoveTarget,
    RenameTarget,
    RestoreTarget,
    Trash,
    Directory,
}
pub struct Root {
    pub path: PathBuf,
    pub handle: Directory,
    pub trash: Directory,
    pub identity: Identity,
    mount: Option<u64>,
}
struct Protected {
    path: PathBuf,
    handle: Directory,
    identity: Identity,
    kind: Protection,
    ancestors: HashSet<Identity>,
}
/// Construct once at startup (B), before accepting requests. Handles remain pinned.
/// Only canonical startup paths are accepted; database paths are never canonicalized.
pub struct FsContext {
    pub roots: Vec<Root>,
    protected: Vec<Protected>,
}
pub struct Resolved {
    pub root: usize,
    pub relative: String,
    pub parent: Directory,
    pub name: String,
}
impl FsContext {
    pub fn new(roots: &[PathBuf], protected: &[(PathBuf, Protection)]) -> Result<Self> {
        let mounts = mount_locations()?;
        let mut protections = Vec::new();
        for (path, kind) in protected {
            let path = std::fs::canonicalize(path)?;
            let dir = Directory::open(&path)?;
            let identity = Identity::of(&dir)?;
            let mut ancestors = HashSet::new();
            let mut current = dir.try_clone()?;
            loop {
                let id = Identity::of(&current)?;
                if !ancestors.insert(id) {
                    break;
                }
                current = current.parent_handle()?;
            }
            protections.push(Protected {
                path,
                handle: dir,
                identity,
                kind: *kind,
                ancestors,
            });
        }
        let mut result: Vec<Root> = Vec::new();
        for path in roots {
            let path = std::fs::canonicalize(path)?;
            lexical(path.to_str().ok_or(Error::PathUnresolvable)?)?;
            let handle = Directory::open(&path)?;
            let identity = Identity::of(&handle)?;
            let chain = ancestor_ids(&handle)?;
            if protections.iter().any(|p| {
                p.kind == Protection::Forbidden
                    && (p.ancestors.contains(&identity) || chain.contains(&p.identity))
            }) || result.iter().any(|r| {
                chain.contains(&r.identity)
                    || ancestor_ids(&r.handle).is_ok_and(|a| a.contains(&identity))
            }) {
                return Err(Error::Protected);
            }
            for p in &protections {
                if p.kind == Protection::Forbidden
                    && mount_alias_overlap(
                        &handle,
                        &path,
                        handle.mount_id()?,
                        &p.handle,
                        &p.path,
                        p.handle.mount_id()?,
                        &mounts,
                    )?
                {
                    return Err(Error::Protected);
                }
            }
            for r in &result {
                if mount_alias_overlap(
                    &handle,
                    &path,
                    handle.mount_id()?,
                    &r.handle,
                    &r.path,
                    r.mount,
                    &mounts,
                )? {
                    return Err(Error::Protected);
                }
            }
            if protections.iter().any(|p| {
                p.kind == Protection::Upload
                    && p.identity != identity
                    && chain.contains(&p.identity)
            }) {
                return Err(Error::Protected);
            }
            let trash = ensure_trash_dir(&handle)?;
            if trash.device()? != handle.device()?
                || different_mount(handle.mount_id()?, trash.mount_id()?)
            {
                return Err(Error::CrossDevice);
            }
            let mount = handle.mount_id()?;
            result.push(Root {
                path,
                handle,
                trash,
                identity,
                mount,
            });
        }
        Ok(Self {
            roots: result,
            protected: protections,
        })
    }
    #[cfg(any(test, feature = "test-support"))]
    pub fn for_test(roots: &[PathBuf], protected: &[(PathBuf, Protection)]) -> Result<Self> {
        Self::new(roots, protected)
    }
    pub fn locate(&self, path: &str) -> Result<(usize, String)> {
        lexical(path)?;
        let p = Path::new(path);
        self.roots
            .iter()
            .enumerate()
            .filter_map(|(i, r)| {
                p.strip_prefix(&r.path)
                    .ok()
                    .filter(|p| !p.as_os_str().is_empty())
                    .map(|rel| (i, rel.to_str().unwrap().to_owned()))
            })
            .max_by_key(|(i, _)| self.roots[*i].path.components().count())
            .ok_or(Error::PathUnresolvable)
    }
    pub fn resolve(&self, path: &str, access: Access) -> Result<Resolved> {
        let (root, relative) = self.locate(path)?;
        self.resolve_relative(root, &relative, access)
    }
    pub fn resolve_relative(
        &self,
        root: usize,
        relative: &str,
        access: Access,
    ) -> Result<Resolved> {
        if relative.starts_with('/') {
            return Err(Error::PathUnresolvable);
        }
        components(relative)?;
        let r = self.roots.get(root).ok_or(Error::PathUnresolvable)?;
        let parts: Vec<_> = relative.split('/').collect();
        let mut parent = r.handle.try_clone()?;
        let mut in_upload = self
            .protected
            .iter()
            .any(|p| p.kind == Protection::Upload && p.identity == r.identity);
        if in_upload && matches!(access, Access::MoveTarget | Access::Directory) {
            return Err(Error::Protected);
        }
        for part in &parts[..parts.len() - 1] {
            if in_upload && access != Access::Trash {
                return Err(Error::Protected);
            }
            parent = parent.open_dir(part)?;
            let id = Identity::of(&parent)?;
            if id.dev != r.identity.dev || different_mount(r.mount, parent.mount_id()?) {
                return Err(Error::CrossDevice);
            }
            if id == Identity::of(&r.trash)? && access != Access::Trash {
                return Err(Error::Protected);
            }
            if self.roots.iter().any(|other| other.identity == id) {
                return Err(Error::Protected);
            }
            for p in &self.protected {
                if id == p.identity {
                    match p.kind {
                        Protection::Forbidden => return Err(Error::Protected),
                        Protection::Upload => {
                            if matches!(access, Access::MoveTarget | Access::Directory) {
                                return Err(Error::Protected);
                            }
                            in_upload = true;
                        }
                        Protection::CatalogBase => (),
                    }
                }
            }
        }
        if access == Access::Trash
            && (parts.len() != 2 || Identity::of(&parent)? != Identity::of(&r.trash)?)
        {
            return Err(Error::TrashForeign);
        }
        let name = parts.last().unwrap().to_string();
        let entry = match parent.entry_identity(&name) {
            Ok(id) => Some(id),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        if let Some((dev, ino)) = entry {
            let id = Identity { dev, ino };
            if self.roots.iter().any(|r| r.identity == id)
                || id == Identity::of(&r.trash)?
                || self.protected.iter().any(|p| {
                    p.identity == id || (access == Access::Directory && p.ancestors.contains(&id))
                })
            {
                return Err(Error::Protected);
            }
        }
        Ok(Resolved {
            root,
            relative: relative.to_owned(),
            parent,
            name,
        })
    }
    pub fn absolute(&self, r: &Resolved) -> String {
        self.roots[r.root]
            .path
            .join(&r.relative)
            .to_str()
            .unwrap()
            .to_owned()
    }
}
fn different_mount(a: Option<u64>, b: Option<u64>) -> bool {
    matches!((a,b), (Some(a),Some(b)) if a != b)
}
fn ancestor_ids(dir: &Directory) -> Result<HashSet<Identity>> {
    let mut d = dir.try_clone()?;
    let mut ids = HashSet::new();
    while ids.insert(Identity::of(&d)?) {
        d = d.parent_handle()?;
    }
    Ok(ids)
}
fn components(path: &str) -> Result<()> {
    if path.contains('\\')
        || path.contains('\0')
        || path.split('/').any(|p| matches!(p, "" | "." | ".."))
    {
        return Err(Error::PathUnresolvable);
    }
    Ok(())
}
fn lexical(path: &str) -> Result<()> {
    components(path.strip_prefix('/').ok_or(Error::PathUnresolvable)?)
}
pub fn ensure_trash_dir(root: &Directory) -> Result<Directory> {
    match root.mkdir(TRASH, 0o700) {
        Ok(()) => root.sync()?,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    }
    let trash = root.open_dir(TRASH)?;
    let m = trash.metadata()?;
    if m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o7777 != 0o700 {
        return Err(Error::Protected);
    }
    Ok(trash)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protected_identity_aliases_and_ancestors_use_handles() {
        let root = crate::unique_test_dir("fsops_protected");
        std::fs::create_dir_all(root.join("parent/upload")).unwrap();
        std::fs::create_dir_all(root.join("catalog/child")).unwrap();
        let mut ctx = FsContext::for_test(
            std::slice::from_ref(&root),
            &[
                (root.join("parent/upload"), Protection::Upload),
                (root.join("catalog"), Protection::CatalogBase),
            ],
        )
        .unwrap();
        for path in ["parent", "parent/upload", "catalog"] {
            assert!(matches!(
                ctx.resolve(root.join(path).to_str().unwrap(), Access::Directory),
                Err(Error::Protected)
            ));
        }
        assert!(ctx
            .resolve(
                root.join("catalog/child/x").to_str().unwrap(),
                Access::MoveTarget
            )
            .is_ok());
        assert!(ctx
            .resolve(
                root.join("parent/upload/x").to_str().unwrap(),
                Access::Source
            )
            .is_ok());
        // Simulate an alias resolving to the forbidden handle identity, not its startup path.
        ctx.protected[0].kind = Protection::Forbidden;
        assert!(matches!(
            ctx.resolve(
                root.join("parent/upload/x").to_str().unwrap(),
                Access::Source
            ),
            Err(Error::Protected)
        ));
        ctx.roots[0].mount = Some(u64::MAX);
        assert!(matches!(
            ctx.resolve(root.join("catalog/x").to_str().unwrap(), Access::Source),
            Err(Error::CrossDevice)
        ));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn overlapping_roots_non_utf8_and_symlink_trash_are_rejected() {
        use std::os::unix::{ffi::OsStringExt, fs::symlink};
        let root = crate::unique_test_dir("fsops_startup");
        std::fs::create_dir(root.join("sub")).unwrap();
        assert!(FsContext::new(&[root.clone(), root.join("sub")], &[]).is_err());
        let bad = root.join(std::ffi::OsString::from_vec(vec![0xff]));
        std::fs::create_dir(&bad).unwrap();
        assert!(FsContext::new(&[bad], &[]).is_err());
        std::fs::remove_dir(root.join(TRASH)).unwrap();
        symlink(root.join("sub"), root.join(TRASH)).unwrap();
        assert!(ensure_trash_dir(&Directory::open(&root).unwrap()).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod mount_overlap_tests {
    use super::*;
    #[test]
    fn bind_descendant_overlap_is_confirmed_by_handle_identity() {
        let base = crate::unique_test_dir("mount_overlap");
        std::fs::create_dir_all(base.join("db/backups")).unwrap();
        let db = Directory::open(&base.join("db")).unwrap();
        let child = Directory::open(&base.join("db/backups")).unwrap();
        // Kernel mount metadata maps a virtual mountpoint to the descendant; handles are real.
        let mounts = vec![
            MountLocation {
                mount_id: 1,
                root: "/".into(),
                point: base.clone(),
            },
            MountLocation {
                mount_id: 2,
                root: "/db/backups".into(),
                point: "/alias".into(),
            },
        ];
        assert!(mount_alias_overlap(
            &db,
            &base.join("db"),
            Some(1),
            &child,
            Path::new("/alias"),
            Some(2),
            &mounts
        )
        .unwrap());
        assert!(mount_alias_overlap(
            &child,
            Path::new("/alias"),
            Some(2),
            &db,
            &base.join("db"),
            Some(1),
            &mounts
        )
        .unwrap());
        std::fs::create_dir(base.join("unrelated")).unwrap();
        let unrelated = Directory::open(&base.join("unrelated")).unwrap();
        assert!(!mount_alias_overlap(
            &unrelated,
            &base.join("unrelated"),
            Some(1),
            &child,
            Path::new("/alias"),
            Some(2),
            &mounts
        )
        .unwrap());
        std::fs::remove_dir_all(base).unwrap();
    }
}

// `..` crosses a bind mount at its destination. Mount metadata supplies candidate
// aliases; the final overlap decision is confirmed by opened (dev,ino), never text.
struct MountLocation {
    mount_id: u64,
    root: PathBuf,
    point: PathBuf,
}
fn mount_locations() -> Result<Vec<MountLocation>> {
    let text = std::fs::read_to_string("/proc/self/mountinfo")?;
    let mut out = Vec::new();
    for line in text.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() < 6 {
            return Err(Error::Unsupported);
        }
        out.push(MountLocation {
            mount_id: fields[0].parse().map_err(|_| Error::Unsupported)?,
            root: mount_path(fields[3])?,
            point: mount_path(fields[4])?,
        });
    }
    Ok(out)
}
fn mount_path(s: &str) -> Result<PathBuf> {
    let mut bytes = Vec::new();
    let mut input = s.as_bytes().iter().copied();
    while let Some(b) = input.next() {
        if b == b'\\' {
            let mut n = 0u16;
            for _ in 0..3 {
                let digit = input
                    .next()
                    .filter(|b| (b'0'..=b'7').contains(b))
                    .ok_or(Error::Unsupported)?;
                n = n * 8 + u16::from(digit - b'0');
            }
            bytes.push(u8::try_from(n).map_err(|_| Error::Unsupported)?);
        } else {
            bytes.push(b);
        }
    }
    use std::os::unix::ffi::OsStringExt;
    Ok(std::ffi::OsString::from_vec(bytes).into())
}
fn mount_physical(path: &Path, id: Option<u64>, mounts: &[MountLocation]) -> Result<PathBuf> {
    let m = mounts
        .iter()
        .filter(|m| id.map_or_else(|| path.starts_with(&m.point), |id| id == m.mount_id))
        .max_by_key(|m| m.point.components().count())
        .ok_or(Error::Unsupported)?;
    Ok(m.root.join(
        path.strip_prefix(&m.point)
            .map_err(|_| Error::PathUnresolvable)?,
    ))
}
fn mount_alias_overlap(
    a: &Directory,
    pa: &Path,
    ma: Option<u64>,
    b: &Directory,
    pb: &Path,
    mb: Option<u64>,
    mounts: &[MountLocation],
) -> Result<bool> {
    if a.device()? != b.device()? {
        return Ok(false);
    }
    let ap = mount_physical(pa, ma, mounts)?;
    let bp = mount_physical(pb, mb, mounts)?;
    let (ancestor, descendant, relative) = if let Ok(rel) = bp.strip_prefix(&ap) {
        (a, b, rel)
    } else if let Ok(rel) = ap.strip_prefix(&bp) {
        (b, a, rel)
    } else {
        return Ok(false);
    };
    let mut dir = ancestor.try_clone()?;
    for part in relative.components() {
        let std::path::Component::Normal(part) = part else {
            return Err(Error::PathUnresolvable);
        };
        dir = dir.open_dir(part.to_str().ok_or(Error::PathUnresolvable)?)?;
    }
    Ok(Identity::of(&dir)? == Identity::of(descendant)?)
}

impl FsContext {
    /// Verify the current handle ancestry, including protection and mount boundaries.
    pub(super) fn directory_chain(
        &self,
        root: usize,
        dir: &Directory,
        cycle: Option<Identity>,
    ) -> Result<()> {
        let r = &self.roots[root];
        let mut current = dir.try_clone()?;
        for _ in 0..1024 {
            let id = Identity::of(&current)?;
            if cycle == Some(id) {
                return Err(Error::Cycle);
            }
            if id.dev != r.identity.dev || different_mount(r.mount, current.mount_id()?) {
                return Err(Error::CrossDevice);
            }
            if id == Identity::of(&r.trash)?
                || self
                    .protected
                    .iter()
                    .any(|p| p.identity == id && p.kind != Protection::CatalogBase)
            {
                return Err(Error::Protected);
            }
            if id == r.identity {
                return Ok(());
            }
            let next = current.parent_handle()?;
            if Identity::of(&next)? == id {
                return Err(Error::Changed);
            }
            current = next;
        }
        Err(Error::PathUnresolvable)
    }
    pub(super) fn directory_identity(&self, r: &Resolved) -> Result<Identity> {
        let dir = r.parent.open_dir(&r.name)?;
        let id = Identity::of(&dir)?;
        self.directory_chain(r.root, &dir, None)?;
        if r.parent.entry_identity(&r.name)? != (id.dev, id.ino) {
            return Err(Error::Changed);
        }
        // Recompute protected ancestors: protected directories may have moved since startup.
        for p in &self.protected {
            if ancestor_ids(&p.handle)?.contains(&id) {
                return Err(Error::Protected);
            }
        }
        if self.roots.iter().any(|root| root.identity == id) {
            return Err(Error::Protected);
        }
        Ok(id)
    }
    pub(super) fn protects_catalog(&self, source: Identity, base: &str) -> Result<()> {
        let mut path = Path::new(base);
        let dir = loop {
            if let Some(root) = self.roots.iter().find(|r| r.path == path) {
                break root.handle.try_clone()?;
            }
            // Only external settings may be canonicalized. Inside a managed root,
            // retain NOFOLLOW, mount and protection checks, failing closed except
            // for missing components, whose nearest existing ancestor is protected.
            let opened = if self.roots.iter().any(|r| path.starts_with(&r.path)) {
                self.resolve(
                    &format!(
                        "{}/catalog-guard",
                        path.to_str().ok_or(Error::PathUnresolvable)?
                    ),
                    Access::Source,
                )
                .map(|child| child.parent)
            } else {
                std::fs::canonicalize(path)
                    .and_then(|canonical| Directory::open(&canonical))
                    .map_err(Error::from)
            };
            match opened {
                Ok(dir) => break dir,
                Err(Error::NotFound) => {
                    path = path.parent().ok_or(Error::PathUnresolvable)?;
                }
                Err(e) => return Err(e),
            }
        };
        if ancestor_ids(&dir)?.contains(&source) {
            return Err(Error::Protected);
        }
        Ok(())
    }
}

#[cfg(test)]
mod removal_mount_tests {
    use super::*;

    #[test]
    fn a2fix_db_removal_ignores_cross_device_observation() {
        struct Ledger;
        impl UploadLedger for Ledger {
            fn detach(&mut self, _: &[i64]) -> Result<()> {
                Ok(())
            }
            fn reset_imported(&mut self, _: &[i64]) -> Result<()> {
                Ok(())
            }
        }
        let dir = crate::unique_test_dir("removal_mount");
        std::fs::create_dir_all(dir.join("root/A/child")).unwrap();
        std::fs::create_dir(dir.join("journal")).unwrap();
        let mut context = FsContext::new(&[dir.join("root")], &[]).unwrap();
        // Exercise the real mount comparison without requiring mount privileges.
        context.roots[0].mount = Some(u64::MAX);
        let db = crate::db::connect_in_memory().unwrap();
        let a = crate::db::insert_folder_with_parent(
            &db,
            "A",
            None,
            dir.join("root/A").to_str().unwrap(),
        )
        .unwrap();
        crate::db::insert_folder_with_parent(
            &db,
            "child",
            Some(a),
            dir.join("root/A/child").to_str().unwrap(),
        )
        .unwrap();
        let mut ops = FileOps::new(
            context,
            Journal::open(&dir.join("journal")).unwrap(),
            db,
            Ledger,
        );
        assert_eq!(ops.remove_folder_from_catalog(a).unwrap().folder_count, 2);
        assert!(dir.join("root/A/child").is_dir());
        assert!(ops.journal.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
