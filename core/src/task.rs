use std::collections::HashMap;

use crate::FrameId;

#[derive(Default)]
#[repr(C)]
pub struct FrameStageStats {
    pub delay: u64,
    pub duration: u64,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct TaskStats {
    pub earliest: u64,
    pub actual: u64,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct TimelinePoint {
    pub task: TaskStats,
    pub signaled: u64,
}

#[derive(Default)]
pub struct Timeline {
    pub earliest: HashMap<FrameId, u64>,
    pub actual: u64,
    pub duration: HashMap<FrameId, u64>,
}

impl TaskStats {
    fn delay(&self) -> u64 {
        self.actual - self.earliest
    }
}

impl Timeline {
    pub fn accumulate(
        &mut self,
        frame_id: FrameId,
        deps: impl IntoIterator<Item = &TimelinePoint> + Clone,
        finish: Option<u64>,
    ) -> TaskStats {
        // TODO: Assert at least one dep
        let earliest = deps
            .clone()
            .into_iter()
            .map(|p: &TimelinePoint| p.signaled - p.task.delay())
            .chain(self.earliest.get(&frame_id).copied())
            .max();
        let actual = deps
            .into_iter()
            .map(|p: &TimelinePoint| p.signaled)
            .max()
            .unwrap_or(0);
        // TODO: Check or handle negative
        let duration = match finish {
            None => 0,
            Some(finish) => finish - actual,
        };
        if let Some(earliest) = earliest {
            self.earliest.insert(frame_id, earliest + duration);
        }
        self.actual = actual + duration;
        self.duration
            .entry(frame_id)
            .and_modify(|d| *d += duration)
            .or_insert(duration);
        TaskStats {
            earliest: earliest.unwrap_or(0),
            actual,
        }
    }

    /// Reset accumulated delay and duration, keeping internal state needed across frames.
    pub fn end_frame(&mut self, frame_id: FrameId) -> u64 {
        self.earliest.remove(&frame_id).unwrap_or(0)
        // TODO: Submit duration
    }
}
