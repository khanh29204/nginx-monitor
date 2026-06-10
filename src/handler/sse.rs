use crate::{
    handler::auth::{verify_token, TokenQuery},
    AppState,
};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
};
use futures_util::stream::Stream;
use std::{convert::Infallible, time::Duration};
use tokio_stream::wrappers::BroadcastStream;
use tracing::info;

pub async fn sse_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<TokenQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    // Auth
    verify_token(
        &headers,
        params.token.as_deref(),
        &state.config.domain_auth,
        &state.http,
    )
    .await?;

    // Interval: client requests via ?interval=5 (seconds), min 1s, max 60s
    let interval_secs = params.interval.unwrap_or(5).clamp(1, 60);
    info!("SSE client connected, interval={}s", interval_secs);

    let rx = state.metrics_tx.subscribe();
    let stream = make_stream(rx, interval_secs);

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    ))
}

fn make_stream(
    rx: tokio::sync::broadcast::Receiver<crate::metrics::MetricsSnapshot>,
    interval_secs: u64,
) -> impl Stream<Item = Result<Event, Infallible>> {
    struct ThrottledStream {
        inner: BroadcastStream<crate::metrics::MetricsSnapshot>,
        interval: tokio::time::Interval,
        buffered: Option<crate::metrics::MetricsSnapshot>,
    }

    impl Stream for ThrottledStream {
        type Item = Result<Event, Infallible>;

        fn poll_next(
            mut self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Self::Item>> {
            use std::pin::Pin;
            use std::task::Poll;

            // Drain broadcast, keep only latest
            loop {
                match Pin::new(&mut self.inner).poll_next(cx) {
                    Poll::Ready(Some(Ok(snapshot))) => {
                        self.buffered = Some(snapshot);
                    }
                    Poll::Ready(Some(Err(_))) => {} // lagged, skip
                    Poll::Ready(None) => return Poll::Ready(None),
                    Poll::Pending => break,
                }
            }

            // Emit on interval tick if we have buffered data
            match self.interval.poll_tick(cx) {
                Poll::Ready(_) => {
                    if let Some(snapshot) = self.buffered.take() {
                        match serde_json::to_string(&snapshot) {
                            Ok(json) => Poll::Ready(Some(Ok(Event::default().data(json)))),
                            Err(_) => Poll::Pending,
                        }
                    } else {
                        Poll::Pending
                    }
                }
                Poll::Pending => Poll::Pending,
            }
        }
    }

    ThrottledStream {
        inner: BroadcastStream::new(rx),
        interval: tokio::time::interval(tokio::time::Duration::from_secs(interval_secs)),
        buffered: None,
    }
}