use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use tokio::sync::oneshot;
use xrtranslate_protocol::InferenceWorkload;

const REALTIME_BURST_LIMIT: usize = 8;

#[derive(Clone)]
pub(crate) struct InferenceScheduler {
    asr: PriorityLimiter,
    translation: PriorityLimiter,
}

impl InferenceScheduler {
    pub(crate) fn new(asr_slots: usize, translation_slots: usize) -> Self {
        Self {
            asr: PriorityLimiter::new(asr_slots),
            translation: PriorityLimiter::new(translation_slots),
        }
    }

    pub(crate) async fn acquire_asr(&self, workload: InferenceWorkload) -> InferencePermit {
        self.asr.acquire(workload).await
    }

    pub(crate) async fn acquire_translation(&self, workload: InferenceWorkload) -> InferencePermit {
        self.translation.acquire(workload).await
    }
}

#[derive(Clone)]
struct PriorityLimiter {
    inner: Arc<LimiterInner>,
}

struct LimiterInner {
    state: Mutex<LimiterState>,
}

struct LimiterState {
    available: usize,
    realtime_burst: usize,
    realtime: VecDeque<oneshot::Sender<()>>,
    offline: VecDeque<oneshot::Sender<()>>,
}

impl PriorityLimiter {
    fn new(slots: usize) -> Self {
        assert!(slots > 0, "inference scheduler requires at least one slot");
        Self {
            inner: Arc::new(LimiterInner {
                state: Mutex::new(LimiterState {
                    available: slots,
                    realtime_burst: 0,
                    realtime: VecDeque::new(),
                    offline: VecDeque::new(),
                }),
            }),
        }
    }

    async fn acquire(&self, workload: InferenceWorkload) -> InferencePermit {
        let receiver = {
            let mut state = self.inner.state.lock().expect("scheduler lock poisoned");
            if state.available > 0 && state.realtime.is_empty() {
                state.available -= 1;
                None
            } else {
                let (sender, receiver) = oneshot::channel();
                match workload {
                    InferenceWorkload::Realtime => state.realtime.push_back(sender),
                    InferenceWorkload::Offline => state.offline.push_back(sender),
                }
                Some(receiver)
            }
        };
        if let Some(receiver) = receiver {
            receiver
                .await
                .expect("inference scheduler closed while a permit was queued");
        }
        InferencePermit {
            limiter: Arc::clone(&self.inner),
        }
    }
}

pub(crate) struct InferencePermit {
    limiter: Arc<LimiterInner>,
}

impl Drop for InferencePermit {
    fn drop(&mut self) {
        self.limiter.release();
    }
}

impl LimiterInner {
    fn release(&self) {
        let mut state = self.state.lock().expect("scheduler lock poisoned");
        loop {
            let prefer_offline = !state.offline.is_empty()
                && (state.realtime.is_empty() || state.realtime_burst >= REALTIME_BURST_LIMIT);
            let next = if prefer_offline {
                state.realtime_burst = 0;
                state.offline.pop_front()
            } else if let Some(sender) = state.realtime.pop_front() {
                state.realtime_burst += 1;
                Some(sender)
            } else {
                state.realtime_burst = 0;
                state.offline.pop_front()
            };
            let Some(next) = next else {
                state.available += 1;
                return;
            };
            if next.send(()).is_ok() {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[tokio::test]
    async fn realtime_overtakes_queued_offline_work() {
        let limiter = PriorityLimiter::new(1);
        let active = limiter.acquire(InferenceWorkload::Offline).await;
        let (completed_tx, mut completed_rx) = tokio::sync::mpsc::unbounded_channel();

        let offline = limiter.clone();
        let offline_tx = completed_tx.clone();
        let offline_task = tokio::spawn(async move {
            let _permit = offline.acquire(InferenceWorkload::Offline).await;
            offline_tx.send("offline").unwrap();
        });
        tokio::task::yield_now().await;

        let realtime = limiter.clone();
        let realtime_task = tokio::spawn(async move {
            let _permit = realtime.acquire(InferenceWorkload::Realtime).await;
            completed_tx.send("realtime").unwrap();
        });
        tokio::task::yield_now().await;

        drop(active);
        assert_eq!(completed_rx.recv().await, Some("realtime"));
        assert_eq!(completed_rx.recv().await, Some("offline"));
        offline_task.await.unwrap();
        realtime_task.await.unwrap();
    }

    #[tokio::test]
    async fn configured_slot_limit_is_never_exceeded() {
        let limiter = PriorityLimiter::new(2);
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for _ in 0..12 {
            let limiter = limiter.clone();
            let active = Arc::clone(&active);
            let maximum = Arc::clone(&maximum);
            tasks.push(tokio::spawn(async move {
                let _permit = limiter.acquire(InferenceWorkload::Realtime).await;
                let current = active.fetch_add(1, Ordering::AcqRel) + 1;
                maximum.fetch_max(current, Ordering::AcqRel);
                tokio::task::yield_now().await;
                active.fetch_sub(1, Ordering::AcqRel);
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
        assert_eq!(maximum.load(Ordering::Acquire), 2);
    }

    #[tokio::test]
    async fn offline_work_runs_after_a_bounded_realtime_burst() {
        let limiter = PriorityLimiter::new(1);
        let active = limiter.acquire(InferenceWorkload::Realtime).await;
        let (completed_tx, mut completed_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut tasks = Vec::new();

        let offline = limiter.clone();
        let offline_tx = completed_tx.clone();
        tasks.push(tokio::spawn(async move {
            let _permit = offline.acquire(InferenceWorkload::Offline).await;
            offline_tx.send(InferenceWorkload::Offline).unwrap();
        }));
        tokio::task::yield_now().await;

        for _ in 0..9 {
            let realtime = limiter.clone();
            let realtime_tx = completed_tx.clone();
            tasks.push(tokio::spawn(async move {
                let _permit = realtime.acquire(InferenceWorkload::Realtime).await;
                realtime_tx.send(InferenceWorkload::Realtime).unwrap();
            }));
            tokio::task::yield_now().await;
        }

        drop(active);
        for _ in 0..REALTIME_BURST_LIMIT {
            assert_eq!(completed_rx.recv().await, Some(InferenceWorkload::Realtime));
        }
        assert_eq!(completed_rx.recv().await, Some(InferenceWorkload::Offline));
        assert_eq!(completed_rx.recv().await, Some(InferenceWorkload::Realtime));
        for task in tasks {
            task.await.unwrap();
        }
    }
}
