//! `trove serve`: the HTTP API on localhost that clients call.

mod activity;
mod categories;
mod error;
mod events;
mod guard;
mod indexing;
mod model;
mod openapi;
mod related;
mod runtime;
mod search;
mod sources;
mod status;
mod suggestions;
mod watcher;

use std::{
    io,
    net::{Ipv4Addr, SocketAddr},
    sync::{Arc, Mutex},
    time::Duration,
};

use thiserror::Error;
use tokio::{net::TcpListener, sync::broadcast};
use tokio_util::sync::CancellationToken;
use utoipa::OpenApi;
use utoipa_axum::{router::OpenApiRouter, routes};

pub use error::ErrorBody;
pub use events::Event;
pub use indexing::{IndexRequest, IndexingStatus};
pub use model::ModelStatus;
pub use openapi::openapi;
pub use runtime::{RuntimeError, RuntimeFile};
pub use sources::SourcePath;
pub use status::Status;

use crate::{
    config::Config,
    data_dir::DataDir,
    embedder::Embedder,
    index::{Index, IndexError},
    indexer::IndexMode,
};
use activity::Activity;
use indexing::{Indexing, Job};
use runtime::ServiceLock;
use watcher::{Changes, Watcher};

const EVENT_BUFFER: usize = 256;

#[derive(Debug, Error)]
pub enum ServeError {
    #[error("`trove serve` is already running for this data folder")]
    AlreadyRunning,
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
    #[error(transparent)]
    Index(#[from] IndexError),
    #[error("failed to watch the sources")]
    Watch(#[source] notify::Error),
    #[error("failed to generate the access token")]
    Token(#[source] getrandom::Error),
    #[error("failed to listen on localhost")]
    Bind(#[source] io::Error),
    #[error("failed to listen for Ctrl-C")]
    Signal(#[source] io::Error),
    #[error("the HTTP server failed")]
    Server(#[source] io::Error),
    #[error("the HTTP server panicked")]
    ServerPanic(#[source] tokio::task::JoinError),
}

pub(crate) struct AppState {
    runtime: RuntimeFile,
    allowed_hosts: [String; 2],
    config: Config,
    index: Arc<Mutex<Index>>,
    embedder: Arc<Embedder>,
    events: broadcast::Sender<Event>,
    activity: Activity,
    indexing: Indexing,
    watcher: Watcher,
    shutdown: CancellationToken,
}

fn router() -> OpenApiRouter<Arc<AppState>> {
    OpenApiRouter::with_openapi(openapi::ApiDoc::openapi())
        .routes(routes!(status::status))
        .routes(routes!(events::events))
        .routes(routes!(search::search))
        .routes(routes!(related::related))
        .routes(routes!(model::prewarm))
        .routes(routes!(model::unload))
        .routes(routes!(sources::list, sources::add, sources::remove))
        .routes(routes!(indexing::index))
        .routes(routes!(categories::categories))
        .routes(routes!(categories::pin))
        .routes(routes!(suggestions::suggestions))
        .routes(routes!(suggestions::decide))
        .routes(routes!(suggestions::orphans))
}

/// A started service: it holds the lock, listens, and has written the runtime file.
pub struct Service {
    data_dir: DataDir,
    idle_timeout: Duration,
    listener: TcpListener,
    state: Arc<AppState>,
    changes: Changes,
    /// The scan that catches changes made while no service ran.
    startup_index: Job,
    _lock: ServiceLock,
}

impl Service {
    pub async fn start(data_dir: &DataDir, config: &Config) -> Result<Self, ServeError> {
        let lock = ServiceLock::try_acquire(data_dir)?.ok_or(ServeError::AlreadyRunning)?;
        let index = Index::open(data_dir)?;
        let (watcher, changes) = Watcher::new().map_err(ServeError::Watch)?;
        for source in index.sources()? {
            watcher.watch(&source.path);
        }
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
            .await
            .map_err(ServeError::Bind)?;
        let port = listener.local_addr().map_err(ServeError::Bind)?.port();

        let runtime = RuntimeFile {
            port,
            token: token()?,
            pid: std::process::id(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
        };
        runtime.write(data_dir)?;

        let state = Arc::new(AppState {
            allowed_hosts: [format!("127.0.0.1:{port}"), format!("localhost:{port}")],
            runtime,
            index: Arc::new(Mutex::new(index)),
            embedder: Arc::new(Embedder::from_config(data_dir, config)),
            config: config.clone(),
            events: broadcast::channel(EVENT_BUFFER).0,
            activity: Activity::new(),
            indexing: Indexing::default(),
            watcher,
            shutdown: CancellationToken::new(),
        });
        // Queued before clients can connect, so `/status` shows it busy from the start.
        let startup_index = Job::queue(&state);

        Ok(Self {
            data_dir: data_dir.clone(),
            idle_timeout: config.service_idle_timeout,
            listener,
            state,
            changes,
            startup_index,
            _lock: lock,
        })
    }

    pub fn runtime(&self) -> &RuntimeFile {
        &self.state.runtime
    }

    /// Serves until the idle timeout passes or Ctrl-C, then removes the runtime file.
    pub async fn run(self) -> Result<(), ServeError> {
        let (router, _) = router().split_for_parts();
        let app = router
            .layer(axum::middleware::from_fn_with_state(
                Arc::clone(&self.state),
                guard::guard,
            ))
            .with_state(Arc::clone(&self.state));

        tokio::spawn(watcher::index_changes(
            Arc::clone(&self.state),
            self.changes,
        ));
        tokio::spawn(self.startup_index.run(IndexMode::Changed));

        let shutdown = self.state.shutdown.clone();
        let server = tokio::spawn(
            axum::serve(self.listener, app)
                .with_graceful_shutdown(shutdown.clone().cancelled_owned())
                .into_future(),
        );

        let stopped = tokio::select! {
            () = self.state.activity.idle(self.idle_timeout) => Ok(()),
            signal = tokio::signal::ctrl_c() => signal.map_err(ServeError::Signal),
        };
        shutdown.cancel();

        let served = server
            .await
            .map_err(ServeError::ServerPanic)
            .and_then(|result| result.map_err(ServeError::Server));
        let removed = RuntimeFile::remove(&self.data_dir).map_err(ServeError::from);

        stopped.and(served).and(removed)
    }
}

fn token() -> Result<String, ServeError> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(ServeError::Token)?;

    Ok(hex::encode(bytes))
}
