use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;

use oauth2::basic::BasicClient;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge,
    RedirectUrl, Scope, TokenResponse, TokenUrl,
};
use tauri_plugin_opener::OpenerExt;

use crate::cloud::provider::{CloudError, CloudResult};

pub struct OAuthTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
}

/// Fuehrt den kompletten Loopback-Redirect-PKCE-Flow fuer Google aus: startet
/// einen kurzlebigen lokalen Server auf einem freien Port, oeffnet die
/// Consent-URL im System-Browser (ueber tauri-plugin-opener), wartet auf
/// genau einen Redirect, tauscht den Code gegen Tokens und beendet den
/// Server wieder. Dies ist Googles offiziell dokumentierter und einzig
/// unterstuetzter Weg fuer Desktop-Apps (RFC 8252) - eingebettete WebViews
/// blockiert Google aktiv, Custom-URI-Schemes akzeptiert der "Desktop-App"-
/// Client-Typ nicht.
pub async fn run_google_oauth_flow(
    app: &tauri::AppHandle,
    client_id: &str,
    client_secret: &str,
    scopes: &[&str],
) -> CloudResult<OAuthTokens> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| CloudError::Network(format!("Loopback-Server konnte nicht gestartet werden: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| CloudError::Network(e.to_string()))?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}");

    let client = BasicClient::new(ClientId::new(client_id.to_string()))
        .set_client_secret(ClientSecret::new(client_secret.to_string()))
        .set_auth_uri(
            AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".to_string())
                .map_err(|e| CloudError::Auth(e.to_string()))?,
        )
        .set_token_uri(
            TokenUrl::new("https://www.googleapis.com/oauth2/v3/token".to_string())
                .map_err(|e| CloudError::Auth(e.to_string()))?,
        )
        .set_redirect_uri(RedirectUrl::new(redirect_uri).map_err(|e| CloudError::Auth(e.to_string()))?);

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let mut auth_request = client
        .authorize_url(CsrfToken::new_random)
        .set_pkce_challenge(pkce_challenge)
        .add_extra_param("access_type", "offline")
        .add_extra_param("prompt", "consent");
    for scope in scopes {
        auth_request = auth_request.add_scope(Scope::new(scope.to_string()));
    }
    let (authorize_url, csrf_state) = auth_request.url();

    app.opener()
        .open_url(authorize_url.to_string(), None::<&str>)
        .map_err(|e| CloudError::Network(format!("Browser konnte nicht geoeffnet werden: {e}")))?;

    let redirect_result = tokio::time::timeout(
        std::time::Duration::from_secs(300),
        tokio::task::spawn_blocking(move || wait_for_redirect(listener)),
    )
    .await;

    let (code, returned_state) = match redirect_result {
        Ok(Ok(inner)) => inner?,
        Ok(Err(join_err)) => {
            return Err(CloudError::Network(format!(
                "Interner Fehler beim Warten auf die Google-Anmeldung: {join_err}"
            )))
        }
        Err(_elapsed) => {
            return Err(CloudError::Network(
                "Zeitüberschreitung beim Warten auf die Google-Anmeldung — bitte erneut versuchen."
                    .to_string(),
            ))
        }
    };
    if returned_state.secret() != csrf_state.secret() {
        return Err(CloudError::Auth("CSRF-Status stimmt nicht ueberein".to_string()));
    }

    let http_client = oauth2::reqwest::ClientBuilder::new()
        .redirect(oauth2::reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| CloudError::Network(e.to_string()))?;

    let token_result = client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(pkce_verifier)
        .request_async(&http_client)
        .await
        .map_err(|e| CloudError::Auth(e.to_string()))?;

    Ok(OAuthTokens {
        access_token: token_result.access_token().secret().clone(),
        refresh_token: token_result.refresh_token().map(|t| t.secret().clone()),
    })
}

/// Blockiert, bis der Loopback-Server genau eine Anfrage empfaengt, und
/// extrahiert `code`/`state` aus der Redirect-URL. Sendet eine einfache
/// HTML-Antwort zurueck, damit der Browser-Tab dem Nutzer signalisiert,
/// dass er das Fenster schliessen kann.
///
/// Nimmt den `TcpListener` bewusst per Value entgegen (statt per Referenz):
/// der Aufrufer verschiebt ihn in `tokio::task::spawn_blocking`, damit das
/// unbegrenzt blockierende `accept()` nicht einen Tokio-Worker-Thread
/// dauerhaft belegt, falls der Nutzer den Browser-Tab ohne Abschluss des
/// Logins schliesst. Ein `tokio::time::timeout` um den Aufruf herum sorgt
/// zusaetzlich dafuer, dass das Promise auf JS-Seite in jedem Fall settled.
fn wait_for_redirect(listener: TcpListener) -> CloudResult<(String, CsrfToken)> {
    let (mut stream, _) = listener
        .accept()
        .map_err(|e| CloudError::Network(format!("Redirect nicht empfangen: {e}")))?;

    let mut reader = BufReader::new(&stream);
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .map_err(|e| CloudError::Network(e.to_string()))?;

    let redirect_path = request_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| CloudError::Auth("ungueltige Redirect-Anfrage".to_string()))?;
    let full_url = format!("http://127.0.0.1{redirect_path}");
    let url = oauth2::url::Url::parse(&full_url).map_err(|e| CloudError::Auth(e.to_string()))?;

    let code = url
        .query_pairs()
        .find(|(key, _)| key == "code")
        .map(|(_, value)| value.into_owned())
        .ok_or_else(|| CloudError::Auth("kein 'code'-Parameter im Redirect".to_string()))?;
    let state = url
        .query_pairs()
        .find(|(key, _)| key == "state")
        .map(|(_, value)| CsrfToken::new(value.into_owned()))
        .ok_or_else(|| CloudError::Auth("kein 'state'-Parameter im Redirect".to_string()))?;

    let response_body =
        "<html><body>Verbindung hergestellt. Dieses Fenster kann geschlossen werden.</body></html>";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html; charset=utf-8\r\n\r\n{}",
        response_body.len(),
        response_body
    );
    stream.write_all(response.as_bytes()).ok();

    Ok((code, state))
}
