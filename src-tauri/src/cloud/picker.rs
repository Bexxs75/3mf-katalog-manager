use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;

use tauri_plugin_opener::OpenerExt;

use crate::cloud::provider::{CloudError, CloudResult};

pub enum PickerMode {
    Files,
    Folder,
}

pub struct PickedItem {
    pub id: String,
    pub name: String,
    pub is_folder: bool,
}

pub enum PickerOutcome {
    Picked(Vec<PickedItem>),
    Cancelled,
}

/// Zeigt Googles offizielles Picker-Widget zur Datei-/Ordnerauswahl im
/// System-Browser an - bewusst NICHT eingebettet in Tauris WebView, aus
/// demselben Grund wie beim Login selbst (siehe oauth::run_google_oauth_flow):
/// Google behandelt erkannte Embedded-WebViews unzuverlaessig/ablehnend.
/// Serviert dafuer eine kleine lokale Seite auf einem freien Loopback-Port
/// (gleiches Muster wie der OAuth-Redirect-Server), die das Picker-JS laedt
/// und das Ergebnis per fetch() an denselben lokalen Server zurueckmeldet.
pub async fn run_picker_flow(
    app: &tauri::AppHandle,
    api_key: &str,
    access_token: &str,
    app_id: &str,
    mode: PickerMode,
) -> CloudResult<PickerOutcome> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| CloudError::Network(format!("Lokaler Server konnte nicht gestartet werden: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| CloudError::Network(e.to_string()))?
        .port();

    let page = render_picker_page(api_key, access_token, app_id, &mode);

    app.opener()
        .open_url(format!("http://127.0.0.1:{port}/"), None::<&str>)
        .map_err(|e| CloudError::Network(format!("Browser konnte nicht geoeffnet werden: {e}")))?;

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(300),
        tokio::task::spawn_blocking(move || wait_for_picker_result(listener, page)),
    )
    .await;

    match result {
        Ok(Ok(inner)) => inner,
        Ok(Err(join_err)) => Err(CloudError::Network(format!(
            "Interner Fehler beim Warten auf die Drive-Auswahl: {join_err}"
        ))),
        Err(_elapsed) => Err(CloudError::Network(
            "Zeitüberschreitung bei der Drive-Auswahl — bitte erneut versuchen.".to_string(),
        )),
    }
}

/// Baut die lokale HTML-Seite, die Googles Picker-JS laedt. `access_token`,
/// `api_key` und `app_id` werden ueber `serde_json::to_string` als
/// JS-String-Literale eingebettet, statt sie manuell in Anfuehrungszeichen
/// zu setzen - das escaped zuverlaessig, falls einer der Werte je ein
/// Sonderzeichen enthaelt.
fn render_picker_page(api_key: &str, access_token: &str, app_id: &str, mode: &PickerMode) -> String {
    let api_key_js = serde_json::to_string(api_key).unwrap_or_else(|_| "\"\"".to_string());
    let token_js = serde_json::to_string(access_token).unwrap_or_else(|_| "\"\"".to_string());
    let app_id_js = serde_json::to_string(app_id).unwrap_or_else(|_| "\"\"".to_string());
    let mode_js = match mode {
        PickerMode::Files => "\"files\"",
        PickerMode::Folder => "\"folder\"",
    };

    format!(
        r#"<!doctype html>
<html>
<head><meta charset="utf-8"><title>Google Drive</title></head>
<body style="font-family: sans-serif; padding: 2rem;">
<p id="status">Öffne Google Drive Auswahl …</p>
<script src="https://apis.google.com/js/api.js"></script>
<script>
  const ACCESS_TOKEN = {token_js};
  const API_KEY = {api_key_js};
  const APP_ID = {app_id_js};
  const MODE = {mode_js};

  function post(payload) {{
    fetch('/callback', {{
      method: 'POST',
      headers: {{ 'Content-Type': 'application/json' }},
      body: JSON.stringify(payload),
    }}).finally(() => {{
      document.getElementById('status').textContent = 'Fertig. Dieses Fenster kann geschlossen werden.';
    }});
  }}

  function onPickerApiLoad() {{
    const builder = new google.picker.PickerBuilder()
      .setOAuthToken(ACCESS_TOKEN)
      .setDeveloperKey(API_KEY)
      .setCallback(pickerCallback);

    // Ohne setAppId() registriert Google fuer den drive.file-Scope keine
    // Zugriffsfreigabe auf hier ausgewaehlte, nicht von dieser App selbst
    // erstellte Dateien - jeder spaetere files.get/download dafuer schlaegt
    // sonst mit HTTP 404 fehl. APP_ID ist die Google-Cloud-Projektnummer,
    // nicht die OAuth-Client-ID (siehe CloudConfig::google_cloud_project_number).
    if (APP_ID) {{
      builder.setAppId(APP_ID);
    }}

    if (MODE === 'folder') {{
      const view = new google.picker.DocsView(google.picker.ViewId.FOLDERS)
        .setSelectFolderEnabled(true)
        .setIncludeFolders(true);
      builder.addView(view);
    }} else {{
      const view = new google.picker.DocsView(google.picker.ViewId.DOCS)
        .setIncludeFolders(true);
      builder.addView(view).enableFeature(google.picker.Feature.MULTISELECT_ENABLED);
    }}

    builder.build().setVisible(true);
  }}

  function pickerCallback(data) {{
    if (data.action === google.picker.Action.PICKED) {{
      const docs = data.docs.map((d) => ({{ id: d.id, name: d.name, mimeType: d.mimeType }}));
      post({{ action: 'picked', docs }});
    }} else if (data.action === google.picker.Action.CANCEL) {{
      post({{ action: 'cancelled' }});
    }}
  }}

  gapi.load('picker', onPickerApiLoad);
</script>
</body>
</html>"#
    )
}

#[derive(serde::Deserialize)]
struct CallbackDoc {
    id: String,
    name: String,
    #[serde(rename = "mimeType")]
    mime_type: String,
}

#[derive(serde::Deserialize)]
#[serde(tag = "action", rename_all = "lowercase")]
enum CallbackPayload {
    Picked { docs: Vec<CallbackDoc> },
    Cancelled {},
}

/// Bedient auf demselben Loopback-Port zwei Anfragen nacheinander: zuerst
/// `GET /` (der Browser laedt die Picker-Seite), danach `POST /callback`
/// (die Seite meldet per fetch() das Auswahlergebnis zurueck). Alles
/// andere (z.B. `/favicon.ico`) wird mit 404 beantwortet, ohne die
/// Warteschleife zu verlassen.
fn wait_for_picker_result(listener: TcpListener, page: String) -> CloudResult<PickerOutcome> {
    loop {
        let (mut stream, _) = listener
            .accept()
            .map_err(|e| CloudError::Network(format!("Verbindung fehlgeschlagen: {e}")))?;

        let mut reader = BufReader::new(&stream);
        let mut request_line = String::new();
        reader
            .read_line(&mut request_line)
            .map_err(|e| CloudError::Network(e.to_string()))?;
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or("").to_string();
        let path = parts.next().unwrap_or("").to_string();

        let mut content_length: usize = 0;
        loop {
            let mut line = String::new();
            let bytes_read = reader
                .read_line(&mut line)
                .map_err(|e| CloudError::Network(e.to_string()))?;
            if bytes_read == 0 || line == "\r\n" {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                content_length = value.trim().parse().unwrap_or(0);
            }
        }

        if method == "GET" && path == "/" {
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html; charset=utf-8\r\n\r\n{}",
                page.len(),
                page
            );
            stream.write_all(response.as_bytes()).ok();
            continue;
        }

        if method == "POST" && path == "/callback" {
            let mut body = vec![0u8; content_length];
            reader
                .read_exact(&mut body)
                .map_err(|e| CloudError::Network(e.to_string()))?;

            let response_body =
                "<html><body>Fertig. Dieses Fenster kann geschlossen werden.</body></html>";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html; charset=utf-8\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            stream.write_all(response.as_bytes()).ok();

            let payload: CallbackPayload = serde_json::from_slice(&body)
                .map_err(|e| CloudError::Auth(format!("Ungueltige Picker-Antwort: {e}")))?;

            return Ok(match payload {
                CallbackPayload::Cancelled {} => PickerOutcome::Cancelled,
                CallbackPayload::Picked { docs } => PickerOutcome::Picked(
                    docs.into_iter()
                        .map(|d| PickedItem {
                            is_folder: d.mime_type == "application/vnd.google-apps.folder",
                            id: d.id,
                            name: d.name,
                        })
                        .collect(),
                ),
            });
        }

        stream
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
            .ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpStream;

    fn send_request(port: u16, request: &str) {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        stream.write_all(request.as_bytes()).expect("write");
        let mut response = String::new();
        stream.read_to_string(&mut response).ok();
    }

    #[test]
    fn wait_for_picker_result_parses_picked_payload_from_callback_post() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let handle =
            std::thread::spawn(move || wait_for_picker_result(listener, "<html></html>".to_string()));

        // Der Browser laedt zuerst die Seite - muss beantwortet werden, ohne
        // dass der Server die Warteschleife verlaesst.
        send_request(port, "GET / HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n");

        let body = r#"{"action":"picked","docs":[{"id":"file-1","name":"cube.3mf","mimeType":"application/octet-stream"},{"id":"folder-1","name":"Vasen","mimeType":"application/vnd.google-apps.folder"}]}"#;
        let request = format!(
            "POST /callback HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        send_request(port, &request);

        let outcome = handle.join().expect("thread").expect("outcome");
        match outcome {
            PickerOutcome::Picked(items) => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0].id, "file-1");
                assert!(!items[0].is_folder);
                assert_eq!(items[1].id, "folder-1");
                assert!(items[1].is_folder);
            }
            PickerOutcome::Cancelled => panic!("expected Picked"),
        }
    }

    #[test]
    fn wait_for_picker_result_returns_cancelled_for_cancel_action() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let handle =
            std::thread::spawn(move || wait_for_picker_result(listener, "<html></html>".to_string()));

        let body = r#"{"action":"cancelled"}"#;
        let request = format!(
            "POST /callback HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        send_request(port, &request);

        let outcome = handle.join().expect("thread").expect("outcome");
        assert!(matches!(outcome, PickerOutcome::Cancelled));
    }

    #[test]
    fn render_picker_page_embeds_token_and_key_as_escaped_js_string_literals() {
        let page = render_picker_page(
            r#"key-"with-quote"#,
            "token-value",
            "123456789",
            &PickerMode::Folder,
        );
        assert!(page.contains(r#"key-\"with-quote"#));
        assert!(page.contains("token-value"));
        assert!(page.contains(r#"const MODE = "folder";"#));
    }

    #[test]
    fn render_picker_page_embeds_app_id_and_calls_set_app_id() {
        let page = render_picker_page("api-key", "token-value", "123456789", &PickerMode::Files);
        assert!(page.contains(r#"const APP_ID = "123456789";"#));
        assert!(page.contains("builder.setAppId(APP_ID);"));
    }

    #[test]
    fn render_picker_page_skips_set_app_id_call_when_app_id_is_empty() {
        // Bestehende cloud.config.json-Dateien ohne das neue Feld duerfen den
        // Picker nicht kaputt machen - siehe CloudConfig::google_cloud_project_number.
        let page = render_picker_page("api-key", "token-value", "", &PickerMode::Files);
        assert!(page.contains(r#"const APP_ID = "";"#));
        assert!(page.contains("if (APP_ID) {"));
    }
}
