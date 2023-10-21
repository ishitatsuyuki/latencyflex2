use std::collections::{HashMap, VecDeque};
use std::time::Duration;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReflexId(pub u64);

pub struct ReflexMappingTracker<F> {
    need_recalibrate: bool,
    reflex_id_to_frame: HashMap<ReflexId, TrackedFrame<F>>,
    frame_queue: VecDeque<F>,
    current_render_frame: Option<TrackedFrame<F>>,
}

#[derive(Clone)]
enum TrackedFrame<F> {
    Tracked(F),
    Untracked,
}

impl<F: Clone + Eq> ReflexMappingTracker<F> {
    const RECALIBRATION_SLEEP: Duration = Duration::from_millis(200);

    pub fn new() -> Self {
        Self {
            need_recalibrate: false,
            reflex_id_to_frame: Default::default(),
            frame_queue: VecDeque::new(),
            current_render_frame: None,
        }
    }

    pub fn recalibrate(&mut self) {
        if self.need_recalibrate {
            eprintln!("Recalibrating");
            self.frame_queue.clear();
            std::thread::sleep(Self::RECALIBRATION_SLEEP);
            self.need_recalibrate = false;
        }
    }

    pub fn add_frame(&mut self, frame: F) {
        self.frame_queue.push_back(frame);
        if self.frame_queue.len() > 8 && !self.need_recalibrate {
            eprintln!("Frame queue is too long");
            self.need_recalibrate = true;
        }
    }

    pub fn mark_simulation_begin(&mut self, frame_id: ReflexId) {
        let frame = self.frame_queue.pop_back();
        let tracked = match frame {
            Some(frame) => TrackedFrame::Tracked(frame.clone()),
            None => TrackedFrame::Untracked,
        };
        self.reflex_id_to_frame.insert(frame_id, tracked);
    }

    pub fn mark_render_begin(&mut self, frame_id: ReflexId) {
        self.current_render_frame = self.reflex_id_to_frame.get(&frame_id).cloned();
    }

    pub fn present(&mut self, frame_id: ReflexId) {
        self.reflex_id_to_frame.retain(|k, _| k > &frame_id);
    }

    pub fn get_render_frame(&mut self) -> Option<F> {
        self.current_render_frame
            .clone()
            .or_else(|| {
                let ret = self.frame_queue.pop_front().map(TrackedFrame::Tracked);
                if ret.is_none() {
                    self.need_recalibrate = true;
                }
                ret
            })
            .and_then(|frame| match frame {
                TrackedFrame::Tracked(frame) => Some(frame),
                TrackedFrame::Untracked => None,
            })
    }

    pub fn get(&mut self, frame_id: ReflexId) -> Option<F> {
        self.reflex_id_to_frame
            .get(&frame_id)
            .and_then(|frame| match frame {
                TrackedFrame::Tracked(frame) => Some(frame.clone()),
                TrackedFrame::Untracked => None,
            })
    }
}
