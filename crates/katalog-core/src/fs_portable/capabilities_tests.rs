//! Tests of the decision function. The capability space is finite, so the properties are checked
//! over every combination instead of samples.
use super::*;
use Group::*;

const LEVELS: [Level; 3] = [Level::Yes, Level::No, Level::Unknown];
const IDENTITIES: [IdentityKind; 5] = [
    IdentityKind::DevIno64,
    IdentityKind::VolumeIndex64,
    IdentityKind::VolumeId128,
    IdentityKind::Unstable,
    IdentityKind::Unknown,
];
const NLINKS: [NlinkKind; 3] = [NlinkKind::Reliable, NlinkKind::NoHardlinks, NlinkKind::Unreliable];
const NORMS: [Normalization; 4] = [
    Normalization::None,
    Normalization::NormalizationInsensitive,
    Normalization::StoresNfd,
    Normalization::Unknown,
];

fn all_capabilities() -> Vec<Capabilities> {
    let mut out = Vec::new();
    for &prevents_symlink_races in &LEVELS {
        for &atomic_noreplace_rename in &LEVELS {
            for &dir_fsync in &LEVELS {
                for &case_insensitive_names in &LEVELS {
                    for &detects_mount_aliases in &LEVELS {
                        for &private_files in &LEVELS {
                            for &unlink_verified_object in &LEVELS {
                                for &file_identity in &IDENTITIES {
                                    for &nlink in &NLINKS {
                                        for &unicode_normalization in &NORMS {
                                            out.push(Capabilities {
                                                prevents_symlink_races,
                                                atomic_noreplace_rename,
                                                dir_fsync,
                                                file_identity,
                                                nlink,
                                                case_insensitive_names,
                                                unicode_normalization,
                                                detects_mount_aliases,
                                                private_files,
                                                unlink_verified_object,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

/// Capabilities at which nothing is weak, so every group is `Full`.
fn strong() -> Capabilities {
    Capabilities {
        prevents_symlink_races: Level::Yes,
        atomic_noreplace_rename: Level::Yes,
        dir_fsync: Level::Yes,
        file_identity: IdentityKind::VolumeId128,
        nlink: NlinkKind::Reliable,
        case_insensitive_names: Level::No,
        unicode_normalization: Normalization::None,
        detects_mount_aliases: Level::Yes,
        private_files: Level::Yes,
        unlink_verified_object: Level::Yes,
    }
}

/// What the Linux server is expected to report (the casefold check is not part of the probe yet).
fn linux_server() -> Capabilities {
    Capabilities {
        file_identity: IdentityKind::DevIno64,
        case_insensitive_names: Level::Unknown,
        unlink_verified_object: Level::No,
        ..strong()
    }
}

fn verdicts(c: &Capabilities, k1b: bool) -> Vec<Verdict> {
    let d = decide(c, k1b);
    Group::ALL.iter().map(|g| d.get(*g)).collect()
}
fn only(c: &Capabilities, expect: &[(Group, Verdict)]) {
    let d = decide(c, false);
    for g in Group::ALL {
        let want = expect.iter().find(|(x, _)| *x == g).map_or(Verdict::Full, |(_, v)| *v);
        assert_eq!(d.get(g), want, "{g:?} for {c:?}");
    }
}

#[test]
fn verdicts_are_ordered_mildest_first() {
    assert!(Verdict::Full < Verdict::Alternative);
    assert!(Verdict::Alternative < Verdict::Restricted);
    assert!(Verdict::Restricted < Verdict::Refuse);
}

#[test]
fn strong_capabilities_allow_everything_fully() {
    only(&strong(), &[]);
    assert_eq!(decide(&strong(), false).worst(), Verdict::Full);
}

#[test]
fn every_stage_zero_failure_refuses_all_groups_whatever_the_rest_says() {
    let mut seen = 0;
    for c in all_capabilities() {
        let stage0 = c.atomic_noreplace_rename != Level::Yes
            || matches!(c.file_identity, IdentityKind::Unstable | IdentityKind::Unknown)
            || c.nlink == NlinkKind::Unreliable
            || c.private_files != Level::Yes;
        if stage0 {
            seen += 1;
            assert!(verdicts(&c, false).iter().all(|v| *v == Verdict::Refuse), "{c:?}");
        } else {
            assert!(verdicts(&c, false).iter().all(|v| *v < Verdict::Refuse), "{c:?}");
        }
    }
    assert!(seen > 0);
}

#[test]
fn a_private_journal_place_lifts_only_the_private_files_precondition() {
    let mut c = strong();
    c.private_files = Level::No;
    assert!(verdicts(&c, false).iter().all(|v| *v == Verdict::Refuse));
    only_with_place(&c, &[(Journal, Verdict::Alternative), (Recovery, Verdict::Alternative)]);
    c.private_files = Level::Unknown;
    only_with_place(&c, &[(Journal, Verdict::Alternative), (Recovery, Verdict::Alternative)]);
    // The place does not rescue the other preconditions.
    c.atomic_noreplace_rename = Level::Unknown;
    assert!(verdicts(&c, true).iter().all(|v| *v == Verdict::Refuse));
}
fn only_with_place(c: &Capabilities, expect: &[(Group, Verdict)]) {
    let d = decide(c, true);
    // Journal and recovery pass their verdict on to every group.
    let base = expect.iter().map(|(_, v)| *v).max().unwrap_or(Verdict::Full);
    for g in Group::ALL {
        let own = expect.iter().find(|(x, _)| *x == g).map_or(Verdict::Full, |(_, v)| *v);
        assert_eq!(d.get(g), own.max(base), "{g:?}");
    }
}

// One case per row of the table; every other group stays Full unless the dependency lifts it.
#[test]
fn volume_index_64_restricts_every_group() {
    let c = Capabilities { file_identity: IdentityKind::VolumeIndex64, ..strong() };
    assert!(verdicts(&c, false).iter().all(|v| *v == Verdict::Restricted));
}
#[test]
fn no_hardlinks_allows_with_an_alternative_and_never_refuses() {
    let c = Capabilities { nlink: NlinkKind::NoHardlinks, ..strong() };
    // Journal and recovery are lifted, so the dependency lifts all groups.
    assert!(verdicts(&c, false).iter().all(|v| *v == Verdict::Alternative));
}
#[test]
fn missing_directory_sync_restricts_every_group() {
    for level in [Level::No, Level::Unknown] {
        let c = Capabilities { dir_fsync: level, ..strong() };
        assert!(verdicts(&c, false).iter().all(|v| *v == Verdict::Restricted));
    }
}
#[test]
fn handles_that_do_not_pin_ancestors_restrict_the_file_groups() {
    for level in [Level::No, Level::Unknown] {
        let c = Capabilities { prevents_symlink_races: level, ..strong() };
        // Recovery is restricted and lifts the five groups; the journal itself only needs an
        // alternative place.
        only(
            &c,
            &[
                (Rename, Verdict::Restricted),
                (Folder, Verdict::Restricted),
                (Delete, Verdict::Restricted),
                (CompanionAdd, Verdict::Restricted),
                (FolderRemove, Verdict::Restricted),
                (Recovery, Verdict::Restricted),
                (Journal, Verdict::Alternative),
            ],
        );
    }
}
#[test]
fn deleting_by_name_restricts_delete_and_recovery_and_through_recovery_all() {
    for level in [Level::No, Level::Unknown] {
        let c = Capabilities { unlink_verified_object: level, ..strong() };
        let all_but_journal: Vec<_> = Group::ALL
            .iter()
            .filter(|g| **g != Journal)
            .map(|g| (*g, Verdict::Restricted))
            .collect();
        only(&c, &all_but_journal);
    }
}
#[test]
fn case_insensitive_or_unknown_names_need_an_alternative() {
    for level in [Level::Yes, Level::Unknown] {
        let c = Capabilities { case_insensitive_names: level, ..strong() };
        // Journal stays Full; recovery lifts the dependency to Alternative for the five groups.
        only(
            &c,
            &[
                (Rename, Verdict::Alternative),
                (Folder, Verdict::Alternative),
                (Delete, Verdict::Alternative),
                (CompanionAdd, Verdict::Alternative),
                (FolderRemove, Verdict::Alternative),
                (Recovery, Verdict::Alternative),
            ],
        );
    }
}
#[test]
fn unicode_normalizing_systems_restrict_all_but_the_journal_and_folder_removal_directly() {
    for norm in [Normalization::NormalizationInsensitive, Normalization::StoresNfd, Normalization::Unknown] {
        let c = Capabilities { unicode_normalization: norm, ..strong() };
        // Recovery is restricted, so the dependency lifts every group except the journal itself.
        only(
            &c,
            &[
                (Rename, Verdict::Restricted),
                (Folder, Verdict::Restricted),
                (Delete, Verdict::Restricted),
                (CompanionAdd, Verdict::Restricted),
                (FolderRemove, Verdict::Restricted),
                (Recovery, Verdict::Restricted),
            ],
        );
    }
}
#[test]
fn undetected_mount_aliases_need_an_alternative_but_not_for_journal_or_recovery() {
    for level in [Level::No, Level::Unknown] {
        let c = Capabilities { detects_mount_aliases: level, ..strong() };
        only(
            &c,
            &[
                (Rename, Verdict::Alternative),
                (Folder, Verdict::Alternative),
                (Delete, Verdict::Alternative),
                (CompanionAdd, Verdict::Alternative),
                (FolderRemove, Verdict::Alternative),
            ],
        );
    }
}

#[test]
fn property_no_group_is_milder_than_journal_or_recovery() {
    for c in all_capabilities() {
        for k1b in [false, true] {
            let d = decide(&c, k1b);
            let base = d.get(Journal).max(d.get(Recovery));
            for g in [Rename, Folder, Delete, CompanionAdd, FolderRemove] {
                assert!(d.get(g) >= base, "{g:?} for {c:?}");
            }
        }
    }
}

// Rank of each field: 0 is the strongest answer, larger is weaker. Unknown ranks like the weak one.
fn level_rank(l: Level) -> u8 {
    u8::from(l != Level::Yes)
}
fn inverse_level_rank(l: Level) -> u8 {
    u8::from(l != Level::No)
}
fn identity_rank(k: IdentityKind) -> u8 {
    match k {
        IdentityKind::DevIno64 | IdentityKind::VolumeId128 => 0,
        IdentityKind::VolumeIndex64 => 1,
        IdentityKind::Unstable | IdentityKind::Unknown => 2,
    }
}
fn nlink_rank(k: NlinkKind) -> u8 {
    match k {
        NlinkKind::Reliable => 0,
        NlinkKind::NoHardlinks => 1,
        NlinkKind::Unreliable => 2,
    }
}
fn norm_rank(n: Normalization) -> u8 {
    u8::from(n != Normalization::None)
}

#[test]
fn property_a_weaker_capability_never_gives_a_milder_verdict() {
    let all = all_capabilities();
    let mut steps = 0u64;
    for c in &all {
        for k1b in [false, true] {
            let base = verdicts(c, k1b);
            let mut weaker: Vec<(Capabilities, bool)> = Vec::new();
            for &l in &LEVELS {
                let mut w = *c;
                w.prevents_symlink_races = l;
                if level_rank(l) > level_rank(c.prevents_symlink_races) { weaker.push((w, k1b)); }
                let mut w = *c;
                w.atomic_noreplace_rename = l;
                if level_rank(l) > level_rank(c.atomic_noreplace_rename) { weaker.push((w, k1b)); }
                let mut w = *c;
                w.dir_fsync = l;
                if level_rank(l) > level_rank(c.dir_fsync) { weaker.push((w, k1b)); }
                let mut w = *c;
                w.detects_mount_aliases = l;
                if level_rank(l) > level_rank(c.detects_mount_aliases) { weaker.push((w, k1b)); }
                let mut w = *c;
                w.private_files = l;
                if level_rank(l) > level_rank(c.private_files) { weaker.push((w, k1b)); }
                let mut w = *c;
                w.unlink_verified_object = l;
                if level_rank(l) > level_rank(c.unlink_verified_object) { weaker.push((w, k1b)); }
                let mut w = *c;
                w.case_insensitive_names = l;
                if inverse_level_rank(l) > inverse_level_rank(c.case_insensitive_names) { weaker.push((w, k1b)); }
            }
            for &k in &IDENTITIES {
                let mut w = *c;
                w.file_identity = k;
                if identity_rank(k) > identity_rank(c.file_identity) { weaker.push((w, k1b)); }
            }
            for &k in &NLINKS {
                let mut w = *c;
                w.nlink = k;
                if nlink_rank(k) > nlink_rank(c.nlink) { weaker.push((w, k1b)); }
            }
            for &n in &NORMS {
                let mut w = *c;
                w.unicode_normalization = n;
                if norm_rank(n) > norm_rank(c.unicode_normalization) { weaker.push((w, k1b)); }
            }
            if k1b {
                weaker.push((*c, false));
            }
            for (w, wk) in weaker {
                steps += 1;
                let after = verdicts(&w, wk);
                for (g, (b, a)) in Group::ALL.iter().zip(base.iter().zip(after.iter())) {
                    assert!(a >= b, "{g:?}: {c:?} (k1b {k1b}) -> {w:?} (k1b {wk}): {b:?} -> {a:?}");
                }
            }
        }
    }
    assert!(steps > 1_000_000, "{steps}");
}

#[test]
fn property_expected_linux_server_values_are_restricted_but_never_refused() {
    let d = decide(&linux_server(), false);
    for g in Group::ALL {
        assert!(d.get(g) <= Verdict::Restricted, "{g:?}");
    }
    // Deleting by name (no handle-based delete) lifts delete and recovery, and through recovery
    // the other groups; the journal itself stays untouched.
    for g in Group::ALL {
        let want = if g == Journal { Verdict::Full } else { Verdict::Restricted };
        assert_eq!(d.get(g), want, "{g:?}");
    }
    // With the casefold check answered No the verdict stays the same.
    let known = Capabilities { case_insensitive_names: Level::No, ..linux_server() };
    assert_eq!(decide(&known, false), d);
}

#[test]
fn file_key_compares_by_both_fields() {
    let a = FileKey { volume: 1, index: 2 };
    assert_eq!(a, FileKey { volume: 1, index: 2 });
    assert_ne!(a, FileKey { volume: 2, index: 2 });
    assert_ne!(a, FileKey { volume: 1, index: 3 });
}
