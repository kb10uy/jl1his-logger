use std::{io, pin::pin, time::Duration};

use axum::{Router, body::Body, extract::Request};
use hyper::{body::Incoming, service::service_fn};
use hyper_util::{
    rt::{TokioExecutor, TokioIo, TokioTimer},
    server::{conn::auto::Builder, graceful::GracefulShutdown},
};
use tokio::net::TcpListener;
use tower::ServiceExt;

const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(30);
const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(30);
const KEEP_ALIVE_TIMEOUT: Duration = Duration::from_secs(10);
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);

pub async fn serve(listener: TcpListener, router: Router, shutdown: impl Future<Output = ()>) -> io::Result<()> {
    let mut builder = Builder::new(TokioExecutor::new());
    builder
        .http1()
        .timer(TokioTimer::new())
        .header_read_timeout(HEADER_READ_TIMEOUT);
    builder
        .http2()
        .timer(TokioTimer::new())
        .keep_alive_interval(KEEP_ALIVE_INTERVAL)
        .keep_alive_timeout(KEEP_ALIVE_TIMEOUT);
    let graceful = GracefulShutdown::new();
    let mut shutdown = pin!(shutdown);

    loop {
        let (stream, remote) = tokio::select! {
            biased;
            _ = &mut shutdown => break,
            accepted = listener.accept() => match accepted {
                Ok(accepted) => accepted,
                Err(e) if is_connection_error(&e) => continue,
                Err(e) => {
                    tracing::warn!("accepting a connection failed: {e}");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            },
        };
        let router = router.clone();
        let service = service_fn(move |request: Request<Incoming>| {
            let router = router.clone();
            async move { router.oneshot(request.map(Body::new)).await }
        });
        let connection = builder.serve_connection(TokioIo::new(stream), service).into_owned();
        let watched = graceful.watch(connection);
        tokio::spawn(async move {
            if let Err(e) = watched.await {
                tracing::debug!("connection from {remote} ended: {e}");
            }
        });
    }

    drop(listener);
    tokio::select! {
        _ = graceful.shutdown() => {}
        _ = tokio::time::sleep(SHUTDOWN_GRACE) => {
            tracing::warn!("closing with connections still open after {SHUTDOWN_GRACE:?}");
        }
    }
    Ok(())
}

fn is_connection_error(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::ConnectionRefused | io::ErrorKind::ConnectionAborted | io::ErrorKind::ConnectionReset
    )
}
