use std::collections::BTreeMap;

use crate::task::FrameStageStats;
use crate::{ewma::EwmaEstimator, time, Interval, Timestamp};

#[derive(Copy, Clone, Debug, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub struct FrameId(u64);
#[derive(Copy, Clone, Debug, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub struct StageId(pub usize);

#[derive(Copy, Clone)]
pub struct Config {
    pub delay_gain: f64,
    pub duration_gain: f64,
    pub target_delay: u64,
    pub clamp_delay: u64,
    pub clamp_frame_time: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            delay_gain: 0.15,
            duration_gain: 0.3,
            target_delay: 2_000_000,
            clamp_delay: 50_000_000,
            clamp_frame_time: 50_000_000,
        }
    }
}

pub struct FrameAggregator {
    config: Config,

    stages: Vec<Stage>,
    frames: BTreeMap<FrameId, Frame>,
    next_frame_id: FrameId,
    reference_delay: Option<u64>,
    last_frame_start: Option<u64>,
}

impl FrameAggregator {
    pub fn new(config: Config, num_stages: usize) -> Self {
        Self {
            config,
            stages: (0..num_stages)
                .map(|_| Stage {
                    active: false,
                    next_frame_id: FrameId(0),
                    frame_queue: Default::default(),
                    duration_estimator: EwmaEstimator::new(config.duration_gain),
                })
                .collect(),
            frames: BTreeMap::new(),
            next_frame_id: FrameId(0),
            reference_delay: None,
            last_frame_start: None,
        }
    }

    fn update_estimates(&mut self) {
        while self.frames.first_key_value().is_some_and(|x| x.1.complete) {
            let (_, frame) = self.frames.pop_first().unwrap();
            let delay: u64 = frame.delay.iter().filter_map(|x| *x).sum();
            self.reference_delay = Some(delay);
        }

        for stage in &mut self.stages {
            stage.update_estimates();
        }
    }

    fn estimate_delay(&self) -> Option<u64> {
        self.reference_delay.map(|reference_delay| {
            self.frames
                .iter()
                .fold(reference_delay as i64, |acc, (_, frame)| {
                    (acc - frame.adjustment).max(0)
                })
                .min(self.config.clamp_delay as i64) as u64
        })
    }

    fn estimate_frame_time(&self) -> u64 {
        self.stages
            .iter()
            .filter(|s| s.active)
            .map(|s| s.duration_estimator.get())
            .sum::<f64>() as u64
    }

    pub fn new_frame(&mut self) -> (FrameId, Timestamp) {
        self.update_estimates();

        let id = self.next_frame_id;
        self.next_frame_id.0 += 1;

        let now = time::now();
        let (target, adjustment) = if let Some(last_frame_start) = self.last_frame_start {
            let predicted_delay_from_target = self
                .estimate_delay()
                .map(|delay| delay as i64 - self.config.target_delay as i64)
                .unwrap_or(0);
            let adjustment = (predicted_delay_from_target as f64 * self.config.delay_gain) as i64;
            let frame_time = self.estimate_frame_time();
            let target = now.max((last_frame_start + frame_time).saturating_add_signed(adjustment));
            let adjustment = (target - last_frame_start) as i64 - frame_time as i64;
            (target, adjustment)
        } else {
            (now, 0)
        };

        self.frames.insert(
            id,
            Frame {
                id,
                adjustment,
                delay: vec![None; self.stages.len()],
                complete: false,
            },
        );
        self.last_frame_start = Some(target);

        (id, target)
    }

    pub fn mark(&mut self, frame: FrameId, stage: StageId, stats: FrameStageStats) {
        let frame = self.frames.get_mut(&frame).unwrap();
        frame.delay[stage.0] = Some(stats.delay);
        self.stages[stage.0].update_duration(
            frame.id,
            Some(stats.duration.min(self.config.clamp_frame_time)),
        );
    }

    pub fn finish_frame(&mut self, frame: FrameId) {
        let frame = self.frames.get_mut(&frame).unwrap();
        frame.complete = true;
        for (stage, delay) in frame.delay.iter().enumerate() {
            if delay.is_none() {
                self.stages[stage].update_duration(frame.id, None);
            }
        }
    }
}

struct Frame {
    id: FrameId,
    delay: Vec<Option<Interval>>,
    adjustment: i64,
    complete: bool,
}

struct Stage {
    active: bool,
    next_frame_id: FrameId,
    frame_queue: BTreeMap<FrameId, Option<Interval>>,
    duration_estimator: EwmaEstimator,
}

impl Stage {
    fn update_estimates(&mut self) {
        while self.frame_queue.first_key_value().map(|x| *x.0) == Some(self.next_frame_id) {
            let (_, duration) = self.frame_queue.pop_first().unwrap();
            if let Some(duration) = duration {
                self.duration_estimator.update(duration as f64);
            }
            self.active = duration.is_some();
            self.next_frame_id.0 += 1;
        }
    }

    fn update_duration(&mut self, frame: FrameId, duration: Option<Interval>) {
        assert!(self.frame_queue.insert(frame, duration).is_none());
    }
}
