//! STEP preview in a separate, self-limiting process.
//!
//! STEP files come from strangers and are parsed by a large C++ library. A crash, an endless
//! loop or a runaway allocation in it must not take the server down, so the server never calls
//! OCCT itself: it starts its own binary as `katalog-server step-worker <mode> ...` with the
//! admitted file as standard input and reads the result from standard output. The worker first
//! limits itself (resource limits, no core dumps, dies with its parent, a seccomp filter against
//! network, exec, process control and writing files) and only then touches the input. The
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

/// Lets the test-only modes run. The server starts workers with an empty environment, so a
/// request can never reach them.
pub const SELFTEST_ENV: &str = "KATALOG_STEP_WORKER_SELFTEST";

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
    /// The parser rejected the file or died on it.
    Unreadable,
    /// This build or this host cannot run the worker.
    Unavailable,
    /// All worker places are taken.
    Busy,
}

/// The server side: starts workers and turns their exit into a [`WorkerError`].
pub struct StepWorker {
    /// `None`: no preview (built without OCCT, or the sandbox does not work on this host).
    exe: Option<PathBuf>,
    limits: Limits,
    slots: Arc<Semaphore>,
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
        Self { exe: None, limits: Limits::default(), slots: Arc::new(Semaphore::new(Self::PARALLEL)) }
    }

    /// A worker using `exe` without checking it first; tests use it with the real binary.
    pub fn with_exe(exe: PathBuf, limits: Limits, parallel: usize) -> Self {
        Self { exe: Some(exe), limits, slots: Arc::new(Semaphore::new(parallel)) }
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
        let meta = input.metadata().map_err(|_| WorkerError::Unreadable)?;
        if !meta.is_file() {
            return Err(WorkerError::Unreadable);
        }
        if meta.len() > MAX_INPUT_BYTES {
            return Err(WorkerError::TooLarge);
        }
        // A second handle on the same open file: the worker reopens it through /proc, so the
        // shared offset does not matter.
        let stdin = input.try_clone().map_err(|_| WorkerError::Unreadable)?;
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
            // OCCT and the C++ runtime talk on stderr; it is not ours to forward.
            .stderr(Stdio::null());
        let mut child = command.spawn().map_err(|_| WorkerError::Unavailable)?;
        let mut stdout = child.stdout.take().ok_or(WorkerError::Unavailable)?;
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
        if out.len() > cap {
            return Err(WorkerError::TooComplex);
        }
        classify(status, read_ok, out)
    }
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
            return Err(WorkerError::Unreadable);
        }
    }
    match status.code() {
        Some(0) if read_ok => Ok(out),
        Some(0) => Err(WorkerError::Unreadable),
        Some(EXIT_TOO_LARGE) => Err(WorkerError::TooLarge),
        Some(EXIT_TOO_COMPLEX) => Err(WorkerError::TooComplex),
        Some(EXIT_NO_GEOMETRY) => Err(WorkerError::NoGeometry),
        Some(EXIT_UNSUPPORTED | EXIT_SANDBOX | EXIT_USAGE) => Err(WorkerError::Unavailable),
        // Unreadable, not a regular file, failed to write, or an exit we did not plan (for example
        // a Rust panic or an allocation failure under the address-space limit).
        _ => Err(WorkerError::Unreadable),
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
        if enter(address_space_mib, cpu_seconds, parent).is_err() {
            return EXIT_SANDBOX;
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

    #[cfg(target_env = "gnu")]
    type Resource = libc::__rlimit_resource_t;
    #[cfg(not(target_env = "gnu"))]
    type Resource = libc::c_int;

    fn set_limit(resource: Resource, soft: u64, hard: u64) -> Result<(), ()> {
        let limit = libc::rlimit { rlim_cur: soft as libc::rlim_t, rlim_max: hard as libc::rlim_t };
        // SAFETY: plain syscall with a valid pointer to a local struct.
        if unsafe { libc::setrlimit(resource, &limit) } == 0 { Ok(()) } else { Err(()) }
    }

    fn enter(address_space_mib: u64, cpu_seconds: u64, parent: u32) -> Result<(), ()> {
        // SAFETY: prctl with integer arguments only.
        if unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL as libc::c_ulong, 0, 0, 0) } != 0 {
            return Err(());
        }
        // The parent may have died before the line above; then nobody waits for the answer. The
        // server names itself, because in a container it often is PID 1 and "reparented to 1"
        // cannot be told from "started by 1".
        // SAFETY: getppid cannot fail.
        if unsafe { libc::getppid() } as u32 != parent {
            return Err(());
        }
        set_limit(libc::RLIMIT_CORE, 0, 0)?;
        // The worker writes no files; pipes and /dev/null are not affected by this limit.
        set_limit(libc::RLIMIT_FSIZE, 0, 0)?;
        set_limit(libc::RLIMIT_NOFILE, 32, 32)?;
        let bytes = address_space_mib.saturating_mul(1024 * 1024);
        set_limit(libc::RLIMIT_AS, bytes, bytes)?;
        // Soft limit sends SIGXCPU, the hard one a second later SIGKILL.
        set_limit(libc::RLIMIT_CPU, cpu_seconds, cpu_seconds.saturating_add(1))?;
        super::filter::apply().map_err(|_| ())
    }

    /// Exits 0 only if the sandbox blocks what it must and still allows threads.
    fn probe() -> i32 {
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
}

#[cfg(not(target_os = "linux"))]
mod sandbox {
    /// Without the Linux limits the worker would parse unprotected; it refuses instead.
    pub fn run(_mode: &str, _address_space_mib: u64, _cpu_seconds: u64, _parent: u32) -> i32 {
        super::EXIT_SANDBOX
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
        ];
        for nr in always {
            rules.insert(nr, Vec::new());
        }
        #[cfg(target_arch = "x86_64")]
        for nr in [
            libc::SYS_creat, libc::SYS_mkdir, libc::SYS_rmdir, libc::SYS_unlink, libc::SYS_rename,
            libc::SYS_renameat, libc::SYS_link, libc::SYS_symlink, libc::SYS_chmod, libc::SYS_chown,
            libc::SYS_lchown, libc::SYS_mknod, libc::SYS_fork, libc::SYS_vfork,
        ] {
            rules.insert(nr, Vec::new());
        }
    }

    /// Opening for writing or creating, by flag argument `flags_arg` of the open call.
    fn writing_opens(flags_arg: u8) -> Result<Vec<SeccompRule>, seccompiler::BackendError> {
        let accmode = libc::O_ACCMODE as u64;
        Ok(vec![
            rule(vec![Cond::new(flags_arg, Len::Dword, Op::MaskedEq(accmode), libc::O_WRONLY as u64)?])?,
            rule(vec![Cond::new(flags_arg, Len::Dword, Op::MaskedEq(accmode), libc::O_RDWR as u64)?])?,
            rule(vec![Cond::new(flags_arg, Len::Dword, Op::MaskedEq(libc::O_CREAT as u64), libc::O_CREAT as u64)?])?,
        ])
    }

    fn conditional(rules: &mut Rules, pid: u64) -> Result<(), seccompiler::BackendError> {
        rules.insert(libc::SYS_openat, writing_opens(2)?);
        #[cfg(target_arch = "x86_64")]
        rules.insert(libc::SYS_open, writing_opens(1)?);
        // Signals only to itself: abort() and the runtime use them, the server must not get one.
        rules.insert(libc::SYS_kill, vec![rule(vec![Cond::new(0, Len::Dword, Op::Ne, pid)?])?]);
        rules.insert(libc::SYS_tgkill, vec![rule(vec![Cond::new(0, Len::Dword, Op::Ne, pid)?])?]);
        // Threads yes, new processes no: a clone without CLONE_THREAD is a fork.
        rules.insert(
            libc::SYS_clone,
            vec![rule(vec![Cond::new(0, Len::Qword, Op::MaskedEq(libc::CLONE_THREAD as u64), 0)?])?],
        );
        Ok(())
    }

    fn build(rules: Rules, action: SeccompAction, arch: TargetArch) -> Result<BpfProgram, String> {
        let filter = SeccompFilter::new(rules, SeccompAction::Allow, action, arch).map_err(|e| format!("{e:?}"))?;
        filter.try_into().map_err(|e: seccompiler::BackendError| format!("{e:?}"))
    }

    pub fn apply() -> Result<(), String> {
        let arch: TargetArch = std::env::consts::ARCH.try_into().map_err(|e| format!("{e:?}"))?;
        // SAFETY: getpid cannot fail.
        let pid = unsafe { libc::getpid() } as u64;
        let mut rules = Rules::new();
        denied(&mut rules);
        conditional(&mut rules, pid).map_err(|e| format!("{e:?}"))?;
        let main = build(rules, SeccompAction::Errno(libc::EPERM as u32), arch)?;
        // clone3 passes its flags in memory the filter cannot read. ENOSYS (not EPERM) makes the C
        // library fall back to clone, where the rule above can tell a thread from a fork.
        let clone3 = build(
            BTreeMap::from([(libc::SYS_clone3, Vec::new())]),
            SeccompAction::Errno(libc::ENOSYS as u32),
            arch,
        )?;
        seccompiler::apply_filter(&clone3).map_err(|e| format!("{e:?}"))?;
        seccompiler::apply_filter(&main).map_err(|e| format!("{e:?}"))
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
            E::Io(_) | E::ReadFailed | E::TessellationFailed => EXIT_UNREADABLE,
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
