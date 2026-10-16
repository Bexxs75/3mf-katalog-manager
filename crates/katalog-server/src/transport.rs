//! HTTP/1 admission and deadlines, shared by TCP and socket-free duplex tests.
use axum::{extract::ConnectInfo, Router};
use hyper::{server::conn::http1, service::service_fn};
use hyper_util::rt::{TokioIo, TokioTimer};
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    sync::{watch, Semaphore},
    task::JoinSet,
    time::Instant,
};
use tower::ServiceExt;

pub async fn serve(
    listener: tokio::net::TcpListener,
    app: Router,
    shutdown: impl Future<Output = ()>,
) -> std::io::Result<()> {
    let slots = Arc::new(Semaphore::new(256));
    let (stop, stopped) = watch::channel(false);
    let mut tasks = JoinSet::new();
    tokio::pin!(shutdown);
    let result = loop {
        tokio::select! {
            biased;
            _ = &mut shutdown => break Ok(()),
            Some(_) = tasks.join_next(), if !tasks.is_empty() => {},
            accepted = listener.accept() => match accepted {
                Ok((stream, address)) => {
                    admit(&mut tasks, &slots, stream, app.clone().layer(axum::Extension(ConnectInfo(address))), stopped.clone());
                }
                Err(e) => break Err(e),
            }
        }
    };
    let _ = stop.send(true);
    while tasks.join_next().await.is_some() {}
    result
}

fn admit<I>(
    tasks: &mut JoinSet<()>,
    slots: &Arc<Semaphore>,
    io: I,
    app: Router,
    stop: watch::Receiver<bool>,
) -> bool
where
    I: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let Ok(permit) = slots.clone().try_acquire_owned() else {
        return false;
    };
    tasks.spawn(async move {
        let _permit = permit;
        connection(io, app, stop).await;
    });
    true
}

// Response headers finish the service future before the body is sent. Keep
// streaming writes visible to the idle timeout, including slow large downloads.
// Require 64 KiB per idle window (~1 KiB/s) so trickle reads cannot
// retain a connection/download slot indefinitely.
const MIN_WRITE_PROGRESS: usize = 64 * 1024;

struct ActiveIo<I> {
    io: I,
    activity: Arc<Mutex<(bool, Instant, usize)>>,
}
impl<I: AsyncRead + Unpin> AsyncRead for ActiveIo<I> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.io).poll_read(cx, buf)
    }
}
impl<I: AsyncWrite + Unpin> AsyncWrite for ActiveIo<I> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let result = Pin::new(&mut self.io).poll_write(cx, buf);
        if let Poll::Ready(Ok(n)) = result {
            let mut activity = self.activity.lock().unwrap();
            activity.2 = activity.2.saturating_add(n);
            if activity.2 >= MIN_WRITE_PROGRESS {
                activity.1 = Instant::now();
                activity.2 = 0;
            }
        }
        result
    }
    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[std::io::IoSlice<'_>],
    ) -> Poll<std::io::Result<usize>> {
        let result = Pin::new(&mut self.io).poll_write_vectored(cx, bufs);
        if let Poll::Ready(Ok(n)) = result {
            let mut activity = self.activity.lock().unwrap();
            activity.2 = activity.2.saturating_add(n);
            if activity.2 >= MIN_WRITE_PROGRESS {
                activity.1 = Instant::now();
                activity.2 = 0;
            }
        }
        result
    }
    fn is_write_vectored(&self) -> bool {
        self.io.is_write_vectored()
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.io).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.io).poll_shutdown(cx)
    }
}

async fn connection<I>(io: I, app: Router, mut stop: watch::Receiver<bool>)
where
    I: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let activity = Arc::new(Mutex::new((false, Instant::now(), 0)));
    let state = activity.clone();
    let service = service_fn(move |request| {
        let app = app.clone();
        let state = state.clone();
        async move {
            state.lock().unwrap().0 = true;
            let response = app.oneshot(request).await;
            *state.lock().unwrap() = (false, Instant::now(), 0);
            response
        }
    });
    let mut builder = http1::Builder::new();
    builder
        .timer(TokioTimer::new())
        .keep_alive(true)
        .header_read_timeout(Duration::from_secs(10));
    let conn = builder.serve_connection(
        TokioIo::new(ActiveIo {
            io,
            activity: activity.clone(),
        }),
        service,
    );
    tokio::pin!(conn);
    let idle = async {
        loop {
            let (active, last, _) = *activity.lock().unwrap();
            let deadline = if active {
                Instant::now() + Duration::from_secs(60)
            } else {
                last + Duration::from_secs(60)
            };
            tokio::time::sleep_until(deadline).await;
            let (active, last, _) = *activity.lock().unwrap();
            if !active && last.elapsed() >= Duration::from_secs(60) {
                break;
            }
        }
    };
    tokio::select! {
        _ = &mut conn => {},
        _ = idle => {},
        _ = async { if !*stop.borrow() { let _ = stop.changed().await; } } => {
            conn.as_mut().graceful_shutdown();
            let _ = tokio::time::timeout(Duration::from_secs(60), &mut conn).await;
        }
    }
}

/// SIGINT and (on Unix) SIGTERM both stop admission and drain active requests.
pub async fn shutdown_signal() {
    #[cfg(unix)]
    {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut term) => {
                tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = term.recv() => {} }
            }
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn app() -> Router {
        Router::new().route("/", axum::routing::get(|| async { "ok" }))
    }

    #[tokio::test(start_paused = true)]
    async fn progressing_response_is_not_cut_off_after_sixty_seconds() {
        let app = Router::new().route(
            "/",
            axum::routing::get(|| async { vec![42u8; 1024 * 1024] }),
        );
        let (mut client, server) = tokio::io::duplex(64 * 1024);
        let (_stop, rx) = watch::channel(false);
        let task = tokio::spawn(connection(server, app, rx));
        client
            .write_all(b"GET / HTTP/1.1\r\nHost: local\r\n\r\n")
            .await
            .unwrap();
        let mut buf = [0; 64 * 1024];
        client.read_exact(&mut buf).await.unwrap();
        for _ in 0..10 {
            tokio::time::advance(Duration::from_secs(50)).await;
            client.read_exact(&mut buf).await.unwrap();
            tokio::task::yield_now().await;
            assert!(
                !task.is_finished(),
                "active streaming response was timed out"
            );
        }
        drop(client);
        task.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn trickling_response_is_closed_at_idle_deadline() {
        let app = Router::new().route(
            "/",
            axum::routing::get(|| async { vec![42u8; 1024 * 1024] }),
        );
        let (mut client, server) = tokio::io::duplex(64);
        let (_stop, rx) = watch::channel(false);
        let task = tokio::spawn(connection(server, app, rx));
        client
            .write_all(b"GET / HTTP/1.1\r\nHost: local\r\n\r\n")
            .await
            .unwrap();
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(55)).await;
        client.read_exact(&mut [0; 8]).await.unwrap();
        tokio::task::yield_now().await;
        assert!(!task.is_finished());
        tokio::time::advance(Duration::from_secs(5)).await;
        tokio::task::yield_now().await;
        assert!(
            task.is_finished(),
            "trickling writer renewed the idle window"
        );
        task.await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn slow_headers_are_disconnected_after_ten_seconds() {
        let (mut client, server) = tokio::io::duplex(4096);
        let (_stop, rx) = watch::channel(false);
        let task = tokio::spawn(connection(server, app(), rx));
        client
            .write_all(b"GET / HTTP/1.1\r\nHost: local")
            .await
            .unwrap();
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(9)).await;
        assert!(!task.is_finished());
        client.write_all(b"h").await.unwrap();
        tokio::time::advance(Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        assert!(task.is_finished());
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        assert!(!String::from_utf8_lossy(&response).contains("200 OK"));
    }

    #[tokio::test(start_paused = true)]
    async fn connection_257_is_closed_and_capacity_returns_on_disconnect() {
        let slots = Arc::new(Semaphore::new(256));
        let mut tasks = JoinSet::new();
        let (_stop, rx) = watch::channel(false);
        let mut clients = Vec::new();
        for _ in 0..256 {
            let (client, server) = tokio::io::duplex(4096);
            assert!(admit(&mut tasks, &slots, server, app(), rx.clone()));
            clients.push(client);
        }
        let (mut client, server) = tokio::io::duplex(4096);
        assert!(!admit(&mut tasks, &slots, server, app(), rx.clone()));
        assert_eq!(client.read(&mut [0; 1]).await.unwrap(), 0);
        drop(clients);
        while tasks.join_next().await.is_some() {}
        let (_client, server) = tokio::io::duplex(4096);
        assert!(admit(&mut tasks, &slots, server, app(), rx));
    }

    #[tokio::test(start_paused = true)]
    async fn keep_alive_idle_connection_closes_within_sixty_seconds() {
        let (mut client, server) = tokio::io::duplex(4096);
        let (_stop, rx) = watch::channel(false);
        let task = tokio::spawn(connection(server, app(), rx));
        client
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();
        let mut buffer = [0; 4096];
        let n = client.read(&mut buffer).await.unwrap();
        assert!(String::from_utf8_lossy(&buffer[..n]).contains("200 OK"));
        tokio::time::advance(Duration::from_secs(9)).await;
        tokio::task::yield_now().await;
        assert!(!task.is_finished());
        tokio::time::advance(Duration::from_secs(51)).await;
        tokio::task::yield_now().await;
        assert!(task.is_finished());
    }
}

#[cfg(test)]
mod shutdown_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test(start_paused = true)]
    async fn active_processing_survives_idle_deadline_and_shutdown_drains_response() {
        let (mut client, server) = tokio::io::duplex(4096);
        let (stop, rx) = watch::channel(false);
        let started = Arc::new(tokio::sync::Notify::new());
        let signal = started.clone();
        let app = Router::new().route(
            "/",
            axum::routing::get(move || {
                let signal = signal.clone();
                async move {
                    signal.notify_one();
                    tokio::time::sleep(Duration::from_secs(100)).await;
                    "completed"
                }
            }),
        );
        let task = tokio::spawn(connection(server, app, rx));
        client
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .unwrap();
        started.notified().await;
        tokio::time::advance(Duration::from_secs(70)).await;
        tokio::task::yield_now().await;
        assert!(!task.is_finished());
        stop.send(true).unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        task.await.unwrap();
        assert!(String::from_utf8_lossy(&response).ends_with("completed"));
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_closes_idle_connections() {
        let (mut client, server) = tokio::io::duplex(4096);
        let (stop, rx) = watch::channel(false);
        let task = tokio::spawn(connection(server, Router::new(), rx));
        tokio::task::yield_now().await;
        stop.send(true).unwrap();
        task.await.unwrap();
        assert_eq!(client.read(&mut [0; 1]).await.unwrap(), 0);
    }
}

#[cfg(test)]
mod output_tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[tokio::test(start_paused = true)]
    async fn stalled_response_writer_is_closed_at_idle_deadline() {
        let (mut client, server) = tokio::io::duplex(64);
        let (_stop, rx) = watch::channel(false);
        let app = Router::new().route("/", axum::routing::get(|| async { "x".repeat(1_000_000) }));
        let task = tokio::spawn(connection(server, app, rx));
        client
            .write_all(b"GET / HTTP/1.1\r\nHost: local\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(59)).await;
        tokio::task::yield_now().await;
        assert!(!task.is_finished());
        tokio::time::advance(Duration::from_secs(1)).await;
        tokio::time::timeout(Duration::from_millis(1), task)
            .await
            .unwrap()
            .unwrap();
    }
}
