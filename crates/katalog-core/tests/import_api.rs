//! Public, transport-neutral import contract: no Tauri or privileged reader.
use katalog_core::{db, import::{self, files::{self, ImportDb}, *}};
use rusqlite::Connection;
use std::{path::{Path, PathBuf}, sync::{Arc, Mutex, atomic::{AtomicUsize, Ordering}}};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!("core_import_api_{}_{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&path).unwrap(); Self(path)
    }
    fn stl(&self, seed: u32) -> PathBuf {
        let path = self.0.join(format!("{seed}.stl"));
        let mut bytes = vec![0;80]; bytes.extend(1u32.to_le_bytes());
        for v in [0.,0.,1.,0.,0.,seed as f32,10.,0.,0.,0.,10.,1.] { bytes.extend(v.to_le_bytes()); }
        bytes.extend(0u16.to_le_bytes()); std::fs::write(&path,bytes).unwrap(); path
    }
}
impl Drop for Fixture { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }
struct Reader { db: Arc<Mutex<Connection>>, calls: AtomicUsize }
impl ImportReader for Reader {
    fn read(&self, path: &Path) -> ImportResult<db::models::NewFile> {
        let guard = self.db.try_lock().expect("parsing/hash I/O must run outside the database lock");
        drop(guard);
        self.calls.fetch_add(1,Ordering::Relaxed);
        let mut row = files::read_model_file(path,None)?;
        row.content_hash = Some(files::compute_content_hash(path)?); Ok(row)
    }
}
struct NoPlacement;
impl TargetChecker for NoPlacement { fn exists(&self, _: &Path) -> bool { panic!("in-place import attempted placement") } }
struct BatchDb { db: Arc<Mutex<Connection>>, locks: usize, require_immediate: bool }
impl ImportDb for BatchDb {
    fn with_conn<R>(&mut self, f: impl FnOnce(&mut Connection) -> ImportResult<R>) -> ImportResult<R> {
        self.locks += 1; f(&mut self.db.lock().unwrap())
    }
    fn savepoint_control(&self) -> fn(&Connection, &str) -> rusqlite::Result<()> {
        if !self.require_immediate { return |c,sql| c.execute_batch(sql); }
        |c,sql| {
            if sql == "SAVEPOINT import_file" {
                let path: String = c.query_row("SELECT file FROM pragma_database_list WHERE name = 'main'",[],|r| r.get(0))?;
                let competing_writer = Connection::open(path)?;
                competing_writer.busy_timeout(std::time::Duration::ZERO)?;
                // This runs before the first INSERT: DEFERRED would allow this writer.
                let error = competing_writer.execute_batch("BEGIN IMMEDIATE").expect_err("batch must already reserve SQLite's writer lock");
                assert_eq!(error.sqlite_error_code(),Some(rusqlite::ErrorCode::DatabaseBusy));
            }
            c.execute_batch(sql)
        }
    }
}
fn setup(f: &Fixture) -> (Arc<Mutex<Connection>>, FixedFolder) {
    let conn = db::connect(&f.0.join("catalog.db")).unwrap();
    let id = db::insert_folder_with_parent(&conn,"uploads",None,f.0.to_str().unwrap()).unwrap();
    // Desktop setting must have no effect on the explicit in-place contract.
    db::printer_link::set_setting(&conn,"catalog_base_dir","/never-place-here").unwrap();
    (Arc::new(Mutex::new(conn)),FixedFolder { id,root:f.0.clone() })
}
fn service(db: Arc<Mutex<Connection>>) -> (ImportJobs, Arc<Reader>) {
    let reader = Arc::new(Reader {db,calls:AtomicUsize::new(0)});
    (ImportJobs::new(reader.clone(),Arc::new(NoPlacement),Arc::new(RealClock::default()),Arc::new(NoEvents)),reader)
}
fn run(service: &ImportJobs, database: &mut impl ImportDb) -> ImportJobResult {
    let job = service.next().unwrap();
    let JobInput::Models(paths) = job.input() else { panic!("expected models") };
    import::run_models(service,&job,database,paths,&[]).unwrap();
    service.finish(&job,None); service.get(&job.id).unwrap().result().unwrap()
}
#[test]
fn fixed_folder_has_no_placement_and_parsing_outside_batch_locks() {
    let f = Fixture::new(); let (db,folder) = setup(&f); let id = folder.id;
    let paths: Vec<_> = (0..30).map(|i| f.stl(i)).collect();
    let originals: Vec<_> = paths.iter().map(|p| std::fs::read(p).unwrap()).collect();
    let (service,reader) = service(db.clone());
    service.enqueue_in_place(paths.clone(),folder).unwrap();
    let mut database = BatchDb { db:db.clone(),locks:0,require_immediate:false };
    let result = run(&service,&mut database);
    assert_eq!(result.state,ImportState::Finished); assert!(!result.placement_required);
    assert_eq!(result.counts.imported,30); assert_eq!(database.locks,2,"only one database lock per batch");
    assert_eq!(reader.calls.load(Ordering::Relaxed),30);
    for row in db::list_files(&db.lock().unwrap()).unwrap() { assert_eq!(row.folder_id,Some(id)); }
    for (p,bytes) in paths.iter().zip(originals) { assert_eq!(std::fs::read(p).unwrap(),bytes); }
}
#[test]
fn in_place_batch_reserves_writer_before_first_insert() {
    let f=Fixture::new(); let (db,folder)=setup(&f); let (service,_)=service(db.clone());
    service.enqueue_in_place(vec![f.stl(1)],folder).unwrap();
    let mut database=BatchDb {db,locks:0,require_immediate:true};
    assert_eq!(run(&service,&mut database).counts.imported,1);
}
#[test]
fn fixed_folder_rejects_missing_or_mismatched_catalog_row() {
    for mismatch in [false,true] {
        let f=Fixture::new(); let (db,mut folder)=setup(&f); let (service,_)=service(db.clone());
        if mismatch { db.lock().unwrap().execute("UPDATE folders SET path='/different' WHERE id=?1",[folder.id]).unwrap(); }
        else { folder.id += 1000; }
        let job=service.enqueue_in_place(vec![f.stl(1)],folder).unwrap(); service.next().unwrap();
        let JobInput::Models(paths)=job.input() else { unreachable!() };
        let error=run_models(&service,&job,&mut &*db,paths,&[]).unwrap_err();
        assert_eq!(error.message,"targetMissing"); assert!(db::list_files(&db.lock().unwrap()).unwrap().is_empty());
        service.finish(&job,Some(ImportJobError {kind:"targetMissing".into(),message:error.message}));
        assert_eq!(job.result().unwrap().state,ImportState::Failed);
    }
}
#[test]
fn fixed_folder_never_scans_directories_or_reads_outside_explicit_root() {
    let f=Fixture::new(); let other=Fixture::new(); let (db,folder)=setup(&f); let (service,reader)=service(db.clone());
    let nested=f.0.join("nested"); std::fs::create_dir(&nested).unwrap();
    std::fs::rename(f.stl(1),nested.join("hidden.stl")).unwrap();
    service.enqueue_in_place(vec![nested,other.stl(2)],folder).unwrap();
    let result=run(&service,&mut &*db);
    assert_eq!(result.counts.skipped,2); assert_eq!(reader.calls.load(Ordering::Relaxed),0);
    assert!(db::list_files(&db.lock().unwrap()).unwrap().is_empty());
}
#[test]
fn public_core_lifecycle_cancel_releases_catalog_gate() {
    let f=Fixture::new(); let (db,folder)=setup(&f); let (service,_)=service(db);
    let job=service.enqueue_in_place(vec![f.stl(1)],folder).unwrap();
    assert_eq!(service.get(&job.id).unwrap().progress().state,ImportState::Queued);
    assert!(service.gate.exclusive().is_err());
    assert_eq!(service.cancel(&job.id),Some(ImportState::Cancelled));
    assert!(service.next().is_none()); assert!(service.gate.exclusive().is_ok());
    assert_eq!(job.result().unwrap().state,ImportState::Cancelled);
}
