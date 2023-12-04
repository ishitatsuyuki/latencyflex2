use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex};

use crate::{FrameId, TaskStats};

struct TimelineContext {
    last_finished_frame_id: AtomicU64,
}

#[derive(Copy, Clone, Debug)]
struct WaitResult {
    frame_id: FrameId,
    result: TaskStats,
}

struct AsyncTimelineInner {
    registered: BTreeMap<u64, FrameId>,
    signaled: BTreeMap<u64, WaitResult>,
}

struct AsyncTimeline {
    inner: Mutex<AsyncTimelineInner>,
    value_cond: Condvar,
}

impl AsyncTimeline {
    pub fn register(&self, frame_id: FrameId, value: u64) {
        let mut inner = self.inner.lock().unwrap();
        inner.registered.insert(value, frame_id);
    }

    pub fn signal(&self, frame_id: FrameId, value: u64, result: TaskStats) {
        let mut inner = self.inner.lock().unwrap();
        inner
            .signaled
            .insert(value, WaitResult { frame_id, result });
        self.value_cond.notify_all();
    }

    fn cleanup(&self, context: &TimelineContext) {
        while let Some(e) = self.inner.lock().unwrap().registered.first_entry() {
            if *e.get() > FrameId(context.last_finished_frame_id.load(Ordering::Acquire)) {
                break;
            }
            e.remove();
        }
        while let Some(e) = self.inner.lock().unwrap().signaled.first_entry() {
            if e.get().frame_id > FrameId(context.last_finished_frame_id.load(Ordering::Acquire)) {
                break;
            }
            e.remove();
        }
    }

    pub fn get(&self, value: u64) -> Option<WaitResult> {
        let inner = self.inner.lock().unwrap();
        if !inner.registered.contains_key(&value) {
            return None;
        }
        let mut signaled = None;
        let _ = self
            .value_cond
            .wait_while(inner, |inner| {
                signaled = inner.signaled.get(&value).copied();
                signaled.is_none()
            })
            .unwrap();
        signaled
    }
}
