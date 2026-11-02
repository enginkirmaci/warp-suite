//! Refresh coalescing for the background passes: while one pass runs,
//! additional triggers only flag a follow-up pass, so a burst of triggers
//! never stacks concurrent passes — and a caller joining mid-pass outwaits the
//! follow-up its trigger armed.

use std::future::Future;
use std::sync::{Arc, Mutex};

struct State {
    running: bool,
    pending: bool,
}

pub struct RefreshCoalescer {
    state: Arc<Mutex<State>>,
}

impl Default for RefreshCoalescer {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                running: false,
                pending: false,
            })),
        }
    }
}

impl RefreshCoalescer {
    /// Runs `pass` in a coalescing loop. A call arriving while a loop is
    /// running flags exactly one follow-up pass and joins the running loop —
    /// the returned future resolves only when the (re-armed) loop exits, so
    /// awaiting callers outwait the follow-up pass their trigger armed.
    pub fn run_coalesced<F, Fut>(&self, pass: F)
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send,
    {
        let mut guard = self.state.lock().unwrap();
        if guard.running {
            guard.pending = true;
            return;
        }
        guard.running = true;
        drop(guard);

        let state = self.state.clone();
        let pass = Arc::new(pass);
        tokio::spawn(async move {
            loop {
                {
                    let mut guard = state.lock().unwrap();
                    guard.pending = false;
                }
                pass().await;
                // The exit decision and the running-flag clear share one
                // critical section — a task's completion is not atomic with
                // the decision, so testing a task handle instead would let a
                // trigger arriving in between join a loop that never runs
                // another pass.
                let mut guard = state.lock().unwrap();
                if !guard.pending {
                    guard.running = false;
                    return;
                }
            }
        });
    }
}

/// Bounded fan-out over a snapshot of the repo list — the shared pass shape:
/// up to `max_parallelism` workers at once.
pub async fn throttled_pass<T, F, Fut>(items: Vec<T>, max_parallelism: usize, worker: F)
where
    T: Send + 'static,
    F: Fn(T) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send,
{
    let semaphore = Arc::new(tokio::sync::Semaphore::new(max_parallelism));
    let worker = Arc::new(worker);
    let mut tasks = Vec::with_capacity(items.len());
    for item in items {
        let permit = semaphore.clone();
        let worker = worker.clone();
        tasks.push(tokio::spawn(async move {
            let _permit = permit.acquire_owned().await;
            worker(item).await;
        }));
    }
    for task in tasks {
        let _ = task.await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn bursts_coalesce_into_follow_up_passes() {
        let coalescer = RefreshCoalescer::default();
        let passes = Arc::new(AtomicUsize::new(0));
        let in_pass = Arc::new(tokio::sync::Notify::new());

        let passes_for_loop = passes.clone();
        let notify = in_pass.clone();
        coalescer.run_coalesced(move || {
            let passes = passes_for_loop.clone();
            let notify = notify.clone();
            async move {
                passes.fetch_add(1, Ordering::SeqCst);
                notify.notify_one();
                tokio::time::sleep(Duration::from_millis(80)).await;
            }
        });

        // first pass starts
        in_pass.notified().await;
        // two triggers while it runs → exactly one follow-up pass
        coalescer.run_coalesced(|| async {});
        coalescer.run_coalesced(|| async {});
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(passes.load(Ordering::SeqCst), 2);
        // loop exited; a later trigger starts a fresh loop
        coalescer.run_coalesced(|| async {});
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(passes.load(Ordering::SeqCst), 2);
    }

    use std::time::Duration;

    #[tokio::test]
    async fn throttled_pass_runs_every_item_bounded() {
        let ran = Arc::new(AtomicUsize::new(0));
        let ran_in_flight = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));
        let items: Vec<usize> = (0..10).collect();
        let check_ran = ran.clone();
        let check_max = max_seen.clone();
        throttled_pass(items, 3, move |_item| {
            let ran = ran.clone();
            let ran_in_flight = ran_in_flight.clone();
            let max_seen = max_seen.clone();
            async move {
                let now = ran_in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                max_seen.fetch_max(now, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(30)).await;
                ran_in_flight.fetch_sub(1, Ordering::SeqCst);
                ran.fetch_add(1, Ordering::SeqCst);
            }
        })
        .await;
        assert_eq!(check_ran.load(Ordering::SeqCst), 10);
        assert!(check_max.load(Ordering::SeqCst) <= 3);
    }
}
