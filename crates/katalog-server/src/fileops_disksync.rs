//! W2 on the server (Forgejo #39): the disk comparison as plan, start and blocks, and the date
//! prefix for file names. Child of `fileops`, so it shares its runtime state, its error helpers
//! and the rule that one request holds the barrier writer, the import gate and `rt.state`.
//!
//! Safety rules this file carries (spec 2.4 to 2.7):
//! - A plan can only be worked off after a fully successful `disk_sync_apply_start`
//!   (`Plan::started`). The only way into the row loop is [`plan_for_block`].
//! - The whole plan is paid once at the start (`reserve_disk_sync`), so the blocks can never
//!   remove more than was booked.
//! - Removing never moves or deletes a file: only `FileOps::remove_missing_entry` and
//!   `remove_missing_companion` are called, and every row is checked against the disk right
//!   before it is changed (`disk_apply::recheck_*`).
//! - A block stops after the current row when maintenance starts, the session ends or the
//!   12-second block time is over. It never breaks off inside a row.
use super::*;
use katalog_core::date_rename::{self, DateRenameFs, DateRenameItem, DateSource};
use katalog_core::disk_apply::{self, Anchored, CatalogPaths, PlanRow, RowKind, SkipReason};
use katalog_core::fsops::{Access, FsContext, FsDatabase, MissingRemoval};
use std::collections::{BTreeSet, HashMap};
use std::io;
use std::path::{Path, PathBuf};

/// Plans live for an hour, like the trash snapshots.
const PLAN_TTL: u64 = 3600;
const MAX_PLANS: usize = 100;
/// Rows in one preview (3 lists of up to 500 from the report).
const MAX_ROWS: usize = 1500;
const BLOCK_ROWS: usize = 10;
const BLOCK_TIME: Duration = Duration::from_secs(12);
/// Date prefix: entries in one request.
pub(super) const MAX_DATE_ITEMS: usize = 200;

/// What the client chose in the report.
#[derive(Debug)]
pub(super) struct Selection {
    new_files: Vec<(u64, String)>,
    missing: Vec<i64>,
    dangling: Vec<i64>,
}

#[derive(Default)]
struct Applied {
    imported: usize,
    removed_models: usize,
    removed_companions: usize,
    skipped: usize,
}

/// A checked and frozen selection. In memory only; gone after a restart.
pub(super) struct Plan {
    session: String,
    created: u64,
    rows: Vec<PlanRow>,
    /// Rows already worked off.
    cursor: usize,
    /// The next `seq` a block must carry.
    seq: u64,
    last_result: Option<Value>,
    /// Set as the very last step of a fully successful `disk_sync_apply_start`, never earlier.
    started: bool,
    confirmed_removal: bool,
    applied: Applied,
}
impl Plan {
    fn removes(&self) -> usize {
        self.rows.iter().filter(|r| r.kind != RowKind::New).count()
    }
}

fn bad() -> Box<Response> {
    err(400, "invalid_arguments")
}
fn id_text(value: &Value) -> Result<i64, Box<Response>> {
    let s = value.as_str().ok_or_else(bad)?;
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad());
    }
    s.parse::<i64>().ok().filter(|i| *i > 0).ok_or_else(bad)
}
fn list<'a>(v: &'a Value, key: &str) -> Result<&'a [Value], Box<Response>> {
    match v.get(key) {
        None => Ok(&[]),
        Some(Value::Array(a)) => Ok(a),
        Some(_) => Err(bad()),
    }
}

/// The keys were checked by the caller. At most 1500 rows, at least one after removing duplicates.
pub(super) fn parse_selection(v: &Value) -> Result<Selection, Box<Response>> {
    let (new, missing, dangling) = (list(v, "newFiles")?, list(v, "missingModels")?, list(v, "danglingCompanions")?);
    if new.len() + missing.len() + dangling.len() > MAX_ROWS {
        return Err(err(422, "limit_exceeded"));
    }
    let mut new_files = BTreeSet::new();
    for entry in new {
        let o = entry.as_object().ok_or_else(bad)?;
        if o.keys().any(|k| k != "root" && k != "relativePath") {
            return Err(err(422, "unknown_argument"));
        }
        let root = o.get("root").and_then(Value::as_u64).ok_or_else(bad)?;
        let path = o.get("relativePath").and_then(Value::as_str).ok_or_else(bad)?;
        new_files.insert((root, path.to_owned()));
    }
    let ids = |values: &[Value]| -> Result<Vec<i64>, Box<Response>> {
        Ok(values.iter().map(id_text).collect::<Result<BTreeSet<_>, _>>()?.into_iter().collect())
    };
    let selection = Selection { new_files: new_files.into_iter().collect(), missing: ids(missing)?, dangling: ids(dangling)? };
    if selection.new_files.len() + selection.missing.len() + selection.dangling.len() == 0 {
        return Err(err(422, "invalid_arguments"));
    }
    Ok(selection)
}

/// Date prefix: the items of `apply_date_rename`. The keys of the request were checked by the caller.
pub(super) fn parse_date_items(v: &Value) -> Result<Vec<DateRenameItem>, Box<Response>> {
    let items = v.get("items").and_then(Value::as_array).ok_or_else(bad)?;
    if items.is_empty() || items.len() > MAX_DATE_ITEMS {
        return Err(err(422, "limit_exceeded"));
    }
    items
        .iter()
        .map(|item| {
            let o = item.as_object().ok_or_else(bad)?;
            if o.keys().any(|k| k != "id" && k != "newName") {
                return Err(err(422, "unknown_argument"));
            }
            let text = |key: &str| o.get(key).and_then(Value::as_str).map(str::to_owned).ok_or_else(bad);
            Ok(DateRenameItem { id: text("id")?, new_name: text("newName")? })
        })
        .collect()
}

pub(super) fn parse_date_source(v: &Value) -> Result<DateSource, Box<Response>> {
    match v.get("dateSource").and_then(Value::as_str) {
        Some("modified") => Ok(DateSource::Modified),
        Some("imported") => Ok(DateSource::Imported),
        _ => Err(bad()),
    }
}

/// All file access of the comparison, built per request from the configured roots; the upload
/// folder is never read from.
fn anchored(app: &App) -> Result<Anchored, SkipReason> {
    let excluded: Vec<PathBuf> = app.config.upload.iter().map(|u| u.dir.clone()).collect();
    Anchored::new(&app.config.file_roots, &excluded)
}

fn row_label(kind: RowKind) -> &'static str {
    match kind {
        RowKind::New => "new",
        RowKind::MissingModel => "removeModel",
        RowKind::DanglingCompanion => "removeCompanion",
    }
}
fn skipped_entry(kind: &str, key: &str, reason: SkipReason) -> Value {
    json!({"kind": kind, "key": key, "reason": reason.code()})
}

/// `disk_sync_preview`: checks every chosen row without writing and keeps the rest as a plan.
pub(super) fn preview(
    app: &App,
    current: &Session,
    state: &mut RuntimeState,
    selection: Selection,
    now: u64,
) -> Result<Value, Box<Response>> {
    state.plans.retain(|_, p| now.saturating_sub(p.created) < PLAN_TTL);
    let roots = katalog_core::disk_report::ordered_roots(&app.config.file_roots);
    let anchored = anchored(app).map_err(|reason| err(409, reason.code()))?;
    let RuntimeState { ops, plans, .. } = state;
    let (rows, skipped, mostly) = ops
        .db
        .with_conn(|conn| {
            let known = CatalogPaths::load(conn).map_err(|_| fsops::Error::Internal)?;
            let (mut news, mut companions, mut models) = (Vec::new(), Vec::new(), Vec::new());
            let mut skipped = Vec::new();
            for (root, relative) in &selection.new_files {
                let key = format!("{root}:{relative}");
                let planned = match usize::try_from(*root).ok().filter(|r| *r < roots.len()) {
                    None => Err(SkipReason::Outside),
                    Some(root) => disk_apply::plan_new_file(&anchored, &known, root, relative),
                };
                match planned {
                    Ok(row) => news.push(row),
                    Err(reason) => skipped.push(skipped_entry("new", &key, reason)),
                }
            }
            for id in &selection.dangling {
                match disk_apply::plan_dangling_companion(conn, &anchored, *id) {
                    Ok(row) => companions.push(row),
                    Err(reason) => skipped.push(skipped_entry("removeCompanion", &id.to_string(), reason)),
                }
            }
            for id in &selection.missing {
                match disk_apply::plan_missing_model(conn, &anchored, *id) {
                    Ok(row) => models.push(row),
                    Err(reason) => skipped.push(skipped_entry("removeModel", &id.to_string(), reason)),
                }
            }
            let mostly = mostly_missing(conn, &roots, &models);
            Ok((news.into_iter().chain(companions).chain(models).collect::<Vec<_>>(), skipped, mostly))
        })
        .map_err(core_error)?;
    let count = |kind: RowKind| rows.iter().filter(|r| r.kind == kind).count();
    let counts = json!({
        "import": count(RowKind::New),
        "removeModels": count(RowKind::MissingModel),
        "removeCompanions": count(RowKind::DanglingCompanion),
    });
    let removes = rows.iter().any(|r| r.kind != RowKind::New);
    let mut answer = json!({
        "token": "", "expiresIn": PLAN_TTL, "counts": counts, "skipped": skipped,
        "mostlyMissingRoots": mostly, "removes": removes,
    });
    if rows.is_empty() {
        return Ok(answer);
    }
    // One plan per session: the new one replaces the old one.
    plans.retain(|_, p| p.session != current.token);
    if plans.len() >= MAX_PLANS {
        return Err(err(503, "snapshot_capacity"));
    }
    let token = random_token()?;
    plans.insert(
        token.clone(),
        Plan {
            session: current.token.clone(),
            created: now,
            rows,
            cursor: 0,
            seq: 0,
            last_result: None,
            started: false,
            confirmed_removal: false,
            applied: Applied::default(),
        },
    );
    answer["token"] = json!(token);
    Ok(answer)
}

/// Names of the roots in which more than half of the active models are in the plan as missing
/// (and at least ten): the volume may be only partly mounted. A hint for the dialog, nothing more.
fn mostly_missing(conn: &katalog_core::write::Connection, roots: &[PathBuf], models: &[PlanRow]) -> Vec<String> {
    let mut names = Vec::new();
    for (index, root) in roots.iter().enumerate() {
        let gone = models.iter().filter(|r| r.root == index).count();
        if gone < 10 {
            continue;
        }
        let prefix = format!("{}/", root.to_string_lossy().trim_end_matches('/'));
        let total: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM files WHERE deleted_at IS NULL AND substr(path, 1, length(?1)) = ?1",
                [&prefix],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if (gone as i64) * 2 > total {
            names.push(root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
        }
    }
    names
}

/// `disk_sync_apply_start`: confirmation, budget, backup, then the plan is switched on.
pub(super) fn apply_start(
    app: &App,
    current: &Session,
    progress: &WriteProgress,
    state: &mut RuntimeState,
    token: &str,
    confirm: bool,
    now: u64,
    allowed: &mut dyn FnMut() -> bool,
) -> Result<Value, Box<Response>> {
    let plan = state
        .plans
        .get_mut(token)
        .filter(|p| p.session == current.token && now.saturating_sub(p.created) < PLAN_TTL)
        .ok_or_else(|| err(404, "snapshot_not_found"))?;
    if plan.started {
        return Err(err(409, "snapshot_mismatch"));
    }
    let removes = plan.removes();
    if removes > 0 && !confirm {
        return Err(err(422, "confirmation_required"));
    }
    let models: Vec<i64> = plan.rows.iter().filter(|r| r.kind == RowKind::MissingModel).filter_map(PlanRow::id).collect();
    progress.set_file_ids(&models);
    if removes > 0 {
        // The whole plan once, against the daily limit: more than the limit is refused with nothing booked.
        app.fileops_safety
            .lock()
            .map_err(|_| err(503, "restart_required"))?
            .reserve_disk_sync(removes, now)
            .map_err(budget_error)?;
    }
    // Before any change; the Tageskopie also covers a plan that only adds entries.
    backup(app, plan.rows.len(), removes > 0)?;
    if !allowed() {
        return Err(err(409, "not_started"));
    }
    plan.confirmed_removal = confirm;
    // Last step: nothing above may leave a half-started plan.
    plan.started = true;
    let total = plan.rows.len();
    Ok(json!({"total": total, "remaining": total}))
}

/// A plan that may be worked off: the right session, not expired, started, and confirmed when it
/// removes entries. Only [`plan_for_block`] makes one.
pub(super) struct Admitted<'a>(&'a mut Plan);

/// The single way into the row loop (spec 2.5). Refuses without changing anything.
pub(super) fn plan_for_block<'a>(
    plans: &'a mut HashMap<String, Plan>,
    session: &str,
    token: &str,
    now: u64,
) -> Result<Admitted<'a>, Box<Response>> {
    let plan = plans
        .get_mut(token)
        .filter(|p| p.session == session && now.saturating_sub(p.created) < PLAN_TTL)
        .ok_or_else(|| err(404, "snapshot_not_found"))?;
    if !plan.started {
        return Err(err(409, "not_started"));
    }
    if plan.removes() > 0 && !plan.confirmed_removal {
        return Err(err(409, "not_started"));
    }
    Ok(Admitted(plan))
}

/// Why a block stopped early.
fn stop_reason(app: &App, current: &Session) -> &'static str {
    if app.geometry.barrier.snapshot().state != crate::barrier::State::Open {
        "maintenance"
    } else if !current.validity.valid(app.clock.now()) {
        "session"
    } else {
        "deadline"
    }
}

#[cfg(test)]
pub(crate) type RowHook = Arc<dyn Fn(usize) + Send + Sync>;
/// Test hook per session token: called after each row of a block (index inside the block).
#[cfg(test)]
pub(crate) fn row_hooks() -> &'static Mutex<HashMap<String, RowHook>> {
    static HOOKS: std::sync::OnceLock<Mutex<HashMap<String, RowHook>>> = std::sync::OnceLock::new();
    HOOKS.get_or_init(Default::default)
}

/// Test helper: fills the plan table with plans of other sessions.
#[cfg(test)]
pub(crate) fn insert_other_plans(state: &mut RuntimeState, count: usize, created: u64) {
    for n in 0..count {
        state.plans.insert(
            format!("filler-{n}"),
            Plan {
                session: format!("filler-session-{n}"),
                created,
                rows: Vec::new(),
                cursor: 0,
                seq: 0,
                last_result: None,
                started: false,
                confirmed_removal: false,
                applied: Applied::default(),
            },
        );
    }
}

/// `disk_sync_apply_block`.
pub(super) fn apply_block(
    app: &App,
    current: &Session,
    progress: &WriteProgress,
    state: &mut RuntimeState,
    token: &str,
    seq: u64,
    now: u64,
    allowed: &mut dyn FnMut() -> bool,
) -> Result<Value, Box<Response>> {
    let RuntimeState { ops, plans, .. } = state;
    let admitted = plan_for_block(plans, &current.token, token, now)?;
    block_rows(app, current, progress, ops, admitted, seq, allowed)
}

fn block_rows(
    app: &App,
    current: &Session,
    progress: &WriteProgress,
    ops: &mut Ops,
    admitted: Admitted<'_>,
    seq: u64,
    allowed: &mut dyn FnMut() -> bool,
) -> Result<Value, Box<Response>> {
    let plan = admitted.0;
    // The block before: the answer got lost, hand out the same one and do nothing again.
    if seq.checked_add(1) == Some(plan.seq) {
        return plan.last_result.clone().ok_or_else(|| err(409, "snapshot_mismatch"));
    }
    if seq != plan.seq || plan.cursor >= plan.rows.len() {
        return Err(err(409, "snapshot_mismatch"));
    }
    let batch: Vec<PlanRow> = plan.rows[plan.cursor..(plan.cursor + BLOCK_ROWS).min(plan.rows.len())].to_vec();
    // Logged before the block runs, like the trash blocks.
    let models: Vec<i64> = batch.iter().filter(|r| r.kind == RowKind::MissingModel).filter_map(PlanRow::id).collect();
    let companions: Vec<i64> = batch.iter().filter(|r| r.kind == RowKind::DanglingCompanion).filter_map(PlanRow::id).collect();
    let owners: Vec<i64> = batch.iter().filter(|r| r.kind == RowKind::DanglingCompanion).filter_map(PlanRow::file_id).collect();
    progress.set_file_ids(&[models.clone(), owners].concat());
    progress.set_companion_ids(&companions);

    let anchored = anchored(app);
    let limits = app.config.upload.as_ref().map(|u| u.limits).unwrap_or_default();
    let step_fn = |file: &std::fs::File| crate::write::step_metadata(&app.geometry.step, file);
    let step = app.geometry.step.available().then_some(&step_fn as katalog_core::rescan::StepReader<'_>);
    let started = Instant::now();
    let mut results = Vec::new();
    let mut stopped = None;
    for (index, row) in batch.iter().enumerate() {
        if !allowed() {
            stopped = Some(stop_reason(app, current));
            break;
        }
        if index > 0 && started.elapsed() >= BLOCK_TIME {
            stopped = Some("deadline");
            break;
        }
        let outcome = match &anchored {
            Ok(anchored) => run_row(ops, anchored, row, limits, step)?,
            Err(reason) => Err(*reason),
        };
        results.push(result_entry(row, outcome));
        #[cfg(test)]
        {
            let hook = row_hooks().lock().unwrap().get(&current.token).cloned();
            if let Some(hook) = hook {
                hook(index);
            }
        }
    }
    for result in &results {
        let kind = result["kind"].as_str().unwrap_or_default();
        match (result["ok"].as_bool() == Some(true), kind) {
            (true, "new") => plan.applied.imported += 1,
            (true, "removeModel") => plan.applied.removed_models += 1,
            (true, _) => plan.applied.removed_companions += 1,
            (false, _) => plan.applied.skipped += 1,
        }
    }
    let total = plan.rows.len();
    let done = plan.cursor + results.len();
    let answer = json!({
        "seq": seq,
        "results": results,
        "done": done, "total": total, "remaining": total - done,
        "counts": {
            "imported": plan.applied.imported,
            "removedModels": plan.applied.removed_models,
            "removedCompanions": plan.applied.removed_companions,
            "skipped": plan.applied.skipped,
        },
        "stopped": stopped,
    });
    // A block that stopped before its first row changed nothing and stays repeatable.
    if !results.is_empty() {
        plan.cursor = done;
        plan.seq += 1;
        plan.last_result = Some(answer.clone());
    }
    Ok(answer)
}

fn result_entry(row: &PlanRow, outcome: Result<Option<i64>, SkipReason>) -> Value {
    let kind = row_label(row.kind);
    match outcome {
        Ok(Some(file_id)) => json!({"kind": kind, "key": row.key, "ok": true, "fileId": file_id.to_string()}),
        Ok(None) => json!({"kind": kind, "key": row.key, "ok": true}),
        Err(reason) => json!({"kind": kind, "key": row.key, "ok": false, "reason": reason.code()}),
    }
}

/// What a removal error from the core means for the row. Only a failure of the runtime itself
/// ends the block.
fn removal_failure(e: fsops::Error) -> Result<SkipReason, Box<Response>> {
    use fsops::Error::*;
    match e {
        // The core says "changed" when the file is (again) there.
        Changed => Ok(SkipReason::ExistsAgain),
        Conflict | NotFound => Ok(SkipReason::Changed),
        HasCompanions => Ok(SkipReason::HasCompanions),
        Protected => Ok(SkipReason::Protected),
        PathUnresolvable | CrossDevice => Ok(SkipReason::Unclear),
        RestartRequired => Err(err(503, "restart_required")),
        _ => Ok(SkipReason::Failed),
    }
}

/// One row, checked against the disk right before it is changed. `Ok(Err(reason))` skips the row,
/// the outer error is a failure of the runtime.
fn run_row(
    ops: &mut Ops,
    anchored: &Anchored,
    row: &PlanRow,
    limits: katalog_core::import::files::WebImportLimits,
    step: Option<katalog_core::rescan::StepReader<'_>>,
) -> Result<Result<Option<i64>, SkipReason>, Box<Response>> {
    match row.kind {
        RowKind::New => {
            // Parsing happens outside the database lock; only the short write holds it.
            let prepared = match disk_apply::read_new_file(row, anchored, limits, step) {
                Ok(prepared) => prepared,
                Err(reason) => return Ok(Err(reason)),
            };
            let stored = ops
                .db
                .with_conn(|conn| Ok(disk_apply::store_new_file(conn, anchored, prepared)))
                .map_err(core_error)?;
            Ok(stored.map(Some))
        }
        RowKind::MissingModel | RowKind::DanglingCompanion => {
            let id = row.id().ok_or_else(|| err(500, "internal"))?;
            let checked = ops
                .db
                .with_conn(|conn| {
                    Ok(if row.kind == RowKind::MissingModel {
                        disk_apply::recheck_missing(row, conn, anchored)
                    } else {
                        disk_apply::recheck_dangling(row, conn, anchored)
                    })
                })
                .map_err(core_error)?;
            if let Err(reason) = checked {
                return Ok(Err(reason));
            }
            // Database only, never a file transfer: a file that is back ends in `Changed`.
            let removed = if row.kind == RowKind::MissingModel {
                ops.remove_missing_entry(id, MissingRemoval::Trash)
            } else {
                ops.remove_missing_companion(id)
            };
            if ops.journal.is_blocked() {
                return Err(err(503, "restart_required"));
            }
            match removed {
                Ok(()) => Ok(Ok(None)),
                Err(e) => Ok(Err(removal_failure(e)?)),
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// date prefix

/// The reads of the date prefix through the same anchored handles as every other file action:
/// no link is followed on any component, upload folder and trash are protected, and "inside the
/// catalog" means "below a root".
struct WebDateRenameFs<'a> {
    ctx: &'a FsContext,
}
impl WebDateRenameFs<'_> {
    fn resolved(&self, path: &Path) -> io::Result<katalog_core::fsops::Resolved> {
        let text = path.to_str().ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
        self.ctx.resolve(text, Access::Source).map_err(|_| io::Error::from(io::ErrorKind::NotFound))
    }
}
impl DateRenameFs for WebDateRenameFs<'_> {
    /// The path as it is, when it resolves below a root (a target that does not exist yet does).
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        self.resolved(path).map(|_| path.to_path_buf())
    }
    fn entry(&self, path: &Path) -> io::Result<()> {
        let r = self.resolved(path)?;
        r.parent.entry_identity(&r.name).map(|_| ())
    }
    /// Reads the file itself (no link): a link has no date here, stricter than the desktop.
    fn modified(&self, path: &Path) -> io::Result<std::time::SystemTime> {
        let r = self.resolved(path)?;
        r.parent.read(&r.name)?.metadata()?.modified()
    }
    fn within_catalog(&self, path: &Path) -> Option<bool> {
        Some(path.to_str().is_some_and(|p| self.ctx.locate(p).is_ok()))
    }
}

/// `preview_date_rename`: reads only.
pub(super) fn date_preview(state: &mut RuntimeState, ids: &[i64], source: DateSource) -> Result<Value, Box<Response>> {
    let ids: Vec<String> = ids.iter().map(i64::to_string).collect();
    let ops = &mut state.ops;
    let fs = WebDateRenameFs { ctx: &ops.context };
    let previews = ops
        .db
        .with_conn(|conn| Ok(date_rename::preview(conn, &ids, source, &[], &fs)))
        .map_err(core_error)?
        .map_err(|_| err(500, "internal"))?;
    Ok(json!(previews))
}

fn date_failure(id: &str, reason: &str) -> Value {
    json!({"id": id, "status": "failed", "reason": reason})
}

/// `apply_date_rename`: every entry is verified again and renamed on its own (journalled, the
/// companion files go along). Entries not started when the time or the barrier ends the request
/// answer `failed` with `not_started`.
pub(super) fn date_apply(
    app: &App,
    progress: &WriteProgress,
    state: &mut RuntimeState,
    items: Vec<DateRenameItem>,
    allowed: &mut dyn FnMut() -> bool,
) -> Result<Value, Box<Response>> {
    let ids: Vec<i64> = items.iter().filter_map(|i| i.id.parse().ok()).collect();
    progress.set_file_ids(&ids);
    backup(app, items.len(), false)?;
    if !allowed() {
        return Err(err(409, "not_started"));
    }
    let ops = &mut state.ops;
    let mut results = Vec::with_capacity(items.len());
    let mut halted = false;
    for item in &items {
        if halted || !allowed() {
            results.push(date_failure(&item.id, "not_started"));
            continue;
        }
        let verified = {
            let fs = WebDateRenameFs { ctx: &ops.context };
            ops.db
                .with_conn(|conn| Ok(date_rename::verify_item(&fs, conn, item, None, &[])))
                .map_err(core_error)?
        };
        let verified = match verified {
            Ok(v) => v,
            Err(reason) => {
                results.push(date_failure(&item.id, &reason));
                continue;
            }
        };
        match ops.rename_file(verified.file.id, &verified.stem) {
            Ok(()) => results.push(json!({"id": item.id, "status": "renamed"})),
            Err(e) => results.push(date_failure(&item.id, &e.to_string())),
        }
        if ops.journal.is_blocked() {
            halted = true;
        }
    }
    Ok(Value::Array(results))
}
