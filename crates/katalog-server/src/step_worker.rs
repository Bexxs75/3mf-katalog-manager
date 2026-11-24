//! STEP preview in a separate, self-limiting process.
//!
//! STEP files come from strangers and are parsed by a large C++ library. A crash, an endless
//! loop or a runaway allocation in it must not take the server down, so the server never calls
//! OCCT itself: it starts its own binary as `katalog-server step-worker <mode> ...` with the
//! admitted file as standard input and reads the result from standard output. The worker first
//! limits itself (resource limits, no core dumps, dies with its parent, Landlock read access to
//! the system libraries and its input only, a seccomp filter against network, exec, process
//! control and changing files) and only then touches the input. Without Landlock it does not run
//! at all. The
//! server adds what the worker cannot enforce on itself: a wall-clock deadline, an output cap and
//! a small number of workers at a time.
//!
//! The protocol is the exit status: 0 means standard output holds the result, every other
//! status is one of the `EXIT_*` codes below, and a signal means the parser died.

use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Same limit as the desktop's STEP parser: larger files are not even handed to the worker.
pub const MAX_INPUT_BYTES: u64 = 64 * 1024 * 1024;
#[cfg(feature = "step-preview")]
const _: () = assert!(MAX_INPUT_BYTES == katalog_core::step::MAX_PARSE_BYTES);

pub const EXIT_USAGE: i32 = 64;
pub const EXIT_TOO_LARGE: i32 = 65;
pub const EXIT_TOO_COMPLEX: i32 = 66;
pub const EXIT_NO_GEOMETRY: i32 = 67;
pub const EXIT_UNREADABLE: i32 = 68;
pub const EXIT_UNSUPPORTED: i32 = 69;
pub const EXIT_SANDBOX: i32 = 70;
pub const EXIT_NOT_REGULAR: i32 = 71;
pub const EXIT_OUTPUT: i32 = 72;
/// The kernel offers no Landlock, or the container forbids it: without it the parser could read
/// every file the server can, so the worker refuses to run at all.
pub const EXIT_NO_LANDLOCK: i32 = 73;
/// Reading the input handle failed below the parser (I/O, not the file's content).
pub const EXIT_INPUT: i32 = 74;
/// The system-call filter stopped the worker at a call outside its allow-list.
pub const EXIT_FILTER: i32 = 75;

/// Lets the test-only modes run. The server starts workers with an empty environment, so a
/// request can never reach them.
pub const SELFTEST_ENV: &str = "KATALOG_STEP_WORKER_SELFTEST";
/// A path for the self-test modes that probe the sandbox from inside.
pub const SELFTEST_PATH_ENV: &str = "KATALOG_STEP_WORKER_SELFTEST_PATH";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Meshes in the viewer's binary format.
    Geometry,
    /// Dimensions, volume and body count as JSON.
    Metadata,
    /// Enters the sandbox and checks that it holds; reads no input.
    Probe,
}
impl Mode {
    fn as_str(self) -> &'static str {
        match self {
            Mode::Geometry => "geometry",
            Mode::Metadata => "metadata",
            Mode::Probe => "probe",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// RLIMIT_AS. OCCT and its thread pool reserve address space well beyond what they touch.
    pub address_space_mib: u64,
    /// RLIMIT_CPU, summed over all threads.
    pub cpu_seconds: u64,
    /// Kill deadline measured by the server; also catches a worker that sleeps or blocks.
    pub wall: Duration,
    /// Largest answer accepted: the mesh of the triangle limit in the viewer format, plus header.
    pub max_output_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            address_space_mib: 2048,
            cpu_seconds: 60,
            wall: Duration::from_secs(90),
            // 1.5 million triangles, non-indexed: 3 x (12 position + 12 normal) + 12 index bytes.
            max_output_bytes: 160 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerError {
    /// Input over [`MAX_INPUT_BYTES`].
    TooLarge,
    /// Over the triangle limit, over the output cap, or out of CPU or wall time.
    TooComplex,
    NoGeometry,
    /// The parser looked at the file and said it cannot read it. Only this outcome (and
    /// `NoGeometry`) says something about the file itself.
    Unreadable,
    /// The worker died (signal, abort, panic) or ended with a status it does not plan. Says
    /// nothing reliable about the file, so nothing may be concluded from it.
    Crashed,
    /// The worker ended "successfully" but its answer is broken, or it failed on its own
    /// channels or its input handle.
    Protocol,
    /// This build or this host cannot run the worker.
    Unavailable,
    /// The kernel or the container offers no Landlock, so the worker cannot confine its file
    /// access and does not run.
    NoIsolation,
    /// All worker places are taken.
    Busy,
}

/// The server side: starts workers and turns their exit into a [`WorkerError`].
pub struct StepWorker {
    /// `None`: no preview (built without OCCT, or the sandbox does not work on this host).
    exe: Option<PathBuf>,
    limits: Limits,
    slots: Arc<Semaphore>,
    /// Number of the last system call the filter stopped a worker at, for the tests.
    last_refused: std::sync::Mutex<Option<String>>,
}
impl Default for StepWorker {
    fn default() -> Self {
        Self::disabled()
    }
}
impl StepWorker {
    pub const PARALLEL: usize = 2;
    pub const RETRY_AFTER_SECONDS: u64 = 5;

    pub fn disabled() -> Self {
        Self::new(None, Limits::default(), Self::PARALLEL)
    }

    /// A worker using `exe` without checking it first; tests use it with the real binary.
    pub fn with_exe(exe: PathBuf, limits: Limits, parallel: usize) -> Self {
        Self::new(Some(exe), limits, parallel)
    }

    fn new(exe: Option<PathBuf>, limits: Limits, parallel: usize) -> Self {
        Self { exe, limits, slots: Arc::new(Semaphore::new(parallel)), last_refused: std::sync::Mutex::new(None) }
    }

    /// Test hook: the call number of the last worker the filter stopped.
    #[doc(hidden)]
    pub fn last_refused_syscall(&self) -> Option<String> {
        self.last_refused.lock().ok().and_then(|last| last.clone())
    }

    /// For the running server: the preview is offered only if this build has OCCT and a probe
    /// worker could enter its sandbox. Blocks for the probe.
    pub fn start(exe: PathBuf, limits: Limits) -> Self {
        if !cfg!(feature = "step-preview") {
            return Self::disabled();
        }
        let candidate = Self::with_exe(exe, limits, Self::PARALLEL);
        match candidate.spawn_and_wait(Mode::Probe, None, Duration::from_secs(10)) {
            Ok(_) => candidate,
            Err(WorkerError::NoIsolation) => {
                eprintln!(
                    "STEP-Vorschau deaktiviert: Landlock (Dateizugriffsschutz) ist im Kernel nicht verfügbar oder vom Container verboten"
                );
                Self::disabled()
            }
            Err(err) => {
                eprintln!("STEP-Vorschau deaktiviert: Prüfprozess fehlgeschlagen ({err:?})");
                Self::disabled()
            }
        }
    }

    pub fn available(&self) -> bool {
        self.exe.is_some()
    }

    /// One worker place, or `Busy` at once: a queue of STEP requests would only hold more
    /// requests open for minutes.
    pub fn try_slot(&self) -> Result<OwnedSemaphorePermit, WorkerError> {
        self.slots.clone().try_acquire_owned().map_err(|_| WorkerError::Busy)
    }

    /// Like [`try_slot`](Self::try_slot), but waits up to `wait` for a place (used by the rescan,
    /// which already runs alone).
    pub fn slot_within(&self, wait: Duration) -> Result<OwnedSemaphorePermit, WorkerError> {
        let deadline = Instant::now() + wait;
        loop {
            match self.try_slot() {
                Ok(permit) => return Ok(permit),
                Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
                Err(e) => return Err(e),
            }
        }
    }

    pub fn limits(&self) -> Limits {
        self.limits
    }

    /// Runs one worker on `input` and returns its output. Blocking; the caller holds a slot.
    /// The size check comes first, so a huge file never starts a process.
    pub fn run(&self, mode: Mode, input: &File, wall: Duration) -> Result<Vec<u8>, WorkerError> {
        if self.exe.is_none() {
            return Err(WorkerError::Unavailable);
        }
        // None of these failures is the parser's verdict on the file (the caller admitted a regular
        // file; a failing fstat or dup means a bad handle or no free descriptors), so they must
        // not be taken for "unreadable", which would clear stored metadata.
        let meta = input.metadata().map_err(|_| WorkerError::Protocol)?;
        if !meta.is_file() {
            return Err(WorkerError::Protocol);
        }
        if meta.len() > MAX_INPUT_BYTES {
            return Err(WorkerError::TooLarge);
        }
        // A second handle on the same open file: the worker reopens it through /proc, so the
        // shared offset does not matter.
        let stdin = input.try_clone().map_err(|_| WorkerError::Protocol)?;
        self.spawn_and_wait(mode, Some(stdin), wall)
    }

    /// Test hook: starts any worker mode, including the self-test ones, with `env` set.
    #[doc(hidden)]
    pub fn run_raw(&self, mode: &str, input: Option<File>, env: &[(&str, &str)]) -> Result<Vec<u8>, WorkerError> {
        let exe = self.exe.as_ref().ok_or(WorkerError::Unavailable)?;
        let mut command = self.command(exe, mode);
        for (key, value) in env {
            command.env(key, value);
        }
        self.wait(command, input, self.limits.wall)
    }

    fn command(&self, exe: &PathBuf, mode: &str) -> Command {
        let mut command = Command::new(exe);
        command
            .arg("step-worker")
            .arg(mode)
            .arg(self.limits.address_space_mib.to_string())
            .arg(self.limits.cpu_seconds.to_string())
            .arg(std::process::id().to_string())
            // Nothing from the server's environment (passwords, paths) reaches the parser.
            .env_clear()
            .current_dir("/");
        command
    }

    fn spawn_and_wait(&self, mode: Mode, input: Option<File>, wall: Duration) -> Result<Vec<u8>, WorkerError> {
        let exe = self.exe.as_ref().ok_or(WorkerError::Unavailable)?;
        let command = self.command(exe, mode.as_str());
        self.wait(command, input, wall)
    }

    fn wait(&self, mut command: Command, input: Option<File>, wall: Duration) -> Result<Vec<u8>, WorkerError> {
        command
            .stdin(input.map_or_else(Stdio::null, Stdio::from))
            .stdout(Stdio::piped())
            // The worker sends OCCT's own stderr to /dev/null at once and keeps this pipe only
            // for one line: which system call its filter refused, if any.
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|_| WorkerError::Unavailable)?;
        let mut stdout = child.stdout.take().ok_or(WorkerError::Unavailable)?;
        let diagnostics = child.stderr.take();
        let cap = self.limits.max_output_bytes;
        // Reading on a thread of its own keeps the pipe drained while this one watches the
        // clock. At the cap the reader stops and closes the pipe, which ends a flooding worker.
        let reader = std::thread::Builder::new()
            .name("step-worker-out".into())
            .spawn(move || {
                let mut out = Vec::new();
                let read = (&mut stdout).take(cap as u64 + 1).read_to_end(&mut out);
                (read.is_ok(), out)
            })
            .map_err(|_| {
                let _ = child.kill();
                let _ = child.wait();
                WorkerError::Unavailable
            })?;
        let deadline = Instant::now() + wall;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
                Ok(None) => break None,
                Err(_) => break None,
            }
        };
        let Some(status) = status else {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            return Err(WorkerError::TooComplex);
        };
        let (read_ok, out) = reader.join().unwrap_or((false, Vec::new()));
        if let Some(number) = log_refused_syscall(status, diagnostics) {
            if let Ok(mut last) = self.last_refused.lock() {
                *last = Some(number);
            }
        }
        if out.len() > cap {
            return Err(WorkerError::TooComplex);
        }
        classify(status, read_ok, out)
    }
}

/// A worker the filter stopped is a crash for the caller (nothing is concluded about the file),
/// but for the operator it means the allow-list misses a call this OCCT build needs, so it is
/// logged with the call's number. Only digits are taken from the worker.
fn log_refused_syscall(status: std::process::ExitStatus, diagnostics: Option<std::process::ChildStderr>) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::ExitStatusExt;
        let filtered = status.code() == Some(EXIT_FILTER) || status.signal() == Some(libc::SIGSYS);
        if !filtered {
            return None;
        }
        let mut line = Vec::new();
        if let Some(mut pipe) = diagnostics {
            let _ = (&mut pipe).take(64).read_to_end(&mut line);
        }
        let number = refused_number(&line);
        eprintln!("STEP-Worker vom Systemaufruf-Filter beendet: Systemaufruf Nr. {number} ({}) ist nicht erlaubt", std::env::consts::ARCH);
        Some(number)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (status, diagnostics);
        None
    }
}

/// The call number from the worker's `syscall=<digits>` line, or "?".
fn refused_number(line: &[u8]) -> String {
    let number: String = line
        .strip_prefix(b"syscall=")
        .unwrap_or(&[])
        .iter()
        .take(10)
        .take_while(|b| b.is_ascii_digit())
        .map(|&b| b as char)
        .collect();
    if number.is_empty() { "?".to_string() } else { number }
}

fn classify(status: std::process::ExitStatus, read_ok: bool, out: Vec<u8>) -> Result<Vec<u8>, WorkerError> {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            // SIGXCPU is the soft CPU limit, SIGKILL the hard one (or the kernel's OOM killer).
            #[cfg(target_os = "linux")]
            if signal == libc::SIGXCPU || signal == libc::SIGKILL {
                return Err(WorkerError::TooComplex);
            }
            let _ = signal;
            // SIGSEGV, SIGABRT and the like: a crash is not a verdict on the file.
            return Err(WorkerError::Crashed);
        }
    }
    match status.code() {
        Some(0) if read_ok => Ok(out),
        Some(0) => Err(WorkerError::Protocol),
        Some(EXIT_TOO_LARGE) => Err(WorkerError::TooLarge),
        Some(EXIT_TOO_COMPLEX) => Err(WorkerError::TooComplex),
        Some(EXIT_NO_GEOMETRY) => Err(WorkerError::NoGeometry),
        Some(EXIT_UNREADABLE) => Err(WorkerError::Unreadable),
        Some(EXIT_NOT_REGULAR | EXIT_OUTPUT | EXIT_INPUT) => Err(WorkerError::Protocol),
        Some(EXIT_UNSUPPORTED | EXIT_SANDBOX | EXIT_USAGE) => Err(WorkerError::Unavailable),
        Some(EXIT_NO_LANDLOCK) => Err(WorkerError::NoIsolation),
        Some(EXIT_FILTER) => Err(WorkerError::Crashed),
        // A Rust panic (101), an allocation failure under the address-space limit or anything
        // else the worker does not plan.
        _ => Err(WorkerError::Crashed),
    }
}

/// Metadata the worker reports in [`Mode::Metadata`].
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepMetadata {
    pub dimensions_mm: Option<[f64; 3]>,
    pub volume_cm3: Option<f64>,
    pub object_count: Option<i64>,
}

// ---------------------------------------------------------------------------------------------
// Worker side

/// Entry point of `katalog-server step-worker <mode> <address-space-MiB> <cpu-seconds> <server-pid>`.
/// Must run before any other thread exists, so every thread OCCT starts later inherits the sandbox.
pub fn worker_main(args: &[String]) -> i32 {
    let [mode, address_space, cpu, parent] = args else { return EXIT_USAGE };
    let (Ok(address_space_mib), Ok(cpu_seconds), Ok(parent)) =
        (address_space.parse::<u64>(), cpu.parse::<u64>(), parent.parse::<u32>())
    else {
        return EXIT_USAGE;
    };
    let selftest = std::env::var_os(SELFTEST_ENV).is_some_and(|v| v == "1");
    let known = matches!(mode.as_str(), "geometry" | "metadata" | "probe")
        || (selftest && mode.starts_with("selftest-"));
    if !known {
        return EXIT_USAGE;
    }
    sandbox::run(mode, address_space_mib, cpu_seconds, parent)
}

#[cfg(target_os = "linux")]
mod sandbox {
    use super::*;
    use std::io::Write;
    use std::os::fd::{AsFd, AsRawFd};

    pub fn run(mode: &str, address_space_mib: u64, cpu_seconds: u64, parent: u32) -> i32 {
        // The result channel is a copy of fd 1; fd 1 itself goes to /dev/null, because OCCT
        // prints progress and warnings to std::cout and would corrupt the answer.
        let Ok(mut out) = take_stdout() else { return EXIT_SANDBOX };
        if take_stderr().is_err() {
            return EXIT_SANDBOX;
        }
        if let Err(code) = enter(address_space_mib, cpu_seconds, parent) {
            return code;
        }
        let result = match mode {
            "probe" => return probe(),
            "geometry" => super::parse::geometry(),
            "metadata" => super::parse::metadata(),
            other => return selftest(other, &mut out),
        };
        match result {
            Ok(bytes) => match out.write_all(&bytes).and_then(|_| out.flush()) {
                Ok(()) => 0,
                Err(_) => EXIT_OUTPUT,
            },
            Err(code) => code,
        }
    }

    fn take_stdout() -> std::io::Result<File> {
        let out = std::io::stdout().as_fd().try_clone_to_owned()?;
        let null = std::fs::OpenOptions::new().write(true).open("/dev/null")?;
        // SAFETY: both descriptors are open and owned by this process; dup2 only replaces fd 1.
        if unsafe { libc::dup2(null.as_raw_fd(), libc::STDOUT_FILENO) } < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(File::from(out))
    }

    /// The copy of fd 2 the SIGSYS handler reports on; -1 until it exists.
    static DIAGNOSTICS: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);

    /// Like stdout: fd 2 goes to /dev/null (OCCT and a panic talk there), a private copy of the
    /// original stays open for one diagnostic line, and SIGSYS (a call outside the filter's
    /// allow-list) writes that line and ends the worker with [`EXIT_FILTER`].
    fn take_stderr() -> std::io::Result<()> {
        use std::os::fd::IntoRawFd;
        let diagnostics = std::io::stderr().as_fd().try_clone_to_owned()?;
        let null = std::fs::OpenOptions::new().write(true).open("/dev/null")?;
        // SAFETY: both descriptors are open and owned by this process; dup2 only replaces fd 2.
        if unsafe { libc::dup2(null.as_raw_fd(), libc::STDERR_FILENO) } < 0 {
            return Err(std::io::Error::last_os_error());
        }
        DIAGNOSTICS.store(diagnostics.into_raw_fd(), std::sync::atomic::Ordering::SeqCst);
        // SAFETY: a zeroed sigaction with a valid handler address and SA_SIGINFO is the
        // documented way to install a three-argument handler.
        unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            let handler: extern "C" fn(libc::c_int, *mut libc::siginfo_t, *mut libc::c_void) = on_sigsys;
            action.sa_sigaction = handler as usize;
            action.sa_flags = libc::SA_SIGINFO;
            libc::sigemptyset(&mut action.sa_mask);
            if libc::sigaction(libc::SIGSYS, &action, std::ptr::null_mut()) != 0 {
                return Err(std::io::Error::last_os_error());
            }
        }
        Ok(())
    }

    /// Async-signal-safe: no allocation, only write(2) and _exit(2).
    extern "C" fn on_sigsys(_signal: libc::c_int, info: *mut libc::siginfo_t, _context: *mut libc::c_void) {
        // In the SIGSYS layout of siginfo_t (64-bit Linux) the call number is the int at byte 24,
        // after signo, errno, code, padding and the faulting address.
        // SAFETY: the kernel passes a valid siginfo_t for a SA_SIGINFO handler.
        let number = unsafe { std::ptr::read_unaligned((info as *const u8).add(24) as *const i32) };
        let mut line = *b"syscall=0000000000\n";
        let mut value = number.unsigned_abs();
        let mut end = line.len() - 1;
        loop {
            end -= 1;
            line[end] = b'0' + (value % 10) as u8;
            value /= 10;
            if value == 0 || end == 8 {
                break;
            }
        }
        let digits = &line[end..];
        let fd = DIAGNOSTICS.load(std::sync::atomic::Ordering::SeqCst);
        // SAFETY: write(2) and _exit(2) are async-signal-safe; the buffers are locals.
        unsafe {
            if fd >= 0 {
                libc::write(fd, b"syscall=".as_ptr().cast(), 8);
                libc::write(fd, digits.as_ptr().cast(), digits.len());
            }
            libc::_exit(EXIT_FILTER);
        }
    }

    #[cfg(target_env = "gnu")]
    type Resource = libc::__rlimit_resource_t;
    #[cfg(not(target_env = "gnu"))]
    type Resource = libc::c_int;

    fn set_limit(resource: Resource, soft: u64, hard: u64) -> Result<(), ()> {
        let limit = libc::rlimit { rlim_cur: soft as libc::rlim_t, rlim_max: hard as libc::rlim_t };
        // SAFETY: plain syscall with a valid pointer to a local struct.
        if unsafe { libc::setrlimit(resource, &limit) } == 0 { Ok(()) } else { Err(()) }
    }

    fn enter(address_space_mib: u64, cpu_seconds: u64, parent: u32) -> Result<(), i32> {
        // SAFETY: prctl with integer arguments only.
        if unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL as libc::c_ulong, 0, 0, 0) } != 0 {
            return Err(EXIT_SANDBOX);
        }
        // The parent may have died before the line above; then nobody waits for the answer. The
        // server names itself, because in a container it often is PID 1 and "reparented to 1"
        // cannot be told from "started by 1".
        // SAFETY: getppid cannot fail.
        if unsafe { libc::getppid() } as u32 != parent {
            return Err(EXIT_SANDBOX);
        }
        let limits = || -> Result<(), ()> {
            set_limit(libc::RLIMIT_CORE, 0, 0)?;
            // The worker writes no files; pipes and /dev/null are not affected by this limit.
            set_limit(libc::RLIMIT_FSIZE, 0, 0)?;
            set_limit(libc::RLIMIT_NOFILE, 32, 32)?;
            let bytes = address_space_mib.saturating_mul(1024 * 1024);
            set_limit(libc::RLIMIT_AS, bytes, bytes)?;
            // Soft limit sends SIGXCPU, the hard one a second later SIGKILL.
            set_limit(libc::RLIMIT_CPU, cpu_seconds, cpu_seconds.saturating_add(1))
        };
        limits().map_err(|_| EXIT_SANDBOX)?;
        // Required by Landlock and seccomp alike for an unprivileged process.
        // SAFETY: prctl with integer arguments only.
        if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
            return Err(EXIT_SANDBOX);
        }
        // The file view first: the seccomp filter cannot tell a library from the catalog
        // database, Landlock can.
        super::landlock::restrict().map_err(|e| match e {
            super::landlock::Error::Missing => EXIT_NO_LANDLOCK,
            super::landlock::Error::Failed => EXIT_SANDBOX,
        })?;
        super::filter::apply().map_err(|_| EXIT_SANDBOX)
    }

    /// Exits 0 only if the sandbox blocks what it must and still allows threads.
    fn probe() -> i32 {
        // Landlock holds: the root directory is outside every rule, yet readable for the server.
        if std::fs::read_dir("/").is_ok() {
            return EXIT_SANDBOX;
        }
        // SAFETY: plain syscalls with constant arguments; a returned descriptor is closed at once.
        let socket = unsafe { libc::socket(libc::AF_INET, libc::SOCK_STREAM, 0) };
        if socket >= 0 {
            unsafe { libc::close(socket) };
            return EXIT_SANDBOX;
        }
        // SAFETY: kill with signal 0 only checks permission.
        if unsafe { libc::kill(libc::getppid(), 0) } == 0 {
            return EXIT_SANDBOX;
        }
        if std::fs::OpenOptions::new().write(true).create(true).truncate(true).open("/tmp/.katalog-step-probe").is_ok() {
            return EXIT_SANDBOX;
        }
        if Command::new("/bin/true").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok() {
            return EXIT_SANDBOX;
        }
        // OCCT meshes with a thread pool; a filter that breaks threads would break every preview.
        match std::thread::spawn(|| 7).join() {
            Ok(7) => 0,
            _ => EXIT_SANDBOX,
        }
    }

    /// Misbehaving workers for the tests, inside the same sandbox as a real one.
    fn selftest(mode: &str, out: &mut File) -> i32 {
        match mode {
            "selftest-spin" => {
                let mut x = 0u64;
                loop {
                    x = std::hint::black_box(x.wrapping_add(1));
                }
            }
            "selftest-sleep" => loop {
                std::thread::sleep(Duration::from_secs(1));
            },
            "selftest-abort" => std::process::abort(),
            "selftest-panic" => panic!("worker self-test"),
            // A call outside the allow-list (and not among the polite refusals): the filter must
            // stop the worker instead of letting it carry on.
            // SAFETY: syslog with a null buffer and length 0 only asks for the buffer size.
            "selftest-forbidden" => unsafe {
                libc::syscall(libc::SYS_syslog, 10, std::ptr::null_mut::<libc::c_char>(), 0);
                0
            },
            "selftest-read" => {
                // A file the server could read: inside the sandbox it must stay closed.
                let Some(path) = std::env::var_os(SELFTEST_PATH_ENV) else { return EXIT_USAGE };
                let mut content = Vec::new();
                match File::open(path).and_then(|mut f| f.read_to_end(&mut content)) {
                    Ok(_) => match out.write_all(&content) {
                        Ok(()) => 0,
                        Err(_) => EXIT_OUTPUT,
                    },
                    Err(_) => EXIT_UNREADABLE,
                }
            }
            "selftest-foreign" => {
                // The process named in the path variable (the server in the test) must be out of
                // reach for signals, limits and scheduling.
                let Some(target) = std::env::var(SELFTEST_PATH_ENV).ok().and_then(|v| v.parse().ok()) else {
                    return EXIT_USAGE;
                };
                match out.write_all(act_on(target, 0, None).as_bytes()) {
                    Ok(()) => 0,
                    Err(_) => EXIT_OUTPUT,
                }
            }
            "selftest-mutate" => {
                // Every way to change the input file the worker holds open, with the errno each
                // attempt got ("ok" if it went through).
                let report = mutate(0, c"/proc/self/fd/0");
                match out.write_all(report.as_bytes()) {
                    Ok(()) => 0,
                    Err(_) => EXIT_OUTPUT,
                }
            }
            "selftest-flood" => {
                let block = vec![0x5au8; 1 << 16];
                while out.write_all(&block).is_ok() {}
                EXIT_OUTPUT
            }
            "selftest-alloc" => {
                // Far over the address-space limit: must fail, not succeed.
                let mut blocks: Vec<Vec<u8>> = Vec::new();
                loop {
                    let mut block = Vec::new();
                    if block.try_reserve_exact(256 * 1024 * 1024).is_err() {
                        return EXIT_TOO_COMPLEX;
                    }
                    block.resize(256 * 1024 * 1024, 1);
                    blocks.push(block);
                }
            }
            "selftest-echo" => {
                let mut input = Vec::new();
                if File::open("/proc/self/fd/0").and_then(|mut f| f.read_to_end(&mut input)).is_err() {
                    return EXIT_UNREADABLE;
                }
                match out.write_all(&input) {
                    Ok(()) => 0,
                    Err(_) => EXIT_OUTPUT,
                }
            }
            _ => EXIT_USAGE,
        }
    }

    fn outcome(result: libc::c_long) -> String {
        if result >= 0 {
            "ok".into()
        } else {
            std::io::Error::last_os_error().raw_os_error().unwrap_or(0).to_string()
        }
    }

    /// Every way to change the file behind `fd` (also reachable as `path`), with the errno each
    /// attempt got ("ok" if it went through). Shared by the self-test and the filter test.
    pub(super) fn mutate(fd: libc::c_int, path: &std::ffi::CStr) -> String {
        const FS_IOC_SETFLAGS: u64 = 0x4008_6602;
        const FS_IOC_FSSETXATTR: u64 = 0x401c_5820;
        const FS_NODUMP_FL: libc::c_long = 0x40;
        let mut lines = Vec::new();
        // SAFETY: every call gets constant arguments, a NUL-terminated path, pointers to live
        // locals or null pointers (which mean "now" for the time calls); a returned descriptor is
        // closed at once.
        unsafe {
            let opened = libc::open(path.as_ptr(), libc::O_RDONLY | libc::O_TRUNC);
            if opened >= 0 {
                libc::close(opened);
            }
            lines.push(format!("open_rdonly_trunc={}", outcome(opened as libc::c_long)));
            let opened = libc::openat(libc::AT_FDCWD, path.as_ptr(), libc::O_RDONLY | libc::O_TRUNC);
            if opened >= 0 {
                libc::close(opened);
            }
            lines.push(format!("openat_rdonly_trunc={}", outcome(opened as libc::c_long)));
            lines.push(format!("fchmod={}", outcome(libc::fchmod(fd, 0o666) as libc::c_long)));
            lines.push(format!(
                "fchmodat={}",
                outcome(libc::fchmodat(libc::AT_FDCWD, path.as_ptr(), 0o666, 0) as libc::c_long)
            ));
            lines.push(format!(
                "fchmodat2={}",
                outcome(libc::syscall(super::filter::SYS_FCHMODAT2, libc::AT_FDCWD, path.as_ptr(), 0o666, 0))
            ));
            lines.push(format!("fchown={}", outcome(libc::fchown(fd, libc::getuid(), libc::getgid()) as libc::c_long)));
            lines.push(format!("futimens={}", outcome(libc::futimens(fd, std::ptr::null()) as libc::c_long)));
            lines.push(format!(
                "utimensat={}",
                outcome(libc::utimensat(libc::AT_FDCWD, path.as_ptr(), std::ptr::null(), 0) as libc::c_long)
            ));
            lines.push(format!("ftruncate={}", outcome(libc::ftruncate(fd, 0) as libc::c_long)));
            let flags: libc::c_long = FS_NODUMP_FL;
            lines.push(format!(
                "ioctl_setflags={}",
                outcome(libc::syscall(libc::SYS_ioctl, fd, FS_IOC_SETFLAGS, &flags as *const libc::c_long))
            ));
            let fsxattr = [0u8; 28];
            lines.push(format!(
                "ioctl_fssetxattr={}",
                outcome(libc::syscall(libc::SYS_ioctl, fd, FS_IOC_FSSETXATTR, fsxattr.as_ptr()))
            ));
        }
        lines.join("\n")
    }

    /// Every way to act on the process `target` (signals, limits, scheduling, handles), with the
    /// errno each attempt got. Signal 0 only checks permission, so nothing reaches the target if
    /// a call goes through; the limit attempt sets the open-file limit to zero on purpose, so a
    /// test sees whether it landed.
    pub(super) fn act_on(target: libc::pid_t, async_fd: libc::c_int, async_write: Option<libc::c_int>) -> String {
        let mut lines = Vec::new();
        // SAFETY: plain system calls with integer arguments or pointers to live locals of the
        // layout the kernel expects; a returned descriptor is closed at once.
        unsafe {
            lines.push(format!("kill={}", outcome(libc::kill(target, 0) as libc::c_long)));
            lines.push(format!("kill_group={}", outcome(libc::kill(0, 0) as libc::c_long)));
            lines.push(format!("kill_all={}", outcome(libc::kill(-1, 0) as libc::c_long)));
            lines.push(format!("tkill={}", outcome(libc::syscall(libc::SYS_tkill, target, 0))));
            lines.push(format!("tgkill={}", outcome(libc::syscall(libc::SYS_tgkill, target, target, 0))));
            let mut info: libc::siginfo_t = std::mem::zeroed();
            info.si_code = libc::SI_QUEUE;
            lines.push(format!(
                "rt_sigqueueinfo={}",
                outcome(libc::syscall(libc::SYS_rt_sigqueueinfo, target, 0, &info as *const libc::siginfo_t))
            ));
            lines.push(format!(
                "rt_tgsigqueueinfo={}",
                outcome(libc::syscall(libc::SYS_rt_tgsigqueueinfo, target, target, 0, &info as *const libc::siginfo_t))
            ));
            let pidfd = libc::syscall(libc::SYS_pidfd_open, target, 0);
            if pidfd >= 0 {
                libc::close(pidfd as libc::c_int);
            }
            lines.push(format!("pidfd_open={}", outcome(pidfd)));
            let zero = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
            lines.push(format!(
                "prlimit64={}",
                outcome(libc::syscall(
                    libc::SYS_prlimit64,
                    target,
                    libc::RLIMIT_NOFILE,
                    &zero as *const libc::rlimit,
                    std::ptr::null_mut::<libc::rlimit>()
                ))
            ));
            let mut set: libc::cpu_set_t = std::mem::zeroed();
            libc::sched_getaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &mut set);
            lines.push(format!(
                "sched_setaffinity={}",
                outcome(libc::sched_setaffinity(target, std::mem::size_of::<libc::cpu_set_t>(), &set) as libc::c_long)
            ));
            let param = libc::sched_param { sched_priority: 0 };
            lines.push(format!(
                "sched_setscheduler={}",
                outcome(libc::sched_setscheduler(target, libc::SCHED_OTHER, &param) as libc::c_long)
            ));
            lines.push(format!(
                "setpriority={}",
                outcome(libc::setpriority(libc::PRIO_PROCESS, target as libc::id_t, 0) as libc::c_long)
            ));
            lines.push(format!("ioprio_set={}", outcome(libc::syscall(libc::SYS_ioprio_set, 1, target, 0))));
            lines.push(format!("kcmp={}", outcome(libc::syscall(libc::SYS_kcmp, target, target, 0, 0, 0))));
            // A descriptor that signals its owner: owner = target, signal = SIGTERM, O_ASYNC on,
            // then something to read on it. Each step must be refused.
            lines.push(format!("fcntl_setown={}", outcome(libc::fcntl(async_fd, libc::F_SETOWN, target) as libc::c_long)));
            // libc names neither F_SETOWN_EX nor F_SETSIG for these targets.
            const F_SETSIG: libc::c_int = 10;
            const F_SETOWN_EX: libc::c_int = 15;
            #[repr(C)]
            struct OwnerEx {
                kind: libc::c_int,
                pid: libc::pid_t,
            }
            let owner = OwnerEx { kind: 1, pid: target };
            lines.push(format!(
                "fcntl_setown_ex={}",
                outcome(libc::fcntl(async_fd, F_SETOWN_EX, &owner as *const OwnerEx) as libc::c_long)
            ));
            lines.push(format!("fcntl_setsig={}", outcome(libc::fcntl(async_fd, F_SETSIG, libc::SIGTERM) as libc::c_long)));
            let flags = libc::fcntl(async_fd, libc::F_GETFL);
            lines.push(format!(
                "fcntl_setfl_async={}",
                outcome(libc::fcntl(async_fd, libc::F_SETFL, flags | libc::O_ASYNC) as libc::c_long)
            ));
            if let Some(fd) = async_write {
                libc::write(fd, b"x".as_ptr().cast(), 1);
            }
            let mut fds = [0 as libc::c_int; 2];
            let piped = libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC);
            if piped == 0 {
                libc::close(fds[0]);
                libc::close(fds[1]);
            }
            lines.push(format!("pipe2={}", outcome(piped as libc::c_long)));
            lines.push(format!(
                "process_vm_readv={}",
                outcome(libc::syscall(libc::SYS_process_vm_readv, target, std::ptr::null::<libc::iovec>(), 0, std::ptr::null::<libc::iovec>(), 0, 0))
            ));
        }
        lines.join("\n")
    }
}

#[cfg(not(target_os = "linux"))]
mod sandbox {
    /// Without the Linux limits the worker would parse unprotected; it refuses instead.
    pub fn run(_mode: &str, _address_space_mib: u64, _cpu_seconds: u64, _parent: u32) -> i32 {
        super::EXIT_SANDBOX
    }
}

/// Landlock through the raw system calls: the worker keeps read access to the system libraries
/// and data OCCT and the C library load at run time, to its own /proc entry, and to the one
/// input file it was handed, and loses every other file access, including the catalog database
/// and the password file the server itself can read.
#[cfg(target_os = "linux")]
mod landlock {
    use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd};

    pub enum Error {
        /// No Landlock in this kernel, or the container's own filter forbids it.
        Missing,
        /// Landlock exists but the ruleset could not be built or entered.
        Failed,
    }

    const CREATE_RULESET_VERSION: u32 = 1;
    const RULE_PATH_BENEATH: libc::c_int = 1;

    const FS_READ_FILE: u64 = 1 << 2;
    const FS_READ_DIR: u64 = 1 << 3;
    /// Every access right of the first ABI; later ABIs add REFER, TRUNCATE and IOCTL_DEV.
    const FS_ABI1: u64 = (1 << 13) - 1;
    const FS_REFER: u64 = 1 << 13;
    const FS_TRUNCATE: u64 = 1 << 14;
    const FS_IOCTL_DEV: u64 = 1 << 15;
    const NET_BIND_TCP: u64 = 1 << 0;
    const NET_CONNECT_TCP: u64 = 1 << 1;
    const SCOPE_ABSTRACT_UNIX_SOCKET: u64 = 1 << 0;
    const SCOPE_SIGNAL: u64 = 1 << 1;

    /// Paths read below which is allowed. Library loading, locale and character set data,
    /// the loader cache and the CPU list (thread pool size) are all OCCT and glibc need after
    /// the worker has started; a missing entry is skipped.
    const READABLE: &[&str] = &["/usr", "/lib", "/lib64", "/etc/ld.so.cache", "/sys/devices/system/cpu", "/proc/self"];

    #[repr(C)]
    struct RulesetAttr {
        handled_access_fs: u64,
        handled_access_net: u64,
        scoped: u64,
    }

    #[repr(C, packed)]
    struct PathBeneathAttr {
        allowed_access: u64,
        parent_fd: i32,
    }

    fn abi() -> Option<i64> {
        // SAFETY: the version query takes a null attribute pointer and size 0 by definition.
        let version = unsafe {
            libc::syscall(
                libc::SYS_landlock_create_ruleset,
                std::ptr::null::<RulesetAttr>(),
                0usize,
                CREATE_RULESET_VERSION,
            )
        };
        (version >= 1).then_some(version)
    }

    fn add_rule(ruleset: &OwnedFd, target: BorrowedFd<'_>, access: u64) -> Result<(), Error> {
        let attr = PathBeneathAttr { allowed_access: access, parent_fd: target.as_raw_fd() };
        // SAFETY: the attribute is a live local of the layout the kernel expects.
        let result = unsafe {
            libc::syscall(
                libc::SYS_landlock_add_rule,
                ruleset.as_raw_fd(),
                RULE_PATH_BENEATH,
                &attr as *const PathBeneathAttr,
                0u32,
            )
        };
        if result == 0 { Ok(()) } else { Err(Error::Failed) }
    }

    /// Read rights for `path` and what lies below it; files only take file rights.
    fn allow_path(ruleset: &OwnedFd, path: &str) -> Result<(), Error> {
        let Ok(c_path) = std::ffi::CString::new(path) else { return Err(Error::Failed) };
        // SAFETY: NUL-terminated path; O_PATH opens nothing for reading or writing.
        let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_PATH | libc::O_CLOEXEC) };
        if fd < 0 {
            return match std::io::Error::last_os_error().raw_os_error() {
                Some(libc::ENOENT | libc::ENOTDIR) => Ok(()),
                _ => Err(Error::Failed),
            };
        }
        // SAFETY: a fresh descriptor this function owns.
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        let is_dir = std::fs::File::from(fd.try_clone().map_err(|_| Error::Failed)?)
            .metadata()
            .map_err(|_| Error::Failed)?
            .is_dir();
        let access = if is_dir { FS_READ_FILE | FS_READ_DIR } else { FS_READ_FILE };
        add_rule(ruleset, fd.as_fd(), access)
    }

    pub fn restrict() -> Result<(), Error> {
        let Some(abi) = abi() else { return Err(Error::Missing) };
        let mut fs = FS_ABI1;
        if abi >= 2 {
            fs |= FS_REFER;
        }
        if abi >= 3 {
            fs |= FS_TRUNCATE;
        }
        if abi >= 5 {
            fs |= FS_IOCTL_DEV;
        }
        let attr = RulesetAttr {
            handled_access_fs: fs,
            handled_access_net: if abi >= 4 { NET_BIND_TCP | NET_CONNECT_TCP } else { 0 },
            scoped: if abi >= 6 { SCOPE_ABSTRACT_UNIX_SOCKET | SCOPE_SIGNAL } else { 0 },
        };
        // SAFETY: the attribute is a live local and its size is passed with it.
        let ruleset = unsafe {
            libc::syscall(
                libc::SYS_landlock_create_ruleset,
                &attr as *const RulesetAttr,
                std::mem::size_of::<RulesetAttr>(),
                0u32,
            )
        };
        if ruleset < 0 {
            return Err(Error::Failed);
        }
        // SAFETY: a fresh descriptor this function owns.
        let ruleset = unsafe { OwnedFd::from_raw_fd(ruleset as i32) };
        for path in READABLE {
            allow_path(&ruleset, path)?;
        }
        // The input: OCCT opens it again by path through /proc/self/fd/0, which Landlock checks
        // against the file itself. Only a regular file gets a rule (the probe has /dev/null).
        // SAFETY: fd 0 stays open for the whole life of the worker.
        let input = unsafe { BorrowedFd::borrow_raw(0) };
        let input_is_file = input
            .try_clone_to_owned()
            .ok()
            .and_then(|fd| std::fs::File::from(fd).metadata().ok())
            .is_some_and(|m| m.is_file());
        if input_is_file {
            add_rule(&ruleset, input, FS_READ_FILE)?;
        }
        // SAFETY: plain syscall on the ruleset descriptor; no_new_privs is already set.
        if unsafe { libc::syscall(libc::SYS_landlock_restrict_self, ruleset.as_raw_fd(), 0u32) } != 0 {
            return Err(Error::Failed);
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
mod filter {
    use seccompiler::{
        BpfProgram, SeccompAction, SeccompCmpArgLen as Len, SeccompCmpOp as Op, SeccompCondition as Cond,
        SeccompFilter, SeccompRule, TargetArch,
    };
    use std::collections::BTreeMap;

    type Rules = BTreeMap<i64, Vec<SeccompRule>>;

    // Generic numbers, the same on x86_64 and aarch64; libc does not name all of them for every
    // target. A kernel that predates one answers ENOSYS anyway.
    pub(super) const SYS_FCHMODAT2: i64 = 452;
    const SYS_SETXATTRAT: i64 = 463;
    const SYS_REMOVEXATTRAT: i64 = 466;
    const SYS_FILE_SETATTR: i64 = 469;

    /// Terminal queries (is it a terminal, window size) and "bytes waiting": read-only questions
    /// the C and C++ runtimes may ask of a descriptor.
    // The request type is c_ulong with glibc but c_int with musl, so the casts are not idle there.
    #[allow(clippy::unnecessary_cast)]
    const ALLOWED_IOCTLS: [u64; 3] = [libc::TCGETS as u64, libc::TIOCGWINSZ as u64, libc::FIONREAD as u64];

    /// Descriptor flags, status flags, duplicating and the pipe size query.
    const ALLOWED_FCNTLS: [u64; 7] = [
        libc::F_DUPFD as u64,
        libc::F_DUPFD_CLOEXEC as u64,
        libc::F_GETFD as u64,
        libc::F_SETFD as u64,
        libc::F_GETFL as u64,
        libc::F_SETFL as u64,
        libc::F_GETPIPE_SZ as u64,
    ];

    /// PR_SET_NAME, PR_GET_NAME, PR_SET_NO_NEW_PRIVS, PR_GET_NO_NEW_PRIVS and PR_SET_VMA (the
    /// C library names anonymous mappings with it).
    const ALLOWED_PRCTLS: [u64; 5] = [15, 16, 38, 39, 0x5356_4d41];

    fn rule(conditions: Vec<Cond>) -> Result<SeccompRule, seccompiler::BackendError> {
        SeccompRule::new(conditions)
    }

    /// Refused outright: network, running programs, debugging or signalling others, mounts and
    /// namespaces, kernel interfaces a parser never needs, and every way to change the filesystem.
    fn denied(rules: &mut Rules) {
        let always = [
            libc::SYS_socket, libc::SYS_socketpair, libc::SYS_connect, libc::SYS_bind, libc::SYS_listen,
            libc::SYS_accept, libc::SYS_accept4, libc::SYS_sendto, libc::SYS_sendmsg, libc::SYS_sendmmsg,
            libc::SYS_execve, libc::SYS_execveat, libc::SYS_ptrace, libc::SYS_process_vm_readv,
            libc::SYS_process_vm_writev, libc::SYS_mount, libc::SYS_umount2, libc::SYS_pivot_root,
            libc::SYS_chroot, libc::SYS_unshare, libc::SYS_setns, libc::SYS_bpf, libc::SYS_perf_event_open,
            libc::SYS_keyctl, libc::SYS_add_key, libc::SYS_request_key, libc::SYS_init_module,
            libc::SYS_finit_module, libc::SYS_delete_module, libc::SYS_kexec_load, libc::SYS_reboot,
            libc::SYS_swapon, libc::SYS_swapoff, libc::SYS_mknodat, libc::SYS_unlinkat, libc::SYS_renameat2,
            libc::SYS_mkdirat, libc::SYS_linkat, libc::SYS_symlinkat, libc::SYS_fchmodat, libc::SYS_fchownat,
            libc::SYS_truncate, libc::SYS_openat2, libc::SYS_io_uring_setup, libc::SYS_userfaultfd,
            libc::SYS_tkill, libc::SYS_pidfd_open, libc::SYS_pidfd_send_signal, libc::SYS_pidfd_getfd,
            libc::SYS_name_to_handle_at, libc::SYS_open_by_handle_at, libc::SYS_process_madvise,
            // Changing a file without writing to it: rights, owner, times, size, attributes.
            // Landlock does not cover these, and a read-only handle is enough for most of them.
            libc::SYS_fchmod, SYS_FCHMODAT2, libc::SYS_fchown, libc::SYS_utimensat, libc::SYS_ftruncate,
            libc::SYS_fallocate, libc::SYS_setxattr, libc::SYS_lsetxattr, libc::SYS_fsetxattr,
            libc::SYS_removexattr, libc::SYS_lremovexattr, libc::SYS_fremovexattr, SYS_SETXATTRAT,
            SYS_REMOVEXATTRAT, SYS_FILE_SETATTR, libc::SYS_fanotify_init, libc::SYS_quotactl,
            // Acting on other processes of the same user: comparing their resources, changing
            // their scheduling, priority, I/O class or memory placement, reading their robust
            // futex list. The worker needs none of it for itself.
            libc::SYS_kcmp, libc::SYS_sched_setscheduler, libc::SYS_sched_setparam, libc::SYS_sched_setattr,
            libc::SYS_setpriority, libc::SYS_ioprio_set, libc::SYS_migrate_pages, libc::SYS_move_pages,
            libc::SYS_get_robust_list, libc::SYS_process_mrelease,
            // System V IPC and POSIX message queues are shared by every process of the user; the
            // worker could remove or fill the server's or other programs' objects.
            libc::SYS_shmget, libc::SYS_shmat, libc::SYS_shmctl, libc::SYS_shmdt, libc::SYS_semget,
            libc::SYS_semop, libc::SYS_semctl, libc::SYS_semtimedop, libc::SYS_msgget, libc::SYS_msgsnd,
            libc::SYS_msgrcv, libc::SYS_msgctl, libc::SYS_mq_open, libc::SYS_mq_unlink,
            libc::SYS_mq_timedsend, libc::SYS_mq_timedreceive, libc::SYS_mq_notify, libc::SYS_mq_getsetattr,
            // The newer mount interface, next to mount/umount2 above.
            libc::SYS_open_tree, libc::SYS_move_mount, libc::SYS_fsopen, libc::SYS_fsconfig,
            libc::SYS_fsmount, libc::SYS_fspick, libc::SYS_mount_setattr,
            // A pipe of its own is the start of signal tricks (O_ASYNC with a foreign owner);
            // the worker already has every pipe it needs.
            libc::SYS_pipe2,
        ];
        for nr in always {
            rules.insert(nr, Vec::new());
        }
        #[cfg(target_arch = "x86_64")]
        for nr in [
            libc::SYS_creat, libc::SYS_mkdir, libc::SYS_rmdir, libc::SYS_unlink, libc::SYS_rename,
            libc::SYS_renameat, libc::SYS_link, libc::SYS_symlink, libc::SYS_chmod, libc::SYS_chown,
            libc::SYS_lchown, libc::SYS_mknod, libc::SYS_fork, libc::SYS_vfork, libc::SYS_utime,
            libc::SYS_utimes, libc::SYS_futimesat, libc::SYS_pipe,
        ] {
            rules.insert(nr, Vec::new());
        }
    }

    /// Opening for writing, creating or truncating, by flag argument `flags_arg` of the open call.
    /// O_TRUNC counts on its own: Linux empties a file opened O_RDONLY|O_TRUNC if the caller may
    /// write it, and RLIMIT_FSIZE does not stop a file from shrinking.
    fn writing_opens(flags_arg: u8) -> Result<Vec<SeccompRule>, seccompiler::BackendError> {
        let accmode = libc::O_ACCMODE as u64;
        let flag = |bit: libc::c_int| Cond::new(flags_arg, Len::Dword, Op::MaskedEq(bit as u64), bit as u64);
        Ok(vec![
            rule(vec![Cond::new(flags_arg, Len::Dword, Op::MaskedEq(accmode), libc::O_WRONLY as u64)?])?,
            rule(vec![Cond::new(flags_arg, Len::Dword, Op::MaskedEq(accmode), libc::O_RDWR as u64)?])?,
            rule(vec![flag(libc::O_CREAT)?])?,
            rule(vec![flag(libc::O_TRUNC)?])?,
        ])
    }

    fn conditional(rules: &mut Rules, pid: u64) -> Result<(), seccompiler::BackendError> {
        rules.insert(libc::SYS_openat, writing_opens(2)?);
        #[cfg(target_arch = "x86_64")]
        rules.insert(libc::SYS_open, writing_opens(1)?);
        // Signals only to itself: abort() and the runtime use them, the server must not get one.
        // Every call that names a target process is covered here (tkill and pidfd_send_signal
        // are refused above), because Landlock's signal scope exists only from its sixth ABI.
        // Pid 0 and negative pids address process groups, which include the server.
        let own = |arg: u8| -> Result<Vec<SeccompRule>, seccompiler::BackendError> {
            Ok(vec![rule(vec![Cond::new(arg, Len::Dword, Op::Ne, pid)?])?])
        };
        rules.insert(libc::SYS_kill, own(0)?);
        rules.insert(libc::SYS_tgkill, own(0)?);
        rules.insert(libc::SYS_rt_sigqueueinfo, own(0)?);
        rules.insert(libc::SYS_rt_tgsigqueueinfo, own(0)?);
        // Resource limits and CPU affinity of itself only (pid 0 or its own pid): with the same
        // user id it could otherwise set the server's open-file limit to zero.
        let own_or_zero = |arg: u8| -> Result<Vec<SeccompRule>, seccompiler::BackendError> {
            Ok(vec![rule(vec![Cond::new(arg, Len::Dword, Op::Ne, pid)?, Cond::new(arg, Len::Dword, Op::Ne, 0)?])?])
        };
        rules.insert(libc::SYS_prlimit64, own_or_zero(0)?);
        rules.insert(libc::SYS_sched_setaffinity, own_or_zero(0)?);
        // ioctl only for what the runtime asks of its standard streams; everything else, such as
        // setting file flags (FS_IOC_SETFLAGS) or attributes (FS_IOC_FSSETXATTR) on the input,
        // is refused. The kernel reads the request as 32 bits, so comparing 32 bits is exact.
        rules.insert(
            libc::SYS_ioctl,
            vec![rule(
                ALLOWED_IOCTLS
                    .iter()
                    .map(|&request| Cond::new(1, Len::Dword, Op::Ne, request))
                    .collect::<Result<Vec<_>, _>>()?,
            )?],
        );
        // Threads yes, new processes no: a clone without CLONE_THREAD is a fork.
        rules.insert(
            libc::SYS_clone,
            vec![rule(vec![Cond::new(0, Len::Qword, Op::MaskedEq(libc::CLONE_THREAD as u64), 0)?])?],
        );
        // fcntl only for descriptor flags, status flags and duplicating. Refused are, among
        // others, F_SETOWN/F_SETOWN_EX/F_SETSIG (a descriptor that signals another process),
        // leases, directory notifications and locks; F_SETFL may not switch on O_ASYNC.
        rules.insert(
            libc::SYS_fcntl,
            vec![
                rule(
                    ALLOWED_FCNTLS
                        .iter()
                        .map(|&command| Cond::new(1, Len::Dword, Op::Ne, command))
                        .collect::<Result<Vec<_>, _>>()?,
                )?,
                rule(vec![
                    Cond::new(1, Len::Dword, Op::Eq, libc::F_SETFL as u64)?,
                    Cond::new(2, Len::Dword, Op::MaskedEq(libc::O_ASYNC as u64), libc::O_ASYNC as u64)?,
                ])?,
            ],
        );
        // prctl for thread and mapping names (the C library names its allocations) and for the
        // no-new-privileges flag the filter setup sets; nothing else, so the worker cannot, for
        // example, drop its parent-death signal.
        rules.insert(
            libc::SYS_prctl,
            vec![rule(
                ALLOWED_PRCTLS
                    .iter()
                    .map(|&option| Cond::new(0, Len::Dword, Op::Ne, option))
                    .collect::<Result<Vec<_>, _>>()?,
            )?],
        );
        Ok(())
    }

    /// What the worker needs after the sandbox is closed: reading its input, memory, threads
    /// (OCCT and TBB), time, its own signals and exit. Measured with strace over every worker
    /// mode and fixture, plus the thread calls glibc and the Rust runtime make. Anything that is
    /// neither here nor refused politely by the rules above ends the worker with SIGSYS.
    fn allowed() -> Vec<i64> {
        let mut calls = vec![
            libc::SYS_read, libc::SYS_write, libc::SYS_readv, libc::SYS_writev, libc::SYS_pread64,
            libc::SYS_preadv, libc::SYS_lseek, libc::SYS_close, libc::SYS_close_range, libc::SYS_openat,
            libc::SYS_statx, libc::SYS_newfstatat, libc::SYS_fstat, libc::SYS_getdents64, libc::SYS_getcwd,
            libc::SYS_readlinkat, libc::SYS_faccessat, libc::SYS_faccessat2, libc::SYS_fcntl, libc::SYS_dup,
            libc::SYS_dup3, libc::SYS_ioctl, libc::SYS_ppoll,
            libc::SYS_mmap, libc::SYS_munmap, libc::SYS_mprotect, libc::SYS_mremap, libc::SYS_madvise,
            libc::SYS_brk, libc::SYS_mincore, libc::SYS_msync, libc::SYS_membarrier, libc::SYS_get_mempolicy,
            libc::SYS_set_mempolicy, libc::SYS_mbind,
            libc::SYS_clone, libc::SYS_clone3, libc::SYS_futex, libc::SYS_set_robust_list, libc::SYS_rseq,
            libc::SYS_set_tid_address, libc::SYS_gettid, libc::SYS_sched_yield, libc::SYS_sched_getaffinity,
            libc::SYS_sched_setaffinity, libc::SYS_sched_getparam, libc::SYS_sched_getscheduler,
            libc::SYS_sched_get_priority_max, libc::SYS_sched_get_priority_min, libc::SYS_getpriority,
            libc::SYS_getcpu, libc::SYS_exit, libc::SYS_exit_group, libc::SYS_restart_syscall,
            libc::SYS_nanosleep, libc::SYS_clock_nanosleep, libc::SYS_clock_gettime, libc::SYS_clock_getres,
            libc::SYS_gettimeofday, libc::SYS_times, libc::SYS_getrusage,
            libc::SYS_sigaltstack, libc::SYS_rt_sigaction, libc::SYS_rt_sigprocmask, libc::SYS_rt_sigreturn,
            libc::SYS_rt_sigtimedwait, libc::SYS_kill, libc::SYS_tgkill,
            libc::SYS_getpid, libc::SYS_getppid, libc::SYS_getuid, libc::SYS_geteuid, libc::SYS_getgid,
            libc::SYS_getegid, libc::SYS_getresuid, libc::SYS_getresgid, libc::SYS_uname, libc::SYS_sysinfo,
            libc::SYS_getrandom, libc::SYS_prlimit64, libc::SYS_prctl,
        ];
        #[cfg(target_arch = "x86_64")]
        calls.extend([
            libc::SYS_open, libc::SYS_stat, libc::SYS_lstat, libc::SYS_access, libc::SYS_readlink,
            libc::SYS_dup2, libc::SYS_poll, libc::SYS_time, libc::SYS_arch_prctl, libc::SYS_getrlimit,
            libc::SYS_setrlimit,
        ]);
        calls
    }

    fn build(rules: Rules, mismatch: SeccompAction, matched: SeccompAction, arch: TargetArch) -> Result<BpfProgram, String> {
        let filter = SeccompFilter::new(rules, mismatch, matched, arch).map_err(|e| format!("{e:?}"))?;
        filter.try_into().map_err(|e: seccompiler::BackendError| format!("{e:?}"))
    }

    pub fn apply() -> Result<(), String> {
        let arch: TargetArch = std::env::consts::ARCH.try_into().map_err(|e| format!("{e:?}"))?;
        // SAFETY: getpid cannot fail.
        let pid = unsafe { libc::getpid() } as u64;
        // Three filters; the kernel applies the strictest answer of all of them.
        // 1. Refusals with EPERM: calls the worker or its runtime may try and must survive (the
        //    probe and the self-tests rely on it), with their argument rules.
        let mut refusals = Rules::new();
        denied(&mut refusals);
        conditional(&mut refusals, pid).map_err(|e| format!("{e:?}"))?;
        // 2. The allow-list: what the worker needs, plus everything filter 1 answers; every other
        //    call is trapped (SIGSYS, reported by the worker's handler).
        let mut allow_list: Rules = allowed().into_iter().map(|nr| (nr, Vec::new())).collect();
        for nr in refusals.keys() {
            allow_list.insert(*nr, Vec::new());
        }
        let refusals = build(refusals, SeccompAction::Allow, SeccompAction::Errno(libc::EPERM as u32), arch)?;
        let allow_list = build(allow_list, SeccompAction::Trap, SeccompAction::Allow, arch)?;
        // 3. clone3 passes its flags in memory no filter can read. ENOSYS (not EPERM) makes the C
        //    library fall back to clone, where filter 1 can tell a thread from a fork.
        let clone3 = build(
            BTreeMap::from([(libc::SYS_clone3, Vec::new())]),
            SeccompAction::Allow,
            SeccompAction::Errno(libc::ENOSYS as u32),
            arch,
        )?;
        // The allow-list goes last: installing a filter needs prctl and seccomp, which it traps.
        seccompiler::apply_filter(&clone3).map_err(|e| format!("{e:?}"))?;
        seccompiler::apply_filter(&refusals).map_err(|e| format!("{e:?}"))?;
        seccompiler::apply_filter(&allow_list).map_err(|e| format!("{e:?}"))
    }
}

/// The OCCT part of the worker. Runs inside the sandbox, on the file behind fd 0.
mod parse {
    #[cfg(feature = "step-preview")]
    use super::*;

    #[cfg(feature = "step-preview")]
    const INPUT: &str = "/proc/self/fd/0";

    #[cfg(feature = "step-preview")]
    fn check_input() -> Result<&'static std::path::Path, i32> {
        let path = std::path::Path::new(INPUT);
        let meta = std::fs::metadata(path).map_err(|_| EXIT_NOT_REGULAR)?;
        if !meta.is_file() {
            return Err(EXIT_NOT_REGULAR);
        }
        if meta.len() > MAX_INPUT_BYTES {
            return Err(EXIT_TOO_LARGE);
        }
        Ok(path)
    }

    #[cfg(feature = "step-preview")]
    fn exit_for(e: &katalog_core::step::StepError) -> i32 {
        use katalog_core::step::StepError as E;
        match e {
            E::TooLarge { .. } => EXIT_TOO_LARGE,
            E::TooComplex { .. } | E::TooManyFaces { .. } => EXIT_TOO_COMPLEX,
            E::NoGeometry => EXIT_NO_GEOMETRY,
            E::ReadFailed | E::TessellationFailed => EXIT_UNREADABLE,
            // An I/O error is about the handle or the sandbox, not about the file's content.
            E::Io(_) => EXIT_INPUT,
        }
    }

    #[cfg(feature = "step-preview")]
    pub fn geometry() -> Result<Vec<u8>, i32> {
        let path = check_input()?;
        let meshes = katalog_core::step::parse_step_geometry(path).map_err(|e| exit_for(&e))?;
        Ok(katalog_core::read::encode_render_meshes(&meshes))
    }

    #[cfg(feature = "step-preview")]
    pub fn metadata() -> Result<Vec<u8>, i32> {
        let path = check_input()?;
        let doc = katalog_core::step::parse_step_file(path).map_err(|e| exit_for(&e))?;
        let meta = StepMetadata {
            dimensions_mm: doc.dimensions_mm,
            volume_cm3: doc.volume_cm3,
            object_count: Some(doc.object_count as i64),
        };
        serde_json::to_vec(&meta).map_err(|_| EXIT_OUTPUT)
    }

    #[cfg(not(feature = "step-preview"))]
    pub fn geometry() -> Result<Vec<u8>, i32> {
        Err(super::EXIT_UNSUPPORTED)
    }

    #[cfg(not(feature = "step-preview"))]
    pub fn metadata() -> Result<Vec<u8>, i32> {
        Err(super::EXIT_UNSUPPORTED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_disabled_worker_reports_unavailable_without_starting_anything() {
        let worker = StepWorker::disabled();
        assert!(!worker.available());
        let file = File::open("/dev/null").unwrap();
        assert_eq!(worker.run(Mode::Geometry, &file, Duration::from_secs(1)), Err(WorkerError::Unavailable));
    }

    #[test]
    fn worker_places_are_limited_and_come_back() {
        let worker = StepWorker::with_exe(PathBuf::from("/nonexistent"), Limits::default(), 1);
        let first = worker.try_slot().unwrap();
        assert_eq!(worker.try_slot().unwrap_err(), WorkerError::Busy);
        assert_eq!(worker.slot_within(Duration::from_millis(50)).unwrap_err(), WorkerError::Busy);
        drop(first);
        assert!(worker.try_slot().is_ok());
    }

    #[test]
    fn a_file_over_the_limit_never_starts_a_worker() {
        let dir = std::env::temp_dir().join(format!("step-worker-size-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("big.step");
        let file = File::create(&path).unwrap();
        file.set_len(MAX_INPUT_BYTES + 1).unwrap();
        // The executable does not exist: reaching the spawn would give Unavailable instead.
        let worker = StepWorker::with_exe(PathBuf::from("/nonexistent"), Limits::default(), 1);
        let file = File::open(&path).unwrap();
        assert_eq!(worker.run(Mode::Geometry, &file, Duration::from_secs(1)), Err(WorkerError::TooLarge));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn only_a_parser_verdict_says_the_file_is_unreadable() {
        use std::os::unix::process::ExitStatusExt;
        let exit = |code: i32| std::process::ExitStatus::from_raw(code << 8);
        let signal = |sig: i32| std::process::ExitStatus::from_raw(sig);
        assert_eq!(classify(exit(EXIT_UNREADABLE), true, Vec::new()), Err(WorkerError::Unreadable));
        assert_eq!(classify(exit(EXIT_NO_GEOMETRY), true, Vec::new()), Err(WorkerError::NoGeometry));
        // A crash, a panic or an exit nobody planned says nothing about the file.
        assert_eq!(classify(signal(libc::SIGSEGV), true, Vec::new()), Err(WorkerError::Crashed));
        assert_eq!(classify(signal(libc::SIGABRT), true, Vec::new()), Err(WorkerError::Crashed));
        assert_eq!(classify(exit(101), true, Vec::new()), Err(WorkerError::Crashed));
        assert_eq!(classify(exit(1), true, Vec::new()), Err(WorkerError::Crashed));
        // Broken channels are a protocol failure, also not a verdict.
        assert_eq!(classify(exit(0), false, Vec::new()), Err(WorkerError::Protocol));
        assert_eq!(classify(exit(EXIT_OUTPUT), true, Vec::new()), Err(WorkerError::Protocol));
        assert_eq!(classify(exit(EXIT_INPUT), true, Vec::new()), Err(WorkerError::Protocol));
        assert_eq!(classify(exit(EXIT_NOT_REGULAR), true, Vec::new()), Err(WorkerError::Protocol));
        // Resource limits stay resource limits.
        assert_eq!(classify(signal(libc::SIGXCPU), true, Vec::new()), Err(WorkerError::TooComplex));
        assert_eq!(classify(signal(libc::SIGKILL), true, Vec::new()), Err(WorkerError::TooComplex));
        assert_eq!(classify(exit(EXIT_NO_LANDLOCK), true, Vec::new()), Err(WorkerError::NoIsolation));
        // Stopped by the system-call filter: a crash, so nothing is concluded about the file.
        assert_eq!(classify(exit(EXIT_FILTER), true, Vec::new()), Err(WorkerError::Crashed));
        assert_eq!(classify(signal(libc::SIGSYS), true, Vec::new()), Err(WorkerError::Crashed));
        assert_eq!(refused_number(b"syscall=103\n"), "103");
        assert_eq!(refused_number(b"syscall=12x"), "12");
        assert_eq!(refused_number(b"anything else"), "?");
    }

    /// The system-call filter on its own, without Landlock: in a child copy of this test binary
    /// one thread enters only the filter and then attacks a separate process and a file. Landlock
    /// would refuse some of this too (signals from its sixth ABI, truncation from its third), so
    /// only the filter test shows that older kernels are covered.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_filter_alone_keeps_other_processes_and_files_safe() {
        const CHILD: &str = "KATALOG_STEP_FILTER_TEST_CHILD";
        if let Ok(spec) = std::env::var(CHILD) {
            let (target, path) = spec.split_once(':').unwrap();
            let path = std::ffi::CString::new(path).unwrap();
            // SAFETY: NUL-terminated path; the descriptor stays open for the attempts below.
            let fd = unsafe { libc::open(path.as_ptr(), libc::O_RDONLY) };
            assert!(fd >= 0);
            // SAFETY: prctl with integer arguments only.
            assert_eq!(unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) }, 0);
            // The pipe exists before the filter, like any descriptor a worker might hold.
            let mut pipe = [0 as libc::c_int; 2];
            // SAFETY: valid pointer to a two-element array.
            assert_eq!(unsafe { libc::pipe2(pipe.as_mut_ptr(), libc::O_CLOEXEC) }, 0);
            super::filter::apply().unwrap();
            println!("{}", super::sandbox::act_on(target.parse().unwrap(), pipe[0], Some(pipe[1])));
            println!("{}", super::sandbox::mutate(fd, &path));
            return;
        }
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("step-filter-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("victim.step");
        std::fs::write(&file, b"ISO-10303-21;\n").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
        let mut target = Command::new("sleep").arg("60").spawn().unwrap();
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "step_worker::tests::the_filter_alone_keeps_other_processes_and_files_safe", "--nocapture"])
            .env(CHILD, format!("{}:{}", target.id(), file.display()))
            .output()
            .unwrap();
        let limits = std::fs::read_to_string(format!("/proc/{}/limits", target.id())).unwrap();
        let alive = target.try_wait().unwrap().is_none();
        let _ = target.kill();
        let _ = target.wait();
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        assert!(output.status.success(), "{stdout}\n{}", String::from_utf8_lossy(&output.stderr));
        let eperm = libc::EPERM.to_string();
        let expected = [
            "kill", "kill_group", "kill_all", "tkill", "tgkill", "rt_sigqueueinfo", "rt_tgsigqueueinfo",
            "pidfd_open", "prlimit64", "sched_setaffinity", "sched_setscheduler", "setpriority", "ioprio_set",
            "kcmp", "process_vm_readv", "fcntl_setown", "fcntl_setown_ex", "fcntl_setsig",
            "fcntl_setfl_async", "pipe2", "open_rdonly_trunc", "openat_rdonly_trunc", "fchmod", "fchmodat",
            "fchown", "futimens", "utimensat", "ftruncate", "ioctl_setflags", "ioctl_fssetxattr",
        ];
        for name in expected {
            let got = stdout
                .lines()
                .find_map(|l| l.strip_prefix(&format!("{name}=")))
                .unwrap_or_else(|| panic!("{name} missing in {stdout}"));
            assert_eq!(got, eperm, "{name}: {stdout}");
        }
        assert!(alive, "the target process died");
        let open_files = limits.lines().find(|l| l.starts_with("Max open files")).unwrap();
        assert!(!open_files.split_whitespace().any(|w| w == "0"), "{open_files}");
        assert_eq!(std::fs::read(&file).unwrap(), b"ISO-10303-21;\n");
        assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o7777, 0o644);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn worker_main_refuses_unknown_modes_and_bad_numbers() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(worker_main(&args(&["geometry"])), EXIT_USAGE);
        assert_eq!(worker_main(&args(&["geometry", "x", "1", "1"])), EXIT_USAGE);
        assert_eq!(worker_main(&args(&["shell", "1", "1", "1"])), EXIT_USAGE);
        // Without the environment switch the test modes do not exist.
        assert_eq!(worker_main(&args(&["selftest-spin", "1", "1", "1"])), EXIT_USAGE);
    }
}
