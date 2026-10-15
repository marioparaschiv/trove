use std::{
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Duration,
};

use tokio::{sync::Notify, time::Instant};

/// Tracks work that keeps the service alive: requests, and queued index runs and imports.
#[derive(Clone)]
pub(crate) struct Activity(Arc<Inner>);

struct Inner {
    state: Mutex<State>,
    changed: Notify,
}

#[derive(Clone, Copy)]
struct State {
    active: usize,
    last_finished: Instant,
}

/// Keeps the service from going idle until dropped.
pub(crate) struct ActivityGuard(Activity);

impl Activity {
    pub(crate) fn new() -> Self {
        Self(Arc::new(Inner {
            state: Mutex::new(State {
                active: 0,
                last_finished: Instant::now(),
            }),
            changed: Notify::new(),
        }))
    }

    pub(crate) fn begin(&self) -> ActivityGuard {
        self.state().active += 1;
        self.0.changed.notify_one();

        ActivityGuard(self.clone())
    }

    /// Resolves once nothing has been active for `timeout`.
    pub(crate) async fn idle(&self, timeout: Duration) {
        loop {
            let state = *self.state();
            // `notify_one` stores a permit when nobody waits, so a change made between reading
            // the state and awaiting below still wakes this loop.
            if state.active > 0 {
                self.0.changed.notified().await;
                continue;
            }

            let deadline = state.last_finished + timeout;
            if Instant::now() >= deadline {
                return;
            }

            tokio::select! {
                () = tokio::time::sleep_until(deadline) => {}
                () = self.0.changed.notified() => {}
            }
        }
    }

    // The state is two plain values that are always consistent, so a panic elsewhere while
    // holding the lock can't leave it half-updated.
    fn state(&self) -> MutexGuard<'_, State> {
        self.0.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Drop for ActivityGuard {
    fn drop(&mut self) {
        let mut state = self.0.state();
        state.active -= 1;
        state.last_finished = Instant::now();
        drop(state);

        self.0.0.changed.notify_one();
    }
}
