use std::sync::Arc;

use axum::{
    extract::State,
    response::sse::{self, Sse},
};
use futures_util::{Stream, StreamExt, stream};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use utoipa::ToSchema;

use super::{AppState, openapi::Rejected};
use crate::indexer::{IndexMode, IndexReport};

/// One server-sent event. The `data` of each event is this object as JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// An index run started: from the watcher, on start, or through `POST /index`. Runs never overlap.
    IndexStarted {
        mode: IndexMode,
    },
    /// Chunk texts embedded so far out of those the run has to embed, sent after each batch.
    IndexProgress {
        embedded: u64,
        total: u64,
    },
    IndexFinished {
        report: IndexReport,
    },
    IndexFailed {
        message: String,
    },
    /// The service is shutting down. It is the last event on the stream.
    Stopping,
}

impl AppState {
    pub(super) fn publish(&self, event: Event) {
        // Fails only when nobody is subscribed, and events are not kept for later subscribers.
        let _ = self.events.send(event);
    }
}

/// Streams events until the service stops. Being subscribed doesn't keep the service alive.
#[utoipa::path(
    get,
    path = "/events",
    responses(
        (status = OK, description = "A stream of events", content_type = "text/event-stream", body = Event),
        Rejected,
    ),
)]
pub(super) async fn events(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<sse::Event, axum::Error>>> {
    let subscription = (state.events.subscribe(), state.shutdown.clone());
    let stream = stream::unfold(Some(subscription), |subscription| async move {
        next_event(subscription?).await
    });

    Sse::new(stream.map(|event| sse::Event::default().json_data(event)))
}

type Subscription = (broadcast::Receiver<Event>, CancellationToken);

async fn next_event(
    (mut receiver, shutdown): Subscription,
) -> Option<(Event, Option<Subscription>)> {
    tokio::select! {
        biased;
        () = shutdown.cancelled() => Some((Event::Stopping, None)),
        // A subscriber that falls behind gets its stream closed, and reconnects.
        received = receiver.recv() => received.ok().map(|event| (event, Some((receiver, shutdown)))),
    }
}
