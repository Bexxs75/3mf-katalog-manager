use crate::cloud::provider::{CloudError, CloudResult};

const USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v3/userinfo";

pub async fn fetch_google_account_email(access_token: &str) -> CloudResult<String> {
    fetch_google_account_email_from(USERINFO_URL, access_token).await
}

async fn fetch_google_account_email_from(base_url: &str, access_token: &str) -> CloudResult<String> {
    let client = reqwest::Client::new();
    let response = client
        .get(base_url)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| CloudError::Network(e.to_string()))?;

    if !response.status().is_success() {
        return Err(CloudError::Auth(format!(
            "Google-Userinfo-Anfrage fehlgeschlagen: HTTP {}",
            response.status()
        )));
    }

    #[derive(serde::Deserialize)]
    struct UserInfo {
        email: String,
    }

    let info: UserInfo = response
        .json()
        .await
        .map_err(|e| CloudError::Auth(format!("Antwort konnte nicht gelesen werden: {e}")))?;
    Ok(info.email)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// Minimaler synchroner Mock-HTTP-Server, der genau eine Anfrage
    /// beantwortet - kein neues Test-Crate noetig, nur std::net.
    fn spawn_mock_userinfo_server(body: &'static str, status_line: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        let port = listener.local_addr().expect("local_addr").port();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                let response = format!(
                    "{status_line}\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        format!("http://127.0.0.1:{port}")
    }

    #[tokio::test]
    async fn parses_email_from_successful_response() {
        let url = spawn_mock_userinfo_server(r#"{"email": "user@example.com"}"#, "HTTP/1.1 200 OK");
        let email = fetch_google_account_email_from(&url, "fake-token").await.expect("fetch");
        assert_eq!(email, "user@example.com");
    }

    #[tokio::test]
    async fn returns_auth_error_for_non_success_status() {
        let url = spawn_mock_userinfo_server("", "HTTP/1.1 401 Unauthorized");
        let result = fetch_google_account_email_from(&url, "expired-token").await;
        assert!(matches!(result, Err(CloudError::Auth(_))));
    }
}
