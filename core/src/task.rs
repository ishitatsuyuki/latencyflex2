#[derive(Default)]
#[repr(C)]
pub struct FrameStageStats {
    pub delay: u64,
    pub duration: u64,
}

#[derive(Default)]
pub struct TaskAccumulator {
    delay: Option<u64>,
    duration: u64,
    last_finish: u64,
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct TaskStats {
    pub queued: u64,
    pub start: u64,
    pub end: u64,
}

impl TaskAccumulator {
    pub fn accumulate(&mut self, stats: &TaskStats) {
        let task_delay = stats.start - stats.queued;
        self.delay = Some(self.delay.map_or(task_delay, |qd| qd.min(task_delay)));
        let task_duration = stats.end - stats.queued.max(self.last_finish);
        self.duration += task_duration;
        self.last_finish = stats.end;
    }

    pub fn stats(&self) -> FrameStageStats {
        FrameStageStats {
            delay: self.delay.unwrap_or(0),
            duration: self.duration,
        }
    }

    /// Reset accumulated delay and duration, keeping internal state needed across frames.
    pub fn reset(&mut self) {
        self.delay = None;
        self.duration = 0;
    }
}
