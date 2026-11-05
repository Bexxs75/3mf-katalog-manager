//! What a file backend promises for one root (`Capabilities`) and the pure decision which operation
//! groups may run on top of that (`decide`). No I/O: every host can test it. In this stage nothing
//! applies the verdicts yet; `FsContext` keeps its current behaviour and the server keeps its own
//! start-up probe.

/// A yes/no fact that may also be unknown. `Unknown` counts like the weaker answer in `decide`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Level {
    Yes,
    No,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IdentityKind {
    DevIno64,
    VolumeIndex64,
    VolumeId128,
    Unstable,
    Unknown,
}

/// `NoHardlinks`: the file system has no hard links, so a link count of 1 is true (e.g. FAT).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NlinkKind {
    Reliable,
    NoHardlinks,
    Unreliable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Normalization {
    None,
    NormalizationInsensitive,
    StoresNfd,
    Unknown,
}

/// Identity of a file system object: Linux `(st_dev, st_ino)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FileKey {
    pub volume: u64,
    pub index: u128,
}

/// Established at run time for one root, also under Linux (the probe proves, nothing is assumed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capabilities {
    pub prevents_symlink_races: Level,
    pub atomic_noreplace_rename: Level,
    pub dir_fsync: Level,
    pub file_identity: IdentityKind,
    pub nlink: NlinkKind,
    pub case_insensitive_names: Level,
    pub unicode_normalization: Normalization,
    pub detects_mount_aliases: Level,
    pub private_files: Level,
    pub unlink_verified_object: Level,
}

/// Verdict for an operation group, mildest first. The highest verdict that applies wins.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Verdict {
    /// Full promise.
    Full,
    /// Allowed with a named alternative safeguard.
    Alternative,
    /// Allowed with a weaker, documented promise.
    Restricted,
    /// Refused (`Error::Unsupported`).
    Refuse,
}

/// Operation groups of the design: G1 to G5 plus the journal and the recovery, which every other
/// group depends on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Group {
    /// G1: rename, move, trash, restore, companion into/out of the trash.
    Rename,
    /// G2: create, rename, move a folder.
    Folder,
    /// G3: delete for good, empty the trash, purge a companion.
    Delete,
    /// G4: add a companion file.
    CompanionAdd,
    /// G5: remove a folder from the catalog (database only).
    FolderRemove,
    /// J: write the journal.
    Journal,
    /// R: crash recovery.
    Recovery,
}
impl Group {
    pub const ALL: [Group; 7] = [
        Group::Rename,
        Group::Folder,
        Group::Delete,
        Group::CompanionAdd,
        Group::FolderRemove,
        Group::Journal,
        Group::Recovery,
    ];
    fn index(self) -> usize {
        Self::ALL.iter().position(|g| *g == self).unwrap()
    }
}

/// One verdict per group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decisions([Verdict; 7]);
impl Decisions {
    pub fn get(&self, group: Group) -> Verdict {
        self.0[group.index()]
    }
    pub fn worst(&self) -> Verdict {
        self.0.iter().copied().max().unwrap_or(Verdict::Full)
    }
    fn raise(&mut self, groups: &[Group], to: Verdict) {
        for g in groups {
            let slot = &mut self.0[g.index()];
            *slot = (*slot).max(to);
        }
    }
}

/// Which operation groups may run with these capabilities. `private_journal_place` tells whether
/// the journal lives in a place that is private by other means (decided with the journal format,
/// not yet in this stage).
pub fn decide(c: &Capabilities, private_journal_place: bool) -> Decisions {
    use Group::*;
    // Stage 0: preconditions of the whole root.
    if c.atomic_noreplace_rename != Level::Yes
        || matches!(c.file_identity, IdentityKind::Unstable | IdentityKind::Unknown)
        || c.nlink == NlinkKind::Unreliable
        || (c.private_files != Level::Yes && !private_journal_place)
    {
        return Decisions([Verdict::Refuse; 7]);
    }
    // Stage 1: one row per weak capability; `Unknown` counts as weak.
    let mut u = Decisions([Verdict::Full; 7]);
    if c.file_identity == IdentityKind::VolumeIndex64 {
        u.raise(&Group::ALL, Verdict::Restricted);
    }
    if c.nlink == NlinkKind::NoHardlinks {
        u.raise(&[Rename, Delete, CompanionAdd, Journal, Recovery], Verdict::Alternative);
    }
    if c.dir_fsync != Level::Yes {
        u.raise(&Group::ALL, Verdict::Restricted);
    }
    if c.prevents_symlink_races != Level::Yes {
        u.raise(&[Rename, Delete, CompanionAdd, FolderRemove, Recovery], Verdict::Restricted);
        u.raise(&[Folder, Journal], Verdict::Alternative);
    }
    if c.unlink_verified_object != Level::Yes {
        u.raise(&[Delete, Recovery], Verdict::Restricted);
    }
    if c.case_insensitive_names != Level::No {
        u.raise(&[Rename, Folder, CompanionAdd, Recovery], Verdict::Alternative);
    }
    if c.unicode_normalization != Normalization::None {
        u.raise(&[Rename, Folder, Delete, CompanionAdd, Recovery], Verdict::Restricted);
    }
    if c.detects_mount_aliases != Level::Yes {
        u.raise(&[Rename, Folder, Delete, CompanionAdd, FolderRemove], Verdict::Alternative);
    }
    if c.private_files != Level::Yes {
        u.raise(&[Journal, Recovery], Verdict::Alternative);
    }
    // Dependency: every group inherits the verdict of the journal and the recovery.
    let base = u.get(Journal).max(u.get(Recovery));
    u.raise(&[Rename, Folder, Delete, CompanionAdd, FolderRemove], base);
    u
}

#[cfg(test)]
#[path = "capabilities_tests.rs"]
mod capabilities_tests;
