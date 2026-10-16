use katalog_server::config::Config;
use std::{io::IsTerminal, path::PathBuf};

const HELP: &str = "katalog-server – Katalog nur lesend\n\nVerwendung: katalog-server [--help | --version | set-password]\n\nKATALOG_DB                Bestehende catalog.db (Pflicht)\nKATALOG_ROOTS             Erlaubte Dateiwurzeln (Pflicht; Unix :, Windows ;)\nKATALOG_AUTH_FILE         Passwortdatei (Vorgabe: neben catalog.db/web-auth.json)\nKATALOG_PASSWORD          Passwort nur zur Ersteinrichtung\nKATALOG_PASSWORD_FILE     Alternativ Passwortdatei nur zur Ersteinrichtung\nKATALOG_STATIC_DIR        Optionales Web-Build-Verzeichnis statt Smoke-Seite\nKATALOG_BIND              IP:Port (Vorgabe 127.0.0.1:8080)\nKATALOG_ALLOWED_HOSTS     Erlaubte Hosts, kommagetrennt, optional mit Port\n                         IPv6 in Klammern; Loopback-Vorgabe: localhost,\n                         127.0.0.1, [::1] jeweils mit Bind-Port.\n                         Bei anderer Bindung ohne Angabe keine Hostprüfung.\nKATALOG_ALLOW_NO_AUTH=1    Nur bei Loopback: Anmeldung und CSRF abschalten\nKATALOG_TRUST_PROXY=1      Forwarded-Header für Client-IP und HTTPS vertrauen\n\nWurzeln dürfen nicht von Fremden beschreibbar sein.\nBei KATALOG_TRUST_PROXY=1 darf der Server nur hinter einem abschirmenden Proxy stehen.\nDer Proxy muss X-Forwarded-For anhängen oder überschreiben.\nKATALOG_ALLOW_NO_AUTH=1 darf nicht mit KATALOG_TRUST_PROXY=1 kombiniert werden.\nCookies (katalog_session) sind nicht nach Port getrennt: Andere HTTP-Dienste\nauf demselben Host erhalten das Cookie. Eigenen Host-/Domainnamen verwenden.\nDas Zufallspasswort bleibt dauerhaft im Log (docker logs); nach dem ersten\nAnmelden ändern.\nDer Server bietet kein HTTPS; vorgesehen für Heimnetz/VPN.\nOhne Passwortdatei erfolgt eine Ersteinrichtung mit zufälligem Einmalpasswort.\nset-password liest ein neues Passwort von stdin (Terminal: zweimal, Eingabe sichtbar),\nverlangt 12 bis 128 Zeichen und speichert es atomar. Keine Passwortargumente.\nEin bereits laufender Server muss danach neu gestartet werden.\n";

#[tokio::main]
async fn main() {
    if let Err(message) = run().await {
        eprintln!("Fehler: {message}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [arg] if arg == "--help" => {
            println!("{HELP}");
            return Ok(());
        }
        [arg] if arg == "--version" => {
            println!("katalog-server {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        [arg] if arg == "set-password" => {
            let path = match std::env::var_os("KATALOG_AUTH_FILE") {
                Some(path) if !path.is_empty() => PathBuf::from(path),
                _ => {
                    let db = PathBuf::from(
                        std::env::var_os("KATALOG_DB")
                            .filter(|p| !p.is_empty())
                            .ok_or("KATALOG_AUTH_FILE oder KATALOG_DB muss gesetzt sein")?,
                    );
                    db.parent()
                        .filter(|p| !p.as_os_str().is_empty())
                        .unwrap_or_else(|| std::path::Path::new("."))
                        .join("web-auth.json")
                }
            };
            katalog_server::config::validate_auth_journal_name(&path)?;
            if let Some(db) = std::env::var_os("KATALOG_DB") {
                katalog_server::config::validate_auth_path(std::path::Path::new(&db), &path)?;
            }
            let stdin = std::io::stdin();
            let terminal = stdin.is_terminal();
            katalog_server::password::set_password(&path, &mut stdin.lock(), terminal).await?;
            eprintln!("Passwort gespeichert. Einen laufenden Server bitte neu starten.");
            return Ok(());
        }
        [] => {}
        _ => return Err("Unbekannte Argumente; siehe --help".into()),
    }
    let config = Config::from_env()?;
    let bind = config.bind;
    let app = katalog_server::initialize(config).await?;
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .map_err(|e| format!("Adresse {bind} kann nicht geöffnet werden: {e}"))?;
    let address = listener
        .local_addr()
        .map_err(|e| format!("Adresse nicht lesbar: {e}"))?;
    eprintln!("http://{address} – Katalog nur lesend");
    katalog_server::transport::serve(listener, app, katalog_server::transport::shutdown_signal())
        .await
        .map_err(|e| format!("HTTP-Server fehlgeschlagen: {e}"))
}
