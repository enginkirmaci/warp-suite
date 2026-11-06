//! Bounded fan-out for the background passes: up to `max_parallelism`
//! workers at once, so a long list never spawns one child per entry.
//!
//! The old `RefreshCoalescer` here was never used by a caller; its coalescing
//! test outlived the abstraction. The active passes are driven from the net
//! thread's background lane instead (audit C8).

use std::future::Future;
use std::sync::Arc;

/// How many `git fetch` children a fetch pass runs at once. Fetch is
/// network-bound (the local rescan stays on its own, faster track), so a
/// wider fan-out shrinks the wall-clock pass without contending for cores.
pub const FETCH_PARALLELISM: usize = 8;

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
