//! Decision table for the crash recovery of a journal entry. Pure logic, no I/O.
/// Observable facts, evaluated in priority order. Phase markers can lag the filesystem.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryDecision {
    Discard,
    Finish,
    FinishUnlink,
    FinishTemporary,
    RollbackTemporary,
    /// The model is still at its source but some companions already moved: put them back.
    RollbackCompanions,
    Committed,
    Manual,
}
#[derive(Clone, Copy, Debug)]
pub struct RecoveryFacts {
    pub trusted: bool,
    pub db_before: bool,
    pub db_after: bool,
    pub source_matches: bool,
    pub source_absent: bool,
    pub target_matches: bool,
    pub temporary_matches: bool,
    pub unlink: bool,
    pub db_only: bool,
    /// The model's companion rows equal the journal's picture from before the operation.
    pub companions_db_before: bool,
    /// Every row the operation rewrites already has its new state.
    pub companions_db_after: bool,
    /// Every companion that travels on disk is still at its source (true when none travels).
    pub companions_all_source: bool,
    /// Every companion that travels on disk is at its target (true when none travels).
    pub companions_all_target: bool,
    /// Each companion is at its source or its target, but not all at the same side.
    pub companions_mixed: bool,
    /// At least one companion travels on disk.
    pub companions_any: bool,
}
pub const RECOVERY_TABLE: &[(RecoveryDecision, &str)] = &[
    (
        RecoveryDecision::Manual,
        "untrusted identity/state or unresolved combination",
    ),
    (
        RecoveryDecision::Committed,
        "DB after, matching destination (or absent unlinked source)",
    ),
    (
        RecoveryDecision::Discard,
        "DB before and intact source; foreign target is harmless",
    ),
    (
        RecoveryDecision::Finish,
        "DB before, source absent, matching destination",
    ),
    (
        RecoveryDecision::FinishTemporary,
        "matching temporary and unchanged DB",
    ),
    (
        RecoveryDecision::RollbackTemporary,
        "matching temporary and changed DB",
    ),
    (
        RecoveryDecision::FinishUnlink,
        "DB before and unlink source absent",
    ),
    (
        RecoveryDecision::RollbackCompanions,
        "DB before, model at its source, companions partly or fully at their targets",
    ),
    (
        RecoveryDecision::FinishUnlink,
        "delete with companions: DB before, some file already unlinked; the rest follows",
    ),
];
pub fn recovery_decision(f: RecoveryFacts) -> RecoveryDecision {
    use RecoveryDecision::*;
    if !f.trusted {
        return Manual;
    }
    if f.db_after
        && f.companions_db_after
        && (f.db_only
            || (f.unlink && f.source_absent)
            || (!f.unlink && f.source_absent && f.target_matches && f.companions_all_target))
    {
        return Committed;
    }
    if f.temporary_matches && f.source_absent {
        if f.db_before {
            // The model only reaches its temporary name after every companion moved.
            return if f.companions_db_before && f.companions_all_target {
                FinishTemporary
            } else {
                Manual
            };
        }
        // Moving only the model back would strand companions at the target.
        return if f.companions_any { Manual } else { RollbackTemporary };
    }
    if !f.db_before {
        return Manual;
    }
    if f.db_only {
        return Discard;
    }
    if f.unlink && f.companions_any {
        // A companion that is already unlinked cannot come back, so a delete never
        // goes backwards once the first file is gone.
        if !f.companions_db_before || !(f.companions_all_source || f.companions_all_target || f.companions_mixed) {
            return Manual;
        }
        if f.source_matches && f.companions_all_source {
            return Discard;
        }
        return if f.source_matches || f.source_absent { FinishUnlink } else { Manual };
    }
    if f.source_matches && !f.target_matches {
        if f.companions_all_source {
            return Discard;
        }
        return if f.companions_mixed || f.companions_all_target {
            RollbackCompanions
        } else {
            Manual
        };
    }
    if f.unlink && f.source_absent {
        return FinishUnlink;
    }
    if f.source_absent && f.target_matches {
        return if f.companions_all_target && f.companions_db_before {
            Finish
        } else {
            Manual
        };
    }
    Manual
}
