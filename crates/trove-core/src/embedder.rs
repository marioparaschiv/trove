use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, TryLockError, mpsc},
    thread,
    time::{Duration, Instant},
};

use sha2::{Digest, Sha256};
use thiserror::Error;
use trove_llama::{EmbeddingModel, LlamaError};

pub use trove_llama::{Devices, LLAMA_CPP_TAG, has_gpu};

use crate::{config::Config, data_dir::DataDir, indexer::Chunker};

/// Tokens per input; context, batch and micro-batch are raised together, up to 8192.
pub const CONTEXT_TOKENS: u32 = 2048;

const QUERY_PREFIX: &str = "task: search result | query: ";
const DOCUMENT_TITLE_PREFIX: &str = "title: ";
const DOCUMENT_TEXT_PREFIX: &str = " | text: ";
const NO_TITLE: &str = "none";
/// Query vectors kept for queries asked again, as when typing and deleting.
const QUERY_CACHE_SIZE: usize = 64;

/// One text to embed, framed with the model card's prompt for its role.
#[derive(Debug, Clone, Copy)]
pub enum Input<'a> {
    Query(&'a str),
    /// `title` is the chunk's heading path or the note title; empty when there is none.
    Document {
        title: &'a str,
        text: &'a str,
    },
}

impl Input<'_> {
    fn prompt(&self) -> String {
        match self {
            Self::Query(text) => format!("{QUERY_PREFIX}{text}"),
            Self::Document { title, text } => {
                let title = if title.is_empty() { NO_TITLE } else { title };
                format!("{DOCUMENT_TITLE_PREFIX}{title}{DOCUMENT_TEXT_PREFIX}{text}")
            }
        }
    }
}

/// Identifies everything that shapes vectors: the model file (by its SHA-256), the llama.cpp tag, the prompts and the
/// chunker version. Lowercase hex.
pub fn vector_version(model_sha256: &str) -> String {
    let mut hasher = Sha256::new();
    for part in [
        model_sha256,
        LLAMA_CPP_TAG,
        QUERY_PREFIX,
        DOCUMENT_TITLE_PREFIX,
        DOCUMENT_TEXT_PREFIX,
        NO_TITLE,
        &Chunker::VERSION.to_string(),
    ] {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }

    hex::encode(hasher.finalize())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelEvent {
    Loaded,
    Unloaded,
}

#[derive(Debug, Error)]
#[error("embedding model failed")]
pub struct EmbedderError(#[from] LlamaError);

/// The embedding model, loaded by the first call that needs it and unloaded after `idle_timeout` without calls or on
/// request.
pub struct Embedder {
    shared: Arc<Shared>,
}

struct Shared {
    model_path: PathBuf,
    devices: Devices,
    idle_timeout: Duration,
    state: Mutex<State>,
    state_changed: Condvar,
    listeners: Mutex<Vec<mpsc::Sender<ModelEvent>>>,
}

struct State {
    model: Option<EmbeddingModel>,
    /// Vectors the loaded model made for recent queries.
    query_vectors: QueryCache,
    last_used: Instant,
    // Tells an idle watcher whether the model it was started for is still the loaded one.
    generation: u64,
}

impl Embedder {
    /// The model `trove setup` installs in `data_dir`, unloaded after the configured idle time.
    pub fn from_config(data_dir: &DataDir, config: &Config) -> Self {
        Self::new(config.model.path_in(data_dir), config.model_idle_timeout)
    }

    pub fn new(model_path: PathBuf, idle_timeout: Duration) -> Self {
        Self::on_devices(model_path, idle_timeout, Devices::Default)
    }

    pub fn on_devices(model_path: PathBuf, idle_timeout: Duration, devices: Devices) -> Self {
        Self {
            shared: Arc::new(Shared {
                model_path,
                devices,
                idle_timeout,
                state: Mutex::new(State {
                    model: None,
                    query_vectors: QueryCache::default(),
                    last_used: Instant::now(),
                    generation: 0,
                }),
                state_changed: Condvar::new(),
                listeners: Mutex::new(Vec::new()),
            }),
        }
    }

    /// Loads the model now (prewarm) and restarts the idle timeout.
    pub fn load(&self) -> Result<(), EmbedderError> {
        drop(self.shared.loaded_state()?);
        Ok(())
    }

    /// One unit-length vector per input, in order.
    pub fn embed(&self, inputs: &[Input<'_>]) -> Result<Vec<Vec<f32>>, EmbedderError> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }

        let prompts: Vec<String> = inputs.iter().map(Input::prompt).collect();
        let texts: Vec<&str> = prompts.iter().map(String::as_str).collect();

        let mut state = self.shared.loaded_state()?;
        let model = state
            .model
            .as_mut()
            .expect("loaded_state returns a loaded model");
        let vectors = model.embed(&texts)?;
        state.last_used = Instant::now();

        Ok(vectors)
    }

    /// The vector of a search query, made again only for a query the loaded model hasn't seen lately.
    pub fn embed_query(&self, text: &str) -> Result<Arc<[f32]>, EmbedderError> {
        let mut state = self.shared.loaded_state()?;
        if let Some(vector) = state.query_vectors.get(text) {
            return Ok(vector);
        }

        let model = state
            .model
            .as_mut()
            .expect("loaded_state returns a loaded model");
        let vector: Arc<[f32]> = model
            .embed(&[&Input::Query(text).prompt()])?
            .pop()
            .expect("one vector per input")
            .into();
        state.last_used = Instant::now();
        state.query_vectors.insert(text, Arc::clone(&vector));

        Ok(vector)
    }

    /// The tokens `input` takes as a prompt, uncut: more than `CONTEXT_TOKENS` means `embed` would cut it. Loads the
    /// model, which holds the vocabulary.
    pub fn count_tokens(&self, input: Input<'_>) -> Result<usize, EmbedderError> {
        let mut state = self.shared.loaded_state()?;
        let model = state
            .model
            .as_ref()
            .expect("loaded_state returns a loaded model");
        let count = model.count_tokens(&input.prompt())?;
        state.last_used = Instant::now();

        Ok(count)
    }

    pub fn unload(&self) {
        let mut state = self.shared.lock_state();
        if state.unload_model() {
            self.shared.state_changed.notify_all();
            self.shared.notify(ModelEvent::Unloaded);
        }
    }

    pub fn is_loaded(&self) -> bool {
        match self.shared.state.try_lock() {
            Ok(state) => state.model.is_some(),
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner().model.is_some(),
            // Held for a load or an embed batch, either of which ends with the model loaded; waiting would hold up
            // `/status` for a whole batch.
            Err(TryLockError::WouldBlock) => true,
        }
    }

    /// Receives every later load and unload.
    pub fn subscribe(&self) -> mpsc::Receiver<ModelEvent> {
        let (sender, receiver) = mpsc::channel();
        self.shared.lock_listeners().push(sender);
        receiver
    }
}

impl Drop for Embedder {
    fn drop(&mut self) {
        self.unload();
    }
}

impl State {
    /// Drops the model and the query vectors it made. Returns whether a model was loaded.
    fn unload_model(&mut self) -> bool {
        self.query_vectors = QueryCache::default();
        self.model.take().is_some()
    }
}

/// The last `QUERY_CACHE_SIZE` query vectors, by exact query text, least recently used first.
#[derive(Default)]
struct QueryCache(VecDeque<(String, Arc<[f32]>)>);

impl QueryCache {
    fn get(&mut self, text: &str) -> Option<Arc<[f32]>> {
        let at = self.0.iter().position(|(cached, _)| cached == text)?;
        let entry = self.0.remove(at)?;
        let vector = Arc::clone(&entry.1);
        self.0.push_back(entry);

        Some(vector)
    }

    fn insert(&mut self, text: &str, vector: Arc<[f32]>) {
        if self.0.len() == QUERY_CACHE_SIZE {
            self.0.pop_front();
        }
        self.0.push_back((text.to_owned(), vector));
    }
}

impl Shared {
    fn loaded_state(self: &Arc<Self>) -> Result<MutexGuard<'_, State>, LlamaError> {
        let mut state = self.lock_state();
        state.last_used = Instant::now();
        if state.model.is_some() {
            return Ok(state);
        }

        state.model = Some(EmbeddingModel::load(
            &self.model_path,
            CONTEXT_TOKENS,
            self.devices,
        )?);
        state.generation += 1;
        let generation = state.generation;
        let shared = Arc::clone(self);
        thread::spawn(move || shared.unload_when_idle(generation));
        self.notify(ModelEvent::Loaded);

        Ok(state)
    }

    fn unload_when_idle(&self, generation: u64) {
        let mut state = self.lock_state();
        while state.model.is_some() && state.generation == generation {
            let idle = state.last_used.elapsed();
            if idle >= self.idle_timeout {
                state.unload_model();
                self.notify(ModelEvent::Unloaded);
                return;
            }

            state = self
                .state_changed
                .wait_timeout(state, self.idle_timeout - idle)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    fn notify(&self, event: ModelEvent) {
        self.lock_listeners()
            .retain(|listener| listener.send(event).is_ok());
    }

    // A panic while embedding leaves the state whole: the model is either there or not.
    fn lock_state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn lock_listeners(&self) -> MutexGuard<'_, Vec<mpsc::Sender<ModelEvent>>> {
        self.listeners
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}
