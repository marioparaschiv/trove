use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
};

use axum::{Json, extract::State, extract::rejection::JsonRejection};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use utoipa::ToSchema;

use super::{
    AppState,
    activity::ActivityGuard,
    error::{ApiError, ErrorBody},
    events::Event,
    openapi::Rejected,
};
use crate::{
    embedder::Profile,
    error::full_message,
    indexer::{self, IndexMode, IndexReport, IndexerError},
};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct IndexRequest {
    #[serde(default)]
    pub mode: IndexMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct IndexingStatus {
    /// An index run is running or waiting for its turn.
    pub busy: bool,
    /// Chunk texts embedded since the service started.
    pub embedded: u64,
}

/// Index runs of the service, which take turns.
#[derive(Default)]
pub(super) struct Indexing {
    /// Held by index runs, imports and source changes, which would break a run that read the sources before them.
    pub(super) turn: tokio::sync::Mutex<()>,
    jobs: AtomicUsize,
    embedded: AtomicU64,
}

impl Indexing {
    pub(super) fn status(&self) -> IndexingStatus {
        IndexingStatus {
            busy: self.jobs.load(Ordering::SeqCst) > 0,
            embedded: self.embedded.load(Ordering::SeqCst),
        }
    }
}

#[derive(Debug, Error)]
pub(super) enum JobError {
    #[error("indexing panicked")]
    Panic(#[source] tokio::task::JoinError),
    #[error(transparent)]
    Indexer(#[from] IndexerError),
}

/// A queued index run. Until it has run, the service stays alive and reports itself busy.
pub(super) struct Job {
    state: Arc<AppState>,
    _activity: ActivityGuard,
}

impl Job {
    pub(super) fn queue(state: &Arc<AppState>) -> Self {
        state.indexing.jobs.fetch_add(1, Ordering::SeqCst);

        Self {
            state: Arc::clone(state),
            _activity: state.activity.begin(),
        }
    }

    /// Runs once no other run is running, reporting on the events stream, and failures also on stderr.
    pub(super) async fn run(
        self,
        mode: IndexMode,
        profile: Profile,
    ) -> Result<IndexReport, JobError> {
        let state = Arc::clone(&self.state);
        let _turn = state.indexing.turn.lock().await;
        state.publish(Event::IndexStarted { mode, profile });

        let blocking_state = Arc::clone(&state);
        let result = tokio::task::spawn_blocking(move || {
            let state = blocking_state;
            indexer::index_sources(
                &state.index,
                &state.embedder,
                &state.config,
                mode,
                profile,
                &mut |progress| {
                    state.publish(Event::IndexProgress {
                        embedded: progress.embedded,
                        total: progress.total,
                    });
                },
            )
        })
        .await
        .map_err(JobError::Panic)
        .and_then(|result| Ok(result?));

        match &result {
            Ok(report) => {
                state
                    .indexing
                    .embedded
                    .fetch_add(report.embedded, Ordering::SeqCst);
                state.publish(Event::IndexFinished {
                    report: report.clone(),
                });
            }
            Err(error) => {
                let message = full_message(error);
                eprintln!("indexing failed: {message}");
                state.publish(Event::IndexFailed { message });
            }
        }

        result
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        self.state.indexing.jobs.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Imports the vector files at `paths` once no index run is running, reporting on the events stream, and failures
/// also on stderr.
pub(super) async fn import_vector_files(state: &Arc<AppState>, paths: Vec<PathBuf>) {
    let _activity = state.activity.begin();
    let _turn = state.indexing.turn.lock().await;

    let blocking_state = Arc::clone(state);
    let result = tokio::task::spawn_blocking(move || {
        let state = blocking_state;
        indexer::import_vector_files(&state.index, &state.embedder, &state.config, &paths)
    })
    .await
    .map_err(JobError::Panic)
    .and_then(|result| Ok(result?));

    match result {
        Ok(report) if report.files == 0 && report.problems.is_empty() => {}
        Ok(report) => state.publish(Event::VectorsImported { report }),
        Err(error) => {
            let message = full_message(&error);
            eprintln!("importing vector files failed: {message}");
            state.publish(Event::IndexFailed { message });
        }
    }
}

/// Indexes every source now and returns when done. The default mode chunks only changed notes; every mode embeds only
/// chunk texts without a vector, and `reembed` drops all vectors first.
#[utoipa::path(
    post,
    path = "/index",
    request_body = IndexRequest,
    responses(
        (status = OK, description = "What the run did", body = IndexReport),
        (status = BAD_REQUEST, description = "The request is invalid", body = ErrorBody),
        (status = INTERNAL_SERVER_ERROR, description = "Indexing failed", body = ErrorBody),
        Rejected,
    ),
)]
pub(super) async fn index(
    State(state): State<Arc<AppState>>,
    request: Result<Json<IndexRequest>, JsonRejection>,
) -> Result<Json<IndexReport>, ApiError> {
    let Json(IndexRequest { mode }) = request?;
    // On its own task, so a client that goes away doesn't cut the run short.
    let report = tokio::spawn(Job::queue(&state).run(mode, Profile::Foreground))
        .await
        .map_err(|error| ApiError::internal(&error))?
        .map_err(|error| ApiError::internal(&error))?;

    Ok(Json(report))
}
