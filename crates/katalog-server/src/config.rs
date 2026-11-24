use std::{
    net::{IpAddr, Ipv6Addr, SocketAddr},
    path::{Path, PathBuf},
};

// Betrieb: Cookies (katalog_session) sind nicht nach Port getrennt. Andere
// HTTP-Dienste auf demselben Host erhalten sie; eigenen Host-/Domainnamen nutzen.
// Das Bootstrap-Zufallspasswort bleibt dauerhaft im Log (docker logs) und sollte
// nach dem ersten Anmelden geändert werden. set-password liest sichtbar im
// Terminal; ein laufender Server übernimmt die Datei erst nach einem Neustart.
// Ein vertrauenswürdiger Proxy muss X-Forwarded-For anhängen oder überschreiben.
// Forwarded-Header zählen nur, wenn der direkte Peer in KATALOG_TRUSTED_PROXIES steht.

/// IP addresses and CIDR networks (IPv4 and IPv6) of the reverse proxies whose
/// forwarded headers are believed. An IPv4-mapped IPv6 address counts as IPv4.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrustedProxies(Vec<(IpAddr, u8)>);

fn canonical_ip(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(ip),
        v4 => v4,
    }
}

impl TrustedProxies {
    /// Comma-separated `IP` or `IP/prefix`. Any invalid entry, an empty entry, an
    /// empty list or a `/0` network (would trust the whole internet) is an error.
    pub fn parse(list: &str) -> Result<Self, String> {
        let mut nets = Vec::new();
        for entry in list.split(',') {
            let entry = entry.trim();
            let bad = || format!("KATALOG_TRUSTED_PROXIES: ungültiger Eintrag \"{entry}\" (erwartet IP oder IP/Präfix, kommagetrennt)");
            let (addr, prefix) = match entry.split_once('/') {
                Some((a, p)) => (a, Some(p)),
                None => (entry, None),
            };
            let ip = canonical_ip(addr.trim().parse::<IpAddr>().map_err(|_| bad())?);
            let was_mapped = addr.trim().parse::<IpAddr>().is_ok_and(|a| a != ip);
            let max: u8 = if ip.is_ipv4() { 32 } else { 128 };
            let prefix = match prefix {
                None => max,
                Some(p) if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) => {
                    let p: u8 = p.parse().map_err(|_| bad())?;
                    // A mapped IPv6 prefix counts from the v6 start; shorten it to the v4 part.
                    if was_mapped { p.checked_sub(96).ok_or_else(bad)? } else { p }
                }
                Some(_) => return Err(bad()),
            };
            if prefix == 0 || prefix > max {
                return Err(format!("KATALOG_TRUSTED_PROXIES: Präfix von \"{entry}\" außerhalb 1..={max}"));
            }
            nets.push((ip, prefix));
        }
        Ok(Self(nets))
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn contains(&self, ip: IpAddr) -> bool {
        let ip = canonical_ip(ip);
        self.0.iter().any(|(net, prefix)| match (net, ip) {
            (IpAddr::V4(n), IpAddr::V4(i)) => {
                let mask = u32::MAX << (32 - u32::from(*prefix));
                u32::from(*n) & mask == u32::from(i) & mask
            }
            (IpAddr::V6(n), IpAddr::V6(i)) => {
                let mask = u128::MAX << (128 - u32::from(*prefix));
                u128::from(*n) & mask == u128::from(i) & mask
            }
            _ => false,
        })
    }
}
#[derive(Clone)]
pub struct UploadConfig {
    pub dir: PathBuf,
    pub max_bytes: u64,
    pub max_files: usize,
    /// Upper bound for what ONE uploaded archive may unpack to.
    pub archive_max_bytes: u64,
    pub limits: katalog_core::import::files::WebImportLimits,
}

#[derive(Clone)]
pub struct Config {
    pub db_path: PathBuf,
    pub static_dir: Option<PathBuf>,
    pub roots: Vec<PathBuf>,
    pub source_roots: Vec<PathBuf>,
    pub printer_allow: PrinterAllow,
    pub auth_file: PathBuf,
    pub bind: SocketAddr,
    pub allowed_hosts: Option<Vec<String>>,
    pub trust_proxy: bool,
    /// Direct peers whose forwarded headers are believed; non-empty whenever `trust_proxy` is set.
    pub trusted_proxies: TrustedProxies,
    pub allow_no_auth: bool,
    pub write: bool,
    pub file_ops: bool,
    pub file_ops_daily_limit: u64,
    /// Largest single companion file the web accepts and serves, in bytes.
    pub companion_max_bytes: u64,
    pub file_ops_ack: Vec<String>,
    pub file_roots: Vec<PathBuf>,
    /// Lifetime in seconds of "open in slicer" links; `None` leaves the feature off.
    pub slicer_links: Option<u64>,
    pub upload: Option<UploadConfig>,
    pub password: Option<String>,
    pub password_file: Option<PathBuf>,
}

impl Config {
    /// Beschreibt den tatsächlich aktiven Betriebsmodus für die Startmeldung.
    pub fn mode_description(&self) -> String {
        if !self.write {
            return "nur lesend".into();
        }
        let mut mode = String::from("Schreibmodus");
        if self.upload.is_some() {
            mode.push_str(" mit Upload");
        }
        if self.file_ops {
            mode.push_str(if self.upload.is_some() { " und Dateiverwaltung" } else { " mit Dateiverwaltung" });
        }
        mode
    }
    pub fn from_env() -> Result<Self, String> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
        let required = |key: &str| {
            lookup(key)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| format!("{key} muss gesetzt sein"))
        };
        let static_dir = lookup("KATALOG_STATIC_DIR")
            .map(|value| {
                if value.is_empty() {
                    return Err("KATALOG_STATIC_DIR darf nicht leer sein".to_string());
                }
                let path = PathBuf::from(value)
                    .canonicalize()
                    .map_err(|_| "KATALOG_STATIC_DIR ist nicht lesbar".to_string())?;
                if !path.is_dir() {
                    return Err("KATALOG_STATIC_DIR ist kein Verzeichnis".to_string());
                }
                Ok(path)
            })
            .transpose()?;
        let db_path = PathBuf::from(required("KATALOG_DB")?);
        let root_text = required("KATALOG_ROOTS")?;
        let mut roots: Vec<PathBuf> = std::env::split_paths(&root_text).collect();
        if roots.is_empty() || roots.iter().any(|p| p.as_os_str().is_empty()) {
            return Err("KATALOG_ROOTS enthält eine leere Wurzel".into());
        }
        let file_roots = roots.clone();
        let parent = db_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| std::path::Path::new("."));
        let trash = parent.join("trash");
        if trash.is_dir() && !roots.contains(&trash) {
            roots.push(trash);
        }
        let auth_file = lookup("KATALOG_AUTH_FILE")
            .map(PathBuf::from)
            .unwrap_or_else(|| parent.join("web-auth.json"));
        if auth_file.as_os_str().is_empty() {
            return Err("KATALOG_AUTH_FILE darf nicht leer sein".into());
        }
        validate_auth_path(&db_path, &auth_file)?;
        let bind: SocketAddr = lookup("KATALOG_BIND")
            .unwrap_or_else(|| "127.0.0.1:8080".into())
            .parse()
            .map_err(|_| "KATALOG_BIND ist keine gültige IP-Adresse mit Port".to_string())?;
        let write = lookup("KATALOG_WRITE").as_deref() == Some("1");
        if write
            && (lookup("KATALOG_ALLOW_NO_AUTH").as_deref() == Some("1")
                || lookup("KATALOG_ALLOWED_HOSTS").is_none_or(|s| s.trim().is_empty()))
        {
            return Err(
                "KATALOG_WRITE erfordert Anmeldung und eine explizite KATALOG_ALLOWED_HOSTS-Liste"
                    .into(),
            );
        }
        let allow_no_auth = lookup("KATALOG_ALLOW_NO_AUTH").as_deref() == Some("1");
        if allow_no_auth && !bind.ip().is_loopback() {
            return Err("KATALOG_ALLOW_NO_AUTH=1 ist nur mit Loopback-Bindung erlaubt".into());
        }
        let trust_proxy = lookup("KATALOG_TRUST_PROXY").as_deref() == Some("1");
        if allow_no_auth && trust_proxy {
            return Err(
                "KATALOG_ALLOW_NO_AUTH=1 darf nicht mit KATALOG_TRUST_PROXY=1 kombiniert werden"
                    .into(),
            );
        }
        let trusted_proxies = if trust_proxy {
            let list = lookup("KATALOG_TRUSTED_PROXIES").filter(|s| !s.trim().is_empty()).ok_or(
                "KATALOG_TRUST_PROXY=1 erfordert KATALOG_TRUSTED_PROXIES (IP/CIDR der Proxys, kommagetrennt); ohne diese Liste startet der Server nicht",
            )?;
            TrustedProxies::parse(&list)?
        } else {
            TrustedProxies::default()
        };
        let password = lookup("KATALOG_PASSWORD");
        let password_file = lookup("KATALOG_PASSWORD_FILE").map(PathBuf::from);
        if password.is_some() && password_file.is_some() {
            return Err(
                "KATALOG_PASSWORD und KATALOG_PASSWORD_FILE dürfen nicht gleichzeitig gesetzt sein"
                    .into(),
            );
        }
        let allowed_hosts = match lookup("KATALOG_ALLOWED_HOSTS") {
            Some(value) => {
                let hosts: Vec<String> = value.split(',').map(|v| v.trim().to_string()).collect();
                for host in &hosts {
                    parse_host(host).map_err(|_| {
                        "KATALOG_ALLOWED_HOSTS enthält einen ungültigen Host".to_string()
                    })?;
                }
                Some(hosts)
            }
            None if bind.ip().is_loopback() => Some(
                ["localhost", "127.0.0.1", "[::1]"]
                    .into_iter()
                    .map(|h| format!("{h}:{}", bind.port()))
                    .collect(),
            ),
            // Ohne Hostliste könnte eine fremde Webseite per DNS-Rebinding die
            // angemeldete Sitzung eines Browsers im Heimnetz missbrauchen.
            None => {
                return Err("Nicht-Loopback-Bindung erfordert KATALOG_ALLOWED_HOSTS (Schutz vor DNS-Rebinding), z. B. KATALOG_ALLOWED_HOSTS=katalog.example,192.168.1.50:8080".into());
            }
        };
        let max_mb = positive_limit(&lookup, "KATALOG_UPLOAD_MAX_MB", 1024, 4096)?;
        let max_files = positive_limit(&lookup, "KATALOG_UPLOAD_MAX_FILES", 2, 32)?;
        let archive_mb = positive_limit(&lookup, "KATALOG_ARCHIVE_MAX_MB", 1024, 4096)?;
        let mesh_mb = positive_limit(&lookup, "KATALOG_IMPORT_MAX_MESH_MB", 512, 512)?;
        let unpacked_mb = positive_limit(&lookup, "KATALOG_IMPORT_MAX_UNPACKED_MB", 1024, 1024)?;
        let vertices =
            positive_limit(&lookup, "KATALOG_IMPORT_MAX_VERTICES", 2_000_000, 2_000_000)?;
        let triangles = positive_limit(
            &lookup,
            "KATALOG_IMPORT_MAX_TRIANGLES",
            3_000_000,
            3_000_000,
        )?;
        let upload = lookup("KATALOG_UPLOAD_DIR").map(|dir| UploadConfig {
            dir: PathBuf::from(dir),
            max_bytes: max_mb * 1024 * 1024,
            max_files: max_files as usize,
            archive_max_bytes: archive_mb * 1024 * 1024,
            limits: katalog_core::import::files::WebImportLimits {
                max_file_bytes: max_mb * 1024 * 1024,
                max_mesh_file_bytes: mesh_mb * 1024 * 1024,
                max_unpacked_bytes: unpacked_mb * 1024 * 1024,
                max_vertices: vertices as usize,
                max_triangles: triangles as usize,
            },
        });
        let companion_mb = positive_limit(&lookup, "KATALOG_COMPANION_MAX_MB", 1024, 4096)?;
        let file_ops = lookup("KATALOG_FILE_OPS").as_deref() == Some("1");
        let file_ops_daily_limit = positive_limit(&lookup, "KATALOG_FILE_OPS_DAILY_LIMIT", 2000, u64::MAX)?;
        let file_ops_ack = lookup("KATALOG_FILE_OPS_ACK").map(|v| v.split(',').map(str::to_owned).collect()).unwrap_or_default();
        if file_ops && (!write || allow_no_auth || lookup("KATALOG_ALLOWED_HOSTS").is_none_or(|v| v.trim().is_empty())) {
            return Err("Dateiverwaltung erfordert Schreibmodus, Anmeldung und explizite Hostliste".into());
        }
        let slicer_links = match lookup("KATALOG_SLICER_LINKS").as_deref() {
            None | Some("0") => None,
            Some("1") => Some(ranged_limit(
                &lookup,
                "KATALOG_SLICER_LINK_TTL_SECONDS",
                crate::slicer::DEFAULT_TTL_SECONDS,
                crate::slicer::MIN_TTL_SECONDS,
                crate::slicer::MAX_TTL_SECONDS,
            )?),
            Some(_) => return Err("KATALOG_SLICER_LINKS muss 0 oder 1 sein".into()),
        };
        let source_roots = lookup("KATALOG_SOURCE_ROOTS")
            .filter(|v| !v.is_empty())
            .map(|value| std::env::split_paths(&value).collect())
            .unwrap_or_default();
        let printer_allow = PrinterAllow::parse(&lookup("KATALOG_PRINTER_ALLOW").unwrap_or_default())?;
        let mut config = Self {
            source_roots,
            printer_allow,
            slicer_links,
            file_ops, file_ops_daily_limit, companion_max_bytes: companion_mb * 1024 * 1024, file_ops_ack, file_roots,
            static_dir,
            db_path,
            roots,
            auth_file,
            bind,
            allowed_hosts,
            trust_proxy,
            trusted_proxies,
            allow_no_auth,
            write,
            upload,
            password,
            password_file,
        };
        validate_password_path(Some(&config.db_path), &config.auth_file)?;
        validate_source_config(&mut config)?;
        validate_upload_config(&mut config)?;
        validate_fileops_config(&config)?;
        Ok(config)
    }
}

/// Parsed configuration only; callers must additionally enforce HOME_NETWORK
/// on every resolved address and pin the checked address when connecting.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PrinterAllow {
    entries: Vec<PrinterAllowEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrinterAllowEntry {
    Host(String),
    Network(PrinterNetwork),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrinterNetwork {
    address: IpAddr,
    prefix: u8,
}

impl PrinterNetwork {
    pub fn address(&self) -> IpAddr {
        self.address
    }

    pub fn prefix(&self) -> u8 {
        self.prefix
    }

    fn contains(&self, ip: IpAddr) -> bool {
        match (self.address, ip) {
            (IpAddr::V4(net), IpAddr::V4(ip)) => {
                let mask = u32::MAX
                    .checked_shl(32 - u32::from(self.prefix))
                    .unwrap_or(0);
                u32::from(net) & mask == u32::from(ip) & mask
            }
            (IpAddr::V6(net), IpAddr::V6(ip)) => {
                let mask = u128::MAX
                    .checked_shl(128 - u32::from(self.prefix))
                    .unwrap_or(0);
                u128::from(net) & mask == u128::from(ip) & mask
            }
            _ => false,
        }
    }
}

impl PrinterAllow {
    pub fn entries(&self) -> &[PrinterAllowEntry] {
        &self.entries
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        let invalid = || {
            "KATALOG_PRINTER_ALLOW expects comma-separated hosts, IPs or CIDRs without ports"
                .to_owned()
        };
        let mut entries = Vec::new();
        if value.trim().is_empty() {
            return Ok(Self { entries });
        }
        for token in value.split(',').map(str::trim) {
            let entry = if let Some((address, prefix)) = token.split_once('/') {
                let address: IpAddr = address.parse().map_err(|_| invalid())?;
                let max = if address.is_ipv4() { 32 } else { 128 };
                let prefix = prefix
                    .parse::<u8>()
                    .ok()
                    .filter(|n| {
                        *n <= max
                            && !prefix.is_empty()
                            && prefix.bytes().all(|b| b.is_ascii_digit())
                    })
                    .ok_or_else(invalid)?;
                PrinterAllowEntry::Network(PrinterNetwork { address, prefix })
            } else if let Ok(address) = token.parse::<IpAddr>() {
                let prefix = if address.is_ipv4() { 32 } else { 128 };
                PrinterAllowEntry::Network(PrinterNetwork { address, prefix })
            } else {
                let (host, port) = parse_host(token).map_err(|_| invalid())?;
                if port.is_some()
                    || token.starts_with('[')
                    || host.bytes().all(|b| b.is_ascii_digit() || b == b'.')
                {
                    return Err(invalid());
                }
                PrinterAllowEntry::Host(host.trim_end_matches('.').to_owned())
            };
            entries.push(entry);
        }
        Ok(Self { entries })
    }

    /// This is only the allowlist half of the address policy. Metadata endpoints
    /// remain excluded even under a matching host, broad CIDR or mapped IPv4.
    pub fn matches(&self, host: &str, ip: IpAddr) -> bool {
        let normalized = match ip {
            IpAddr::V6(ip) => ip
                .to_ipv4_mapped()
                .map(IpAddr::V4)
                .unwrap_or(IpAddr::V6(ip)),
            ip => ip,
        };
        if normalized == IpAddr::from([169, 254, 169, 254])
            || normalized == IpAddr::V6(Ipv6Addr::new(0xfd00, 0xec2, 0, 0, 0, 0, 0, 0x254))
        {
            return false;
        }
        self.entries.iter().any(|entry| match entry {
            PrinterAllowEntry::Host(allowed) => {
                allowed.eq_ignore_ascii_case(host.trim_end_matches('.'))
            }
            PrinterAllowEntry::Network(network) => network.contains(ip),
        })
    }
}

fn validate_source_config(config: &mut Config) -> Result<(), String> {
    if config.source_roots.is_empty() {
        return Ok(());
    }
    if config.source_roots.iter().any(|p| p.as_os_str().is_empty()) {
        return Err("KATALOG_SOURCE_ROOTS contains an empty root".into());
    }
    let parent = |p: &Path| {
        p.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
            .to_path_buf()
    };
    let mut forbidden = vec![
        resolved(&parent(&config.auth_file))?,
        parent(&resolved(&config.auth_file)?),
        resolved(&parent(&config.db_path))?.join("trash"),
        parent(&resolved(&config.db_path)?).join("trash"),
    ];
    if let Some(upload) = &config.upload {
        forbidden.push(resolved(&upload.dir)?);
    }
    for root in &config.file_roots {
        forbidden.push(resolved(root)?.join(katalog_core::fsops::TRASH));
    }
    config.source_roots =
        katalog_core::fsops::validate_source_roots(&config.source_roots, &forbidden)
            .map_err(|e| format!("KATALOG_SOURCE_ROOTS: {e}"))?;
    Ok(())
}

/// Like `positive_limit` with a lower bound; the bounds are part of the message.
fn ranged_limit(
    lookup: &impl Fn(&str) -> Option<String>,
    key: &str,
    default: u64,
    min: u64,
    max: u64,
) -> Result<u64, String> {
    match lookup(key) {
        None => Ok(default),
        Some(value) => value
            .parse::<u64>()
            .ok()
            .filter(|n| value.bytes().all(|b| b.is_ascii_digit()) && (min..=max).contains(n))
            .ok_or_else(|| format!("{key} muss eine ganze Zahl zwischen {min} und {max} sein")),
    }
}

fn positive_limit(
    lookup: &impl Fn(&str) -> Option<String>,
    key: &str,
    default: u64,
    max: u64,
) -> Result<u64, String> {
    match lookup(key) {
        None => Ok(default),
        Some(value) => value
            .parse::<u64>()
            .ok()
            .filter(|n| {
                !value.is_empty()
                    && value.bytes().all(|b| b.is_ascii_digit())
                    && *n > 0
                    && *n <= max
            })
            .ok_or_else(|| format!("{key} muss eine ganze Zahl zwischen 1 und {max} sein")),
    }
}

/// Validate direct programmatic configuration as well as environment configuration.
/// Upload directories must already exist; no directories are created on startup.
pub fn validate_upload_config(config: &mut Config) -> Result<(), String> {
    let Some(upload) = config.upload.as_mut() else {
        return Ok(());
    };
    if !config.write
        || config.allow_no_auth
        || config
            .allowed_hosts
            .as_ref()
            .is_none_or(|hosts| hosts.is_empty())
    {
        return Err(
            "Uploads erfordern KATALOG_WRITE=1, Anmeldung und KATALOG_ALLOWED_HOSTS".into(),
        );
    }
    let journal=config.auth_file.with_file_name("web-uploads.json");
    if journal==config.auth_file || validate_auth_path(&config.db_path,&journal).is_err(){
        return Err("Upload-Journal kollidiert mit Auth-Datei oder Katalog".into());
    }
    if upload.max_bytes == 0
        || upload.max_bytes > 4096 * 1024 * 1024
        || upload.max_files == 0
        || upload.max_files > 32
        || upload.archive_max_bytes == 0
        || upload.archive_max_bytes > 4096 * 1024 * 1024
    {
        return Err("Ungültige Upload-Grenzen".into());
    }
    let limits = &upload.limits;
    if limits.max_file_bytes == 0
        || limits.max_file_bytes > upload.max_bytes
        || limits.max_mesh_file_bytes == 0
        || limits.max_mesh_file_bytes > 512 * 1024 * 1024
        || limits.max_unpacked_bytes == 0
        || limits.max_unpacked_bytes > 1024 * 1024 * 1024
        || limits.max_vertices == 0
        || limits.max_vertices > 2_000_000
        || limits.max_triangles == 0
        || limits.max_triangles > 3_000_000
    {
        return Err("Ungültige Web-Parsergrenzen".into());
    }
    let metadata = std::fs::symlink_metadata(&upload.dir)
        .map_err(|_| "KATALOG_UPLOAD_DIR ist nicht zugänglich".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("KATALOG_UPLOAD_DIR muss ein Verzeichnis ohne Symlink sein".into());
    }
    upload.dir = upload
        .dir
        .canonicalize()
        .map_err(|_| "KATALOG_UPLOAD_DIR ist nicht zugänglich".to_string())?;
    for root in &mut config.roots {
        *root = root
            .canonicalize()
            .map_err(|_| "KATALOG_ROOTS ist nicht zugänglich".to_string())?;
        if !root.is_dir() {
            return Err("KATALOG_ROOTS muss Verzeichnisse enthalten".into());
        }
    }
    if !config.roots.iter().any(|root| upload.dir.starts_with(root)) {
        return Err("KATALOG_UPLOAD_DIR muss unter KATALOG_ROOTS liegen".into());
    }
    let parent = |path: &std::path::Path| {
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf()
    };
    let db_location = config
        .db_path
        .canonicalize()
        .unwrap_or_else(|_| config.db_path.clone());
    let auth_location = config
        .auth_file
        .canonicalize()
        .unwrap_or_else(|_| config.auth_file.clone());
    let db_dir = parent(&db_location)
        .canonicalize()
        .map_err(|_| "DB-Verzeichnis ist nicht zugänglich".to_string())?;
    let auth_dir = parent(&auth_location)
        .canonicalize()
        .map_err(|_| "Auth-Verzeichnis ist nicht zugänglich".to_string())?;
    let configured_db_dir = parent(&config.db_path)
        .canonicalize()
        .map_err(|_| "DB-Verzeichnis ist nicht zugänglich".to_string())?;
    let configured_auth_dir = parent(&config.auth_file)
        .canonicalize()
        .map_err(|_| "Auth-Verzeichnis ist nicht zugänglich".to_string())?;
    let trash = configured_db_dir.join("trash");
    let mut excluded = vec![
        db_dir,
        auth_dir,
        configured_db_dir,
        configured_auth_dir,
        trash.canonicalize().unwrap_or(trash),
    ];
    if let Some(static_dir) = &config.static_dir {
        excluded.push(
            static_dir
                .canonicalize()
                .map_err(|_| "Static-Verzeichnis ist nicht zugänglich".to_string())?,
        );
    }
    if excluded
        .iter()
        .any(|path| upload.dir.starts_with(path) || path.starts_with(&upload.dir))
    {
        return Err("KATALOG_UPLOAD_DIR überschneidet sich mit DB, Auth, Trash oder Static".into());
    }
    // A real create/write/fsync/remove probe catches read-only mounts and ACLs.
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce).map_err(|_| "Upload-Schreibprüfung fehlgeschlagen".to_string())?;
    let probe = format!(".upload-probe-{:032x}.part",u128::from_ne_bytes(nonce));
    let directory=katalog_core::upload_fs::Directory::open(&upload.dir)
        .map_err(|_|"KATALOG_UPLOAD_DIR ist nicht schreibbar".to_string())?;
    let mut file=directory.create(&probe)
        .map_err(|_|"KATALOG_UPLOAD_DIR ist nicht schreibbar".to_string())?;
    use std::io::Write;
    let result=file.write_all(b"upload probe").and_then(|_|file.sync_all());
    drop(file);
    let removed=directory.unlink(&probe).and_then(|_|directory.sync());
    result
        .and(removed)
        .map_err(|_| "KATALOG_UPLOAD_DIR ist nicht schreibbar".to_string())
}

/// Names shared by all durable server journals.
pub const RESERVED_JOURNAL_NAMES: [&str; 3] =
    ["web-fsops.json", "web-uploads.json", "web-restore.json"];

pub fn validate_auth_journal_name(path: &Path) -> Result<(), String> {
    for path in [path.to_path_buf(), resolved(path)?] {
        if path
            .file_name()
            .is_some_and(|n| RESERVED_JOURNAL_NAMES.iter().any(|r| n == *r))
        {
            return Err("Auth-Datei kollidiert mit einem Serverjournal".into());
        }
        if path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with(".bak-"))
        {
            return Err("Auth-Datei kollidiert mit einer Rückfallsicherung".into());
        }
    }
    Ok(())
}

/// Called before bootstrap or standalone password writes.
pub fn validate_password_path(db: Option<&Path>, auth: &Path) -> Result<(), String> {
    validate_auth_journal_name(auth)?;
    if let Some(db) = db {
        validate_auth_path(db, auth)?;
    } else {
        for path in [auth.to_path_buf(), resolved(auth)?] {
            if path.file_name().is_some_and(|name| {
                [
                    "catalog.db",
                    "catalog.db-wal",
                    "catalog.db-shm",
                    "catalog.db-journal",
                    "catalog.db.lock",
                ]
                .iter()
                .any(|reserved| name == *reserved)
            }) {
                return Err("KATALOG_AUTH_FILE kollidiert mit einer Katalogdatei".into());
            }
        }
    }
    checked_journal_path(db, auth, RESERVED_JOURNAL_NAMES[2])?;
    Ok(())
}

/// Reserve the restore journal without creating or reading its contents.
pub fn restore_journal_path(db: &Path, auth: &Path) -> Result<PathBuf, String> {
    checked_journal_path(Some(db), auth, RESERVED_JOURNAL_NAMES[2])
}

fn checked_journal_path(db: Option<&Path>, auth: &Path, name: &str) -> Result<PathBuf, String> {
    let journal = auth.with_file_name(name);
    if let Some(db) = db {
        validate_auth_path(db, &journal)?;
    }
    if resolved(&journal)? == resolved(auth)? {
        return Err("Restore-Journal kollidiert mit Auth-Datei".into());
    }
    Ok(journal)
}

pub(crate) fn validate_fileops_config(config: &Config) -> Result<(), String> {
    validate_password_path(Some(&config.db_path), &config.auth_file)?;
    if config.file_ops && (!config.write || config.allow_no_auth || config.file_ops_daily_limit == 0 || config.allowed_hosts.as_ref().is_none_or(|h|h.is_empty() || h.iter().any(|v|parse_host(v).is_err()))) {
        return Err("Dateiverwaltung erfordert Schreibmodus, Anmeldung, Hostliste und positive Tagesgrenze".into());
    }
    if config.file_ops_ack.iter().any(|id|id.len()!=32 || !id.bytes().all(|b|b.is_ascii_hexdigit())) {
        return Err("KATALOG_FILE_OPS_ACK erwartet kommagetrennte Operations-IDs".into());
    }
    Ok(())
}

fn resolved(path: &std::path::Path) -> Result<PathBuf, String> {
    if let Ok(path) = path.canonicalize() {
        return Ok(path);
    }
    if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
        if let Ok(parent) = parent.canonicalize() {
            return Ok(parent.join(name));
        }
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|_| "Arbeitsverzeichnis nicht lesbar".to_string())?
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for part in absolute.components() {
        match part {
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }
    if let (Some(parent), Some(name)) = (normalized.parent(), normalized.file_name()) {
        if let Ok(parent) = parent.canonicalize() {
            return Ok(parent.join(name));
        }
    }
    Ok(normalized)
}
pub fn validate_auth_path(db: &Path, auth: &Path) -> Result<(), String> {
    let auth = resolved(auth)?;
    if let Ok(lock) = katalog_core::db::instance_lock::InstanceLock::path_for(db) {
        if auth == resolved(&lock)? {
            return Err("KATALOG_AUTH_FILE must not replace the catalog lock file".into());
        }
    }
    for db in [db.to_path_buf(), resolved(db)?] {
        for suffix in ["", "-wal", "-shm", "-journal", ".lock"] {
            let mut name = db.as_os_str().to_os_string();
            name.push(suffix);
            if auth == resolved(std::path::Path::new(&name))? {
                return Err(
                "KATALOG_AUTH_FILE darf weder der Katalog noch eine SQLite-Begleitdatei oder Katalog-Sperrdatei sein"
                    .into(),
            );
            }
        }
    }
    Ok(())
}

/// A Host authority, never a URL, userinfo, unbracketed IPv6 or wildcard.
pub fn parse_host(value: &str) -> Result<(String, Option<u16>), String> {
    let invalid = || "Ungültiger Host-Header".to_string();
    if value.is_empty() || !value.is_ascii() || value.bytes().any(|b| b.is_ascii_whitespace()) {
        return Err(invalid());
    }
    let (host, suffix) = if let Some(rest) = value.strip_prefix('[') {
        let end = rest.find(']').ok_or_else(invalid)?;
        let ip: Ipv6Addr = rest[..end].parse().map_err(|_| invalid())?;
        (format!("[{ip}]"), &rest[end + 1..])
    } else {
        let end = value.find(':').unwrap_or(value.len());
        let host = &value[..end];
        if host.is_empty() {
            return Err(invalid());
        }
        if host.parse::<IpAddr>().is_err() {
            let dns = host.strip_suffix('.').unwrap_or(host);
            if dns.len() > 253
                || dns.split('.').any(|label| {
                    label.is_empty()
                        || label.len() > 63
                        || label.starts_with('-')
                        || label.ends_with('-')
                        || !label
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                })
            {
                return Err(invalid());
            }
        }
        (host.to_ascii_lowercase(), &value[end..])
    };
    let port = if suffix.is_empty() {
        None
    } else {
        let digits = suffix.strip_prefix(':').ok_or_else(invalid)?;
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return Err(invalid());
        }
        Some(digits.parse::<u16>().map_err(|_| invalid())?)
    };
    Ok((host, port))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config(extra: &[(&str, &str)]) -> Result<Config, String> {
        Config::from_lookup(|key| {
            extra
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
                .or_else(|| match key {
                    "KATALOG_DB" => Some("/nonexistent/catalog.db".into()),
                    "KATALOG_ROOTS" => Some("/models".into()),
                    _ => None,
                })
        })
    }
    #[test]
    fn fileops_requires_explicit_write_and_auth_configuration() {
        assert!(config(&[("KATALOG_FILE_OPS", "1")]).is_err());
        assert!(config(&[("KATALOG_FILE_OPS_DAILY_LIMIT", "0")]).is_err());
    }

    #[test]
    fn the_companion_limit_defaults_to_1024_mib_and_is_validated() {
        assert_eq!(config(&[]).unwrap().companion_max_bytes, 1024 * 1024 * 1024);
        assert_eq!(config(&[("KATALOG_COMPANION_MAX_MB", "1")]).unwrap().companion_max_bytes, 1024 * 1024);
        assert_eq!(config(&[("KATALOG_COMPANION_MAX_MB", "4096")]).unwrap().companion_max_bytes, 4096 * 1024 * 1024);
        for bad in ["", "0", "-1", "+1", " 1", "1.5", "4097", "abc", "999999999999999999999999"] {
            assert!(config(&[("KATALOG_COMPANION_MAX_MB", bad)]).is_err(), "{bad}");
        }
    }

    #[test]
    fn upload_limits_are_validated_even_when_disabled() {
        assert!(config(&[]).unwrap().upload.is_none());
        for key in [
            "KATALOG_UPLOAD_MAX_MB",
            "KATALOG_UPLOAD_MAX_FILES",
            "KATALOG_ARCHIVE_MAX_MB",
            "KATALOG_IMPORT_MAX_MESH_MB",
            "KATALOG_IMPORT_MAX_UNPACKED_MB",
            "KATALOG_IMPORT_MAX_VERTICES",
            "KATALOG_IMPORT_MAX_TRIANGLES",
        ] {
            for bad in ["", "0", "-1", "+1", " 1", "1.5", "999999999999999999999999"] {
                assert!(config(&[(key, bad)]).is_err(), "{key}={bad}");
            }
        }
        assert!(config(&[("KATALOG_UPLOAD_MAX_MB", "4097")]).is_err());
        assert!(config(&[("KATALOG_UPLOAD_MAX_FILES", "33")]).is_err());
        assert!(config(&[("KATALOG_ARCHIVE_MAX_MB", "4097")]).is_err());
        assert!(config(&[("KATALOG_ARCHIVE_MAX_MB", "4096")]).is_ok());
        assert!(config(&[("KATALOG_ARCHIVE_MAX_MB", "1")]).is_ok());
        assert!(config(&[("KATALOG_IMPORT_MAX_MESH_MB", "513")]).is_err());
        assert!(config(&[("KATALOG_UPLOAD_MAX_MB", "4096")]).is_ok());
        assert!(config(&[("KATALOG_UPLOAD_DIR", "/models")]).is_err());
    }

    #[test]
    fn upload_paths_are_canonical_disjoint_and_writeable() {
        let mut nonce = [0u8; 16];
        getrandom::fill(&mut nonce).unwrap();
        let base = std::env::temp_dir().join(format!(
            "katalog-upload-config-{:032x}",
            u128::from_ne_bytes(nonce)
        ));
        for dir in ["models/upload", "db/trash", "auth", "static", "outside"] {
            std::fs::create_dir_all(base.join(dir)).unwrap();
        }
        let db = base.join("db/catalog.db").to_string_lossy().into_owned();
        let auth = base
            .join("auth/web-auth.json")
            .to_string_lossy()
            .into_owned();
        let models = base.join("models").to_string_lossy().into_owned();
        let static_dir = base.join("static").to_string_lossy().into_owned();
        let make = |upload: &std::path::Path, roots: &str| {
            config(&[
                ("KATALOG_DB", &db),
                ("KATALOG_AUTH_FILE", &auth),
                ("KATALOG_ROOTS", roots),
                ("KATALOG_STATIC_DIR", &static_dir),
                ("KATALOG_WRITE", "1"),
                ("KATALOG_ALLOWED_HOSTS", "localhost:8080"),
                ("KATALOG_UPLOAD_DIR", &upload.to_string_lossy()),
            ])
        };
        let valid = make(&base.join("models/upload"), &models).unwrap();
        let upload = valid.upload.unwrap();
        assert_eq!(
            upload.dir,
            base.join("models/upload").canonicalize().unwrap()
        );
        assert_eq!(upload.max_bytes, 1024 * 1024 * 1024);
        assert_eq!(upload.max_files, 2);
        assert_eq!(upload.archive_max_bytes, 1024 * 1024 * 1024);
        assert_eq!(std::fs::read_dir(&upload.dir).unwrap().count(), 0);
        assert!(make(&base.join("outside"), &models).is_err());
        let all = base.to_string_lossy();
        for forbidden in ["db", "db/trash", "auth", "static", ""] {
            assert!(make(&base.join(forbidden), &all).is_err(), "{forbidden}");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::{symlink, PermissionsExt};
            symlink(base.join("outside"), base.join("models/escape")).unwrap();
            assert!(make(&base.join("models/escape"), &models).is_err());
            std::fs::create_dir_all(base.join("outside/sub")).unwrap();
            assert!(make(&base.join("models/escape/sub"), &models).is_err());
            std::fs::set_permissions(&upload.dir, std::fs::Permissions::from_mode(0o555)).unwrap();
            // Root can bypass mode bits; assert denial when this user cannot.
            if std::fs::File::create(upload.dir.join("permission-check")).is_err() {
                assert!(make(&upload.dir, &models).is_err());
            }
            std::fs::set_permissions(&upload.dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn no_auth_and_trusted_proxy_are_incompatible() {
        assert!(config(&[("KATALOG_ALLOW_NO_AUTH", "1"), ("KATALOG_TRUST_PROXY", "1")]).is_err());
    }
    #[test]
    fn trust_proxy_requires_a_valid_trusted_proxy_list() {
        let base = [("KATALOG_TRUST_PROXY", "1")];
        let with = |list: &'static str| config(&[base[0], ("KATALOG_TRUSTED_PROXIES", list)]);
        // Missing, empty or broken lists stop the start; "everything" is not a proxy list.
        assert!(config(&base).err().unwrap().contains("KATALOG_TRUSTED_PROXIES"));
        for bad in ["", " ", ",", "10.0.0.1,", "nope", "10.0.0.0/33", "::1/129", "10.0.0.0/", "10.0.0.0/-1", "0.0.0.0/0", "::/0", "10.0.0.0/8/9", "10.0.0.1:80"] {
            assert!(with(bad).is_err(), "{bad:?} must be rejected");
        }
        let c = with("127.0.0.1, 10.0.0.0/8 ,fd00::/8,::1").unwrap();
        assert!(c.trust_proxy && !c.trusted_proxies.is_empty());
        // Without TRUST_PROXY nothing changes: no list needed, nothing trusted.
        let c = config(&[("KATALOG_TRUSTED_PROXIES", "10.0.0.0/8")]).unwrap();
        assert!(!c.trust_proxy && c.trusted_proxies.is_empty());
    }
    #[test]
    fn trusted_proxy_matching_covers_ipv4_ipv6_and_mapped_addresses() {
        let t = TrustedProxies::parse("10.0.0.0/8,192.0.2.7,2001:db8::/32,::ffff:198.51.100.0/120").unwrap();
        let yes = ["10.1.2.3", "192.0.2.7", "2001:db8:ffff::1", "::ffff:10.1.2.3", "198.51.100.9"];
        let no = ["11.0.0.1", "192.0.2.8", "2001:db9::1", "::1", "198.51.101.9"];
        for a in yes { assert!(t.contains(a.parse().unwrap()), "{a}"); }
        for a in no { assert!(!t.contains(a.parse().unwrap()), "{a}"); }
    }
    #[test]
    fn defaults_and_bootstrap_inputs() {
        let c = config(&[]).unwrap();
        assert_eq!(c.bind.to_string(), "127.0.0.1:8080");
        assert_eq!(c.auth_file, PathBuf::from("/nonexistent/web-auth.json"));
        assert_eq!(
            c.allowed_hosts.unwrap(),
            ["localhost:8080", "127.0.0.1:8080", "[::1]:8080"]
        );
        assert!(c.password.is_none() && c.password_file.is_none());
        assert!(!c.allow_no_auth && !c.trust_proxy);
    }
    #[test]
    fn unsafe_or_ambiguous_configuration_rejected() {
        assert!(config(&[
            ("KATALOG_BIND", "0.0.0.0:8080"),
            ("KATALOG_ALLOW_NO_AUTH", "1")
        ])
        .is_err());
        assert!(config(&[("KATALOG_PASSWORD", "a"), ("KATALOG_PASSWORD_FILE", "a")]).is_err());
        assert!(config(&[("KATALOG_ROOTS", "")]).is_err());
        assert!(config(&[("KATALOG_ROOTS", ":/models")]).is_err());
        assert!(config(&[("KATALOG_ALLOWED_HOSTS", "http://localhost")]).is_err());
        assert!(config(&[("KATALOG_BIND", "localhost:8080")]).is_err());
        assert!(Config::from_lookup(|_| None).is_err());
    }
    #[test]
    fn mode_description_names_the_active_mode() {
        let mut c = config(&[]).unwrap();
        assert_eq!(c.mode_description(), "nur lesend");
        c.write = true;
        assert_eq!(c.mode_description(), "Schreibmodus");
        c.file_ops = true;
        assert_eq!(c.mode_description(), "Schreibmodus mit Dateiverwaltung");
        c.upload = Some(UploadConfig {
            dir: PathBuf::from("/u"),
            max_bytes: 1,
            max_files: 1,
            archive_max_bytes: 1,
            limits: katalog_core::import::files::WebImportLimits {
                max_file_bytes: 1,
                max_mesh_file_bytes: 1,
                max_unpacked_bytes: 1,
                max_vertices: 1,
                max_triangles: 1,
            },
        });
        assert_eq!(c.mode_description(), "Schreibmodus mit Upload und Dateiverwaltung");
        c.file_ops = false;
        assert_eq!(c.mode_description(), "Schreibmodus mit Upload");
    }
    #[test]
    fn non_loopback_bind_without_allowed_hosts_is_rejected() {
        for bind in ["0.0.0.0:8080", "192.168.1.5:8080", "[::]:8080"] {
            let Err(err) = config(&[("KATALOG_BIND", bind)]) else { panic!("{bind}") };
            assert!(err.contains("KATALOG_ALLOWED_HOSTS"), "{bind}: {err}");
        }
        assert!(config(&[
            ("KATALOG_BIND", "0.0.0.0:8080"),
            ("KATALOG_ALLOWED_HOSTS", "localhost:8080")
        ])
        .is_ok());
    }
    #[test]
    fn proxy_and_external_binding_are_explicit() {
        let c = config(&[
            ("KATALOG_BIND", "0.0.0.0:9090"),
            ("KATALOG_TRUST_PROXY", "1"),
            ("KATALOG_TRUSTED_PROXIES", "172.18.0.0/16"),
            ("KATALOG_ALLOWED_HOSTS", "katalog.example"),
        ])
        .unwrap();
        assert!(c.allowed_hosts.is_some() && c.trust_proxy);
        assert!(
            config(&[
                ("KATALOG_BIND", "[::1]:8080"),
                ("KATALOG_ALLOW_NO_AUTH", "1")
            ])
            .unwrap()
            .allow_no_auth
        );
    }
    #[test]
    fn strict_hosts_support_ipv6_and_optional_ports() {
        assert_eq!(
            parse_host("EXAMPLE.com:443").unwrap(),
            ("example.com".into(), Some(443))
        );
        assert_eq!(parse_host("[::1]").unwrap(), ("[::1]".into(), None));
        for host in [
            "",
            "user@host",
            "host/path",
            "host:65536",
            "host:",
            "::1",
            "[invalid]",
            " host",
            "a..b",
            "-host",
            "host:+80",
            "host#fragment",
        ] {
            assert!(parse_host(host).is_err(), "{host}");
        }
    }
    #[test]
    fn trash_is_only_added_when_already_present() {
        let root = std::env::temp_dir().join(format!("katalog-config-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let db = root.join("catalog.db").to_string_lossy().into_owned();
        let make = || config(&[("KATALOG_DB", &db)]).unwrap();
        assert_eq!(make().roots, vec![PathBuf::from("/models")]);
        assert!(!root.join("trash").exists());
        std::fs::create_dir(root.join("trash")).unwrap();
        assert_eq!(
            make().roots,
            vec![PathBuf::from("/models"), root.join("trash")]
        );
        assert!(!root.join("catalog.db").exists());
        assert!(!root.join("web-auth.json").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn password_file_cannot_replace_catalog_or_sidecars() {
        for suffix in ["", "-wal", "-shm", "-journal", ".lock"] {
            let path = format!("/nonexistent/catalog.db{suffix}");
            assert!(config(&[("KATALOG_AUTH_FILE", &path)]).is_err());
        }
        assert!(validate_auth_path(
            std::path::Path::new("catalog.db"),
            std::path::Path::new("./catalog.db")
        )
        .is_err());
        assert!(validate_auth_path(
            std::path::Path::new("catalog.db"),
            std::path::Path::new("sub/../catalog.db")
        )
        .is_err());
    }
}

#[cfg(test)]
mod upload_journal_tests {
    use super::*;
    #[test]
    fn journal_cannot_replace_auth_or_catalog() {
        for key in ["KATALOG_AUTH_FILE", "KATALOG_DB"] {
            let result=Config::from_lookup(|name|match name {
                "KATALOG_DB"=>Some(if key==name{"/tmp/web-uploads.json"}else{"/tmp/catalog.db"}.into()),
                "KATALOG_AUTH_FILE"=>Some(if key==name{"/tmp/web-uploads.json"}else{"/tmp/web-auth.json"}.into()),
                "KATALOG_ROOTS"=>Some("/models".into()),
                "KATALOG_UPLOAD_DIR"=>Some("/models/uploads".into()),
                "KATALOG_WRITE"=>Some("1".into()),
                "KATALOG_ALLOWED_HOSTS"=>Some("localhost:8080".into()),
                _=>None,
            });
            assert!(matches!(result,Err(message) if message.contains("kollidiert")));
        }
    }
}
