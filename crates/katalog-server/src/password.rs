use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Algorithm, Argon2, Params, Version,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{BufRead, Write},
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Semaphore;

// OWASP Password Storage Cheat Sheet: minimum Argon2id configuration.
pub const ARGON_MEMORY_KIB: u32 = 19_456;
pub const ARGON_ITERATIONS: u32 = 2;
pub const ARGON_PARALLELISM: u32 = 1;
static HASH_SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();
#[cfg(test)]
pub(crate) static PASSWORD_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthFile {
    pub version: u32,
    pub password_hash: String,
    pub must_change: bool,
    pub updated_at: u64,
}

fn argon() -> Argon2<'static> {
    Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(ARGON_MEMORY_KIB, ARGON_ITERATIONS, ARGON_PARALLELISM, None)
            .expect("statische Argon2-Parameter"),
    )
}

async fn hashing<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let permit = hash_slot().await?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        work()
    })
    .await
    .map_err(|_| "Passwortprüfung fehlgeschlagen".to_owned())?
}

pub async fn hash_password(password: String) -> Result<String, String> {
    hashing(move || {
        let mut salt = [0u8; 16];
        getrandom::fill(&mut salt).map_err(|_| "Zufallsquelle nicht verfügbar".to_owned())?;
        let salt = SaltString::encode_b64(&salt)
            .map_err(|_| "Salz konnte nicht erzeugt werden".to_owned())?;
        argon()
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(|_| "Passwort konnte nicht gehasht werden".to_owned())
    })
    .await
}

pub(crate) async fn hash_slot() -> Result<tokio::sync::OwnedSemaphorePermit, String> {
    let semaphore = HASH_SLOTS
        .get_or_init(|| Arc::new(Semaphore::new(2)))
        .clone();
    tokio::time::timeout(Duration::from_millis(250), semaphore.acquire_owned())
        .await
        .map_err(|_| "Passwortprüfung ausgelastet, bitte später versuchen".to_owned())?
        .map_err(|_| "Passwortprüfung nicht verfügbar".to_owned())
}

pub async fn verify_password(hash: String, password: String) -> Result<bool, String> {
    verify_with_slot(hash, password, hash_slot().await?).await
}

pub(crate) async fn verify_with_slot(
    hash: String,
    password: String,
    permit: tokio::sync::OwnedSemaphorePermit,
) -> Result<bool, String> {
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let parsed = PasswordHash::new(&hash).map_err(|_| "Ungültiger Passwort-Hash".to_owned())?;
        match argon().verify_password(password.as_bytes(), &parsed) {
            Ok(()) => Ok(true),
            Err(argon2::password_hash::Error::Password) => Ok(false),
            Err(_) => Err("Passwortprüfung fehlgeschlagen".to_owned()),
        }
    })
    .await
    .map_err(|_| "Passwortprüfung fehlgeschlagen".to_owned())?
}

pub fn read_auth(path: &Path) -> Result<AuthFile, String> {
    let bytes = fs::read(path).map_err(|_| "Passwortdatei kann nicht gelesen werden".to_owned())?;
    let auth: AuthFile = serde_json::from_slice(&bytes)
        .map_err(|_| "Passwortdatei enthält ungültiges JSON".to_owned())?;
    if auth.version != 1 {
        return Err("Nicht unterstützte Version der Passwortdatei".into());
    }
    let hash = PasswordHash::new(&auth.password_hash)
        .map_err(|_| "Passwortdatei enthält ungültigen PHC-Hash".to_owned())?;
    if hash.algorithm.as_str() != "argon2id"
        || hash.version != Some(19)
        || hash.salt.is_none()
        || hash.hash.is_none()
        || Params::try_from(&hash).is_err()
    {
        return Err("Passwortdatei enthält keinen gültigen Argon2id-Hash".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(path)
            .map_err(|_| "Passwortdatei nicht lesbar".to_owned())?
            .permissions()
            .mode();
        if mode & 0o077 != 0 {
            eprintln!("Warnung: Passwortdatei hat zu offene Zugriffsrechte (empfohlen: 0600)");
        }
    }
    Ok(auth)
}

// POSIX permission checks include ownership, ACLs and mount restrictions without
// creating a probe file. Linux AT_EACCESS uses the effective credentials used by I/O.
#[cfg(unix)]
fn directory_writable(path: &Path) -> bool {
    use std::{
        ffi::{c_char, c_int, CString},
        os::unix::ffi::OsStrExt,
    };
    let Ok(path) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    #[cfg(target_os = "linux")]
    {
        unsafe extern "C" {
            fn faccessat(dirfd: c_int, pathname: *const c_char, mode: c_int, flags: c_int)
                -> c_int;
        }
        // AT_FDCWD=-100, W_OK|X_OK=3, AT_EACCESS=0x200 from Linux/POSIX fcntl.h.
        unsafe { faccessat(-100, path.as_ptr(), 3, 0x200) == 0 }
    }
    #[cfg(not(target_os = "linux"))]
    {
        unsafe extern "C" {
            fn access(pathname: *const c_char, mode: c_int) -> c_int;
            fn getuid() -> u32;
            fn geteuid() -> u32;
            fn getgid() -> u32;
            fn getegid() -> u32;
        }
        // access checks real IDs; refuse elevated/set-ID contexts rather than
        // incorrectly claiming effective write access on other Unix platforms.
        unsafe { getuid() == geteuid() && getgid() == getegid() && access(path.as_ptr(), 3) == 0 }
    }
}

#[cfg(unix)]
fn sticky_allows_replace(
    mode: u32,
    directory_uid: u32,
    target_uid: u32,
    effective_uid: u32,
) -> bool {
    mode & 0o1000 == 0
        || effective_uid == 0
        || effective_uid == directory_uid
        || effective_uid == target_uid
}

/// Checks existing paths without creating files; actual atomic replacement remains fallible.
pub fn validate_writable(path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let metadata = fs::metadata(parent).map_err(|_| {
        "Verzeichnis der Passwortdatei existiert nicht oder ist nicht zugänglich".to_owned()
    })?;
    if !metadata.is_dir() || metadata.permissions().readonly() {
        return Err("Verzeichnis der Passwortdatei ist nicht beschreibbar".into());
    }
    #[cfg(unix)]
    if !directory_writable(parent) {
        return Err("Verzeichnis der Passwortdatei ist nicht beschreibbar".into());
    }
    if path.exists() {
        let meta = fs::metadata(path).map_err(|_| "Passwortdatei nicht zugänglich".to_owned())?;
        if !meta.is_file() || meta.permissions().readonly() {
            return Err("Passwortdatei ist nicht beschreibbar".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            unsafe extern "C" {
                fn geteuid() -> u32;
            }
            // Rename replaces the directory entry, including a symlink itself.
            let entry = fs::symlink_metadata(path)
                .map_err(|_| "Passwortdatei nicht zugänglich".to_owned())?;
            // geteuid takes no pointers and has no failure mode.
            let effective_uid = unsafe { geteuid() };
            if !sticky_allows_replace(metadata.mode(), metadata.uid(), entry.uid(), effective_uid) {
                return Err(
                    "Passwortdatei kann im geschützten Verzeichnis nicht ersetzt werden".into(),
                );
            }
        }
        OpenOptions::new()
            .write(true)
            .open(path)
            .map_err(|_| "Passwortdatei ist nicht beschreibbar".to_owned())?;
    }
    Ok(())
}

pub fn write_auth(path: &Path, auth: &AuthFile) -> Result<(), String> {
    validate_writable(path)?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut random = [0; 16];
    getrandom::fill(&mut random).map_err(|_| "Zufallsquelle nicht verfügbar".to_owned())?;
    let temporary = parent.join(format!(
        ".katalog-auth-{:032x}.tmp",
        u128::from_ne_bytes(random)
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|_| "Temporäre Passwortdatei konnte nicht angelegt werden".to_owned())?;
    let result = (|| {
        serde_json::to_writer(&mut file, auth).map_err(std::io::Error::other)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|_| "Passwort konnte nicht gespeichert werden".to_owned())
}

pub fn strip_line_ending(mut value: String) -> String {
    if value.ends_with('\n') {
        value.pop();
        if value.ends_with('\r') {
            value.pop();
        }
    }
    value
}

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub async fn bootstrap(
    path: &Path,
    password: Option<String>,
    password_file: Option<PathBuf>,
) -> Result<AuthFile, String> {
    if password.is_some() && password_file.is_some() {
        return Err(
            "KATALOG_PASSWORD und KATALOG_PASSWORD_FILE dürfen nicht gleichzeitig gesetzt sein"
                .into(),
        );
    }
    let check_path = path.to_path_buf();
    let (existing, supplied) = file_work(move || {
        if check_path
            .try_exists()
            .map_err(|_| "Passwortdatei nicht zugänglich".to_owned())?
        {
            let auth = read_auth(&check_path)?;
            validate_writable(&check_path)?;
            if password.is_some() || password_file.is_some() {
                eprintln!("KATALOG_PASSWORD(_FILE) ignoriert: nur zur Ersteinrichtung");
            }
            return Ok((Some(auth), None));
        }
        validate_writable(&check_path)?;
        let supplied = match password_file {
            Some(p) => Some(strip_line_ending(fs::read_to_string(p).map_err(|_| {
                "KATALOG_PASSWORD_FILE kann nicht gelesen werden".to_owned()
            })?)),
            None => password,
        };
        Ok((None, supplied))
    })
    .await?;
    if let Some(auth) = existing {
        return Ok(auth);
    }
    // A short supplied password is accepted for setup but must be replaced on first login.
    let short_supplied = supplied.as_ref().is_some_and(|v| v.chars().count() < 12);
    let generated = supplied.is_none();
    let must_change = generated || short_supplied;
    let plaintext = if let Some(value) = supplied {
        if short_supplied {
            eprintln!("Warnung: Das übernommene Passwort hat weniger als 12 Zeichen; es muss beim ersten Anmelden geändert werden");
        }
        value
    } else {
        random_password()?
    };
    let auth = AuthFile {
        version: 1,
        password_hash: hash_password(plaintext.clone()).await?,
        must_change,
        updated_at: unix_now(),
    };
    let write_path = path.to_path_buf();
    let persisted = auth.clone();
    file_work(move || write_auth(&write_path, &persisted)).await?;
    // A supplied password is never echoed into the log.
    if generated {
        eprintln!("\n=== EINMALIGES STARTPASSWORT ===\n{plaintext}\nBitte nach der Anmeldung sofort ändern.\n==============================\n");
    }
    Ok(auth)
}

fn random_password() -> Result<String, String> {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
    let mut password = String::with_capacity(20);
    while password.len() < 20 {
        let mut byte = [0];
        getrandom::fill(&mut byte).map_err(|_| "Zufallsquelle nicht verfügbar".to_owned())?;
        // Rejection sampling avoids modulo bias.
        let bound = 256 - (256 % ALPHABET.len());
        if usize::from(byte[0]) < bound {
            password.push(ALPHABET[usize::from(byte[0]) % ALPHABET.len()] as char);
        }
    }
    Ok(password)
}

pub fn policy(new: &str, old: Option<&str>) -> Result<(), String> {
    const TRIVIAL: &[&str] = &[
        "passwort1234",
        "123456789012",
        "password1234",
        "password12345",
        "password123456",
        "passwort12345",
        "passwort123456",
        "qwerty1234567",
        "qwertz1234567",
        "1234567890123",
        "12345678901234",
        "123456789012345",
        "abcdefghijkl",
        "abcdefghijklmnop",
        "admin12345678",
        "administrator",
        "letmein123456",
        "welcome123456",
        "willkommen123",
        "changeme12345",
        "iloveyou12345",
        "passwordpassword",
        "passwortpasswort",
        "123456123456",
    ];
    if !(12..=128).contains(&new.chars().count()) {
        return Err("Das neue Passwort muss 12 bis 128 Zeichen lang sein".into());
    }
    if old == Some(new) {
        return Err("Das neue Passwort muss sich vom bisherigen unterscheiden".into());
    }
    if new.chars().all(|c| new.starts_with(c)) || TRIVIAL.contains(&new.to_lowercase().as_str()) {
        return Err("Das neue Passwort ist zu leicht zu erraten".into());
    }
    Ok(())
}

pub async fn set_password(
    path: &Path,
    mut input: impl BufRead,
    is_terminal: bool,
) -> Result<(), String> {
    let check_path = path.to_path_buf();
    let existing = file_work(move || {
        let existing = if check_path
            .try_exists()
            .map_err(|_| "Passwortdatei nicht zugänglich".to_owned())?
        {
            Some(read_auth(&check_path)?)
        } else {
            None
        };
        validate_writable(&check_path)?;
        Ok(existing)
    })
    .await?;
    let mut first = String::new();
    if is_terminal {
        eprintln!("Neues Passwort (Eingabe sichtbar):");
    }
    input
        .read_line(&mut first)
        .map_err(|_| "Passwort konnte nicht gelesen werden".to_owned())?;
    let first = strip_line_ending(first);
    if is_terminal {
        eprintln!("Neues Passwort wiederholen:");
        let mut second = String::new();
        input
            .read_line(&mut second)
            .map_err(|_| "Passwort konnte nicht gelesen werden".to_owned())?;
        if first != strip_line_ending(second) {
            return Err("Die Passwörter stimmen nicht überein".into());
        }
    }
    policy(&first, None)?;
    if let Some(existing) = existing {
        if verify_password(existing.password_hash, first.clone()).await? {
            return Err("Das neue Passwort muss sich vom bisherigen unterscheiden".into());
        }
    }
    let auth = AuthFile {
        version: 1,
        password_hash: hash_password(first).await?,
        must_change: false,
        updated_at: unix_now(),
    };
    let write_path = path.to_path_buf();
    file_work(move || write_auth(&write_path, &auth)).await
}

async fn file_work<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| "Passwortdatei konnte nicht verarbeitet werden".to_owned())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    fn path() -> PathBuf {
        let mut bytes = [0; 16];
        getrandom::fill(&mut bytes).unwrap();
        let dir =
            std::env::temp_dir().join(format!("katalog-auth-{:x}", u128::from_ne_bytes(bytes)));
        fs::create_dir(&dir).unwrap();
        dir.join("auth.json")
    }
    const PASSWORD: &str = "Sicheres Testpasswort 42!";
    #[test]
    fn policy_and_line_endings() {
        for bad in ["kurz", "aaaaaaaaaaaa", "123456789012", "password123456"] {
            assert!(policy(bad, None).is_err());
        }
        assert!(policy(&"a".repeat(129), None).is_err());
        assert!(policy(PASSWORD, Some(PASSWORD)).is_err());
        assert!(policy(PASSWORD, None).is_ok());
        assert_eq!(strip_line_ending(" secret  \r\n".into()), " secret  ");
        assert_eq!(strip_line_ending("secret\n\n".into()), "secret\n");
    }
    #[tokio::test]
    async fn hash_parameters_salts_and_verification() {
        let _guard = PASSWORD_TEST_LOCK.lock().await;
        let first = hash_password(PASSWORD.into()).await.unwrap();
        let second = hash_password(PASSWORD.into()).await.unwrap();
        assert_ne!(first, second);
        assert!(first.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"));
        assert!(verify_password(first.clone(), PASSWORD.into())
            .await
            .unwrap());
        assert!(!verify_password(first, String::new()).await.unwrap());
    }
    #[tokio::test]
    async fn bootstrap_and_atomic_file() {
        let _guard = PASSWORD_TEST_LOCK.lock().await;
        let p = path();
        let auth = bootstrap(&p, Some(PASSWORD.into()), None).await.unwrap();
        assert!(!auth.must_change);
        assert_eq!(read_auth(&p).unwrap(), auth);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&p).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert_eq!(
            bootstrap(&p, Some("ignored".into()), None).await.unwrap(),
            auth
        );
        let before = fs::read(&p).unwrap();
        assert!(write_auth(&p.join("impossible"), &auth).is_err());
        assert_eq!(before, fs::read(&p).unwrap());
        assert_eq!(fs::read_dir(p.parent().unwrap()).unwrap().count(), 1);
        let mut invalid = auth;
        invalid.version = 2;
        fs::write(&p, serde_json::to_vec(&invalid).unwrap()).unwrap();
        assert!(read_auth(&p).is_err());
        fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }
    #[tokio::test]
    async fn short_supplied_password_requires_change_on_first_login() {
        let _guard = PASSWORD_TEST_LOCK.lock().await;
        let p = path();
        let short = bootstrap(&p, Some("kurz".into()), None).await.unwrap();
        assert!(short.must_change);
        assert!(verify_password(short.password_hash.clone(), "kurz".into()).await.unwrap());
        fs::remove_dir_all(p.parent().unwrap()).unwrap();
        let p = path();
        let exactly_twelve = bootstrap(&p, Some("123456789012".into()), None).await.unwrap();
        assert!(!exactly_twelve.must_change);
        fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }
    #[tokio::test]
    async fn bootstrap_file_random_and_set_password() {
        let _guard = PASSWORD_TEST_LOCK.lock().await;
        let p = path();
        let input_path = p.with_file_name("input");
        fs::write(&input_path, " short \r\n").unwrap();
        let auth = bootstrap(&p, None, Some(input_path.clone())).await.unwrap();
        assert!(verify_password(auth.password_hash, " short ".into())
            .await
            .unwrap());
        fs::remove_file(&p).unwrap();
        assert!(bootstrap(&p, Some("a".into()), Some(input_path))
            .await
            .is_err());
        assert!(!p.exists());
        let generated = bootstrap(&p, None, None).await.unwrap();
        assert!(generated.must_change);
        assert!(
            set_password(&p, Cursor::new(format!("{PASSWORD}\nwrong\n")), true)
                .await
                .is_err()
        );
        assert_eq!(read_auth(&p).unwrap(), generated);
        set_password(&p, Cursor::new(format!("{PASSWORD}\n")), false)
            .await
            .unwrap();
        let changed = read_auth(&p).unwrap();
        assert!(!changed.must_change);
        let before_same = fs::read(&p).unwrap();
        assert!(
            set_password(&p, Cursor::new(format!("{PASSWORD}\n")), false)
                .await
                .is_err()
        );
        assert_eq!(fs::read(&p).unwrap(), before_same);
        assert!(verify_password(changed.password_hash, PASSWORD.into())
            .await
            .unwrap());
        fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }
    #[tokio::test]
    async fn set_password_never_replaces_invalid_existing_file() {
        let p = path();
        fs::write(&p, b"not an auth file").unwrap();
        assert!(
            set_password(&p, Cursor::new(format!("{PASSWORD}\n")), false)
                .await
                .is_err()
        );
        assert_eq!(fs::read(&p).unwrap(), b"not an auth file");
        fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn startup_checks_directory_search_and_actual_write_access_without_probes() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let p = path();
        let parent = p.parent().unwrap();
        let uid = fs::metadata(parent).unwrap().uid();
        fs::set_permissions(parent, fs::Permissions::from_mode(0o222)).unwrap();
        if uid != 0 {
            // Write bits alone are insufficient: directory search permission is required.
            assert!(!directory_writable(parent));
            assert!(validate_writable(&p).is_err());
            // A normal user must not inherit root's write bit from a 0755 directory.
            if fs::metadata("/").unwrap().uid() == 0
                && fs::metadata("/").unwrap().permissions().mode() & 0o022 == 0
            {
                assert!(!directory_writable(Path::new("/")));
                assert!(
                    validate_writable(Path::new("/katalog-auth-permission-test.json")).is_err()
                );
            }
        }
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(fs::read_dir(parent).unwrap().count(), 0);
        fs::remove_dir(parent).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn sticky_directory_replacement_requires_owner_or_root() {
        assert!(!sticky_allows_replace(0o1777, 100, 200, 300));
        assert!(sticky_allows_replace(0o1777, 100, 200, 0));
        assert!(sticky_allows_replace(0o1777, 100, 200, 100));
        assert!(sticky_allows_replace(0o1777, 100, 200, 200));
        assert!(sticky_allows_replace(0o0777, 100, 200, 300));
    }
}
