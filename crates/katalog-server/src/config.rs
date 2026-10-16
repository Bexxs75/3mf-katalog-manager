use std::{
    net::{IpAddr, Ipv6Addr, SocketAddr},
    path::PathBuf,
};

// Betrieb: Cookies (katalog_session) sind nicht nach Port getrennt. Andere
// HTTP-Dienste auf demselben Host erhalten sie; eigenen Host-/Domainnamen nutzen.
// Das Bootstrap-Zufallspasswort bleibt dauerhaft im Log (docker logs) und sollte
// nach dem ersten Anmelden geändert werden. set-password liest sichtbar im
// Terminal; ein laufender Server übernimmt die Datei erst nach einem Neustart.
// Ein vertrauenswürdiger Proxy muss X-Forwarded-For anhängen oder überschreiben.
#[derive(Clone)]
pub struct Config {
    pub db_path: PathBuf,
    pub static_dir: Option<PathBuf>,
    pub roots: Vec<PathBuf>,
    pub auth_file: PathBuf,
    pub bind: SocketAddr,
    pub allowed_hosts: Option<Vec<String>>,
    pub trust_proxy: bool,
    pub allow_no_auth: bool,
    pub password: Option<String>,
    pub password_file: Option<PathBuf>,
}

impl Config {
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
            None => {
                eprintln!("WARNUNG: Nicht-Loopback-Bindung ohne Hostprüfung: Gefahr durch DNS-Rebinding. KATALOG_ALLOWED_HOSTS setzen!");
                None
            }
        };
        Ok(Self {
            static_dir,
            db_path,
            roots,
            auth_file,
            bind,
            allowed_hosts,
            trust_proxy,
            allow_no_auth,
            password,
            password_file,
        })
    }
}

/// Reject the catalog and its SQLite sidecars before any password-file write.
pub fn validate_auth_path(db: &std::path::Path, auth: &std::path::Path) -> Result<(), String> {
    fn resolved(path: &std::path::Path) -> Result<PathBuf, String> {
        if let Ok(path) = path.canonicalize() {
            return Ok(path);
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
    let auth = resolved(auth)?;
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let mut name = db.as_os_str().to_os_string();
        name.push(suffix);
        if auth == resolved(std::path::Path::new(&name))? {
            return Err(
                "KATALOG_AUTH_FILE darf weder der Katalog noch eine SQLite-Begleitdatei sein"
                    .into(),
            );
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
    fn no_auth_and_trusted_proxy_are_incompatible() {
        assert!(config(&[("KATALOG_ALLOW_NO_AUTH", "1"), ("KATALOG_TRUST_PROXY", "1")]).is_err());
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
    fn proxy_and_external_binding_are_explicit() {
        let c = config(&[
            ("KATALOG_BIND", "0.0.0.0:9090"),
            ("KATALOG_TRUST_PROXY", "1"),
        ])
        .unwrap();
        assert!(c.allowed_hosts.is_none() && c.trust_proxy);
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
        for suffix in ["", "-wal", "-shm", "-journal"] {
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
