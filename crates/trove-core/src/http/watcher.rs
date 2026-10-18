use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};
use tokio::{sync::mpsc, time::Instant};

use super::{
    AppState,
    indexing::{Job, import_vector_files},
};
use crate::{error::full_message, indexer::IndexMode, sync_store::vector_file_name};

pub(super) type Changes = mpsc::UnboundedReceiver<notify::Result<notify::Event>>;

/// Watches the source folders for changes to index.
pub(super) struct Watcher(Mutex<Inner>);

struct Inner {
    watcher: RecommendedWatcher,
    roots: Vec<Root>,
}

struct Root {
    source: PathBuf,
    /// The paths events may name the folder by: as registered, and resolved (macOS reports `/var/...` as
    /// `/private/var/...`).
    spellings: Vec<PathBuf>,
}

impl Watcher {
    pub(super) fn new() -> notify::Result<(Self, Changes)> {
        let (sender, changes) = mpsc::unbounded_channel();
        let watcher = notify::recommended_watcher(move |change| {
            // Fails only once the service is stopping and no longer reads changes.
            let _ = sender.send(change);
        })?;
        let inner = Inner {
            watcher,
            roots: Vec::new(),
        };

        Ok((Self(Mutex::new(inner)), changes))
    }

    /// Watches a source folder. One that can't be watched is reported on stderr and still indexed by other runs.
    pub(super) fn watch(&self, source: &str) {
        let source = PathBuf::from(source);
        let mut inner = self.inner();
        if let Err(error) = inner.watcher.watch(&source, RecursiveMode::Recursive) {
            eprintln!(
                "warning: can't watch {}: {}",
                source.display(),
                full_message(&error)
            );
            return;
        }

        let mut spellings = vec![source.clone()];
        // Without the resolved spelling, events under it still count as changes, just without filtering.
        if let Ok(resolved) = fs::canonicalize(&source)
            && resolved != source
        {
            spellings.push(resolved);
        }
        inner.roots.push(Root { source, spellings });
    }

    pub(super) fn unwatch(&self, source: &str) {
        let source = Path::new(source);
        let mut inner = self.inner();
        let watched = inner.roots.len();
        inner.roots.retain(|root| root.source != source);
        if inner.roots.len() == watched {
            return;
        }

        if let Err(error) = inner.watcher.unwatch(source) {
            eprintln!(
                "warning: can't stop watching {}: {}",
                source.display(),
                full_message(&error)
            );
        }
    }

    /// Adds what `change` touches to `batch` and returns whether it touches anything: notes, or vector files in the
    /// store folder. Reads, and other paths in hidden folders or the store folder, don't count.
    fn collect(
        &self,
        change: &notify::Result<notify::Event>,
        store_folder: &str,
        batch: &mut Batch,
    ) -> bool {
        let event = match change {
            Ok(event) => event,
            Err(error) => {
                eprintln!("warning: watching sources failed: {}", full_message(error));
                batch.notes = true;
                return true;
            }
        };
        if event.need_rescan() {
            batch.notes = true;
            return true;
        }
        // Indexing reads every changed note, so counting reads would index forever.
        if matches!(event.kind, EventKind::Access(_)) {
            return false;
        }

        let inner = self.inner();
        let mut relevant = false;
        for path in &event.paths {
            match inner.classify(path, store_folder) {
                Touches::Notes => batch.notes = true,
                Touches::VectorFile => {
                    batch.vector_files.insert(path.clone());
                }
                Touches::Nothing => continue,
            }
            relevant = true;
        }

        relevant
    }

    // The watcher and its roots change together and stay consistent if a holder panics.
    fn inner(&self) -> MutexGuard<'_, Inner> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// What the changes seen during one debounce touched.
#[derive(Default)]
struct Batch {
    notes: bool,
    vector_files: BTreeSet<PathBuf>,
}

enum Touches {
    Notes,
    VectorFile,
    Nothing,
}

impl Inner {
    fn classify(&self, path: &Path, store_folder: &str) -> Touches {
        let Some(relative) = self
            .roots
            .iter()
            .flat_map(|root| &root.spellings)
            .find_map(|root| path.strip_prefix(root).ok())
        else {
            return Touches::Notes;
        };
        if vector_file_name(relative, store_folder).is_some() {
            return Touches::VectorFile;
        }

        let hidden = relative.components().any(|component| match component {
            Component::Normal(name) => name
                .to_str()
                .is_some_and(|name| name.starts_with('.') || name == store_folder),
            _ => false,
        });
        if hidden {
            Touches::Nothing
        } else {
            Touches::Notes
        }
    }
}

/// Indexes changed notes, then imports vector files that arrived in the store folders, once the sources have been
/// quiet for `index_debounce`, until the service stops.
pub(super) async fn index_changes(state: Arc<AppState>, mut changes: Changes) {
    let collect = |change: &notify::Result<notify::Event>, batch: &mut Batch| {
        state
            .watcher
            .collect(change, &state.config.store_folder, batch)
    };

    while let Some(change) = next_change(&state, &mut changes).await {
        let mut batch = Batch::default();
        if !collect(&change, &mut batch) {
            continue;
        }

        let _activity = state.activity.begin();
        let mut job = batch.notes.then(|| Job::queue(&state));
        let mut quiet_until = Instant::now() + state.config.index_debounce;
        loop {
            tokio::select! {
                () = tokio::time::sleep_until(quiet_until) => break,
                change = next_change(&state, &mut changes) => match change {
                    Some(change) if collect(&change, &mut batch) => {
                        quiet_until = Instant::now() + state.config.index_debounce;
                        if batch.notes && job.is_none() {
                            job = Some(Job::queue(&state));
                        }
                    }
                    Some(_) => {}
                    None => return,
                },
            }
        }

        // Failures are reported on the events stream and stderr, and the next change tries again.
        if let Some(job) = job {
            let _ = job.run(IndexMode::Changed).await;
        }
        if !batch.vector_files.is_empty() {
            import_vector_files(&state, batch.vector_files.into_iter().collect()).await;
        }
    }
}

async fn next_change(
    state: &AppState,
    changes: &mut Changes,
) -> Option<notify::Result<notify::Event>> {
    tokio::select! {
        () = state.shutdown.cancelled() => None,
        change = changes.recv() => change,
    }
}
