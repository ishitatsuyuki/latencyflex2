use std::collections::{hash_map, HashMap, VecDeque};
use std::time::Duration;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReflexId(pub u64);

pub struct ReflexMappingTracker<F> {
    need_recalibrate: bool,
    reflex_id_to_frame: HashMap<ReflexId, FrameState<F>>,
    frame_queue: VecDeque<F>,
    last_present: Option<ReflexId>,
}

#[derive(Clone)]
enum FrameState<F> {
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
            last_present: None,
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

    fn bind(&mut self, frame_id: ReflexId, allow_untracked: bool) -> Option<&FrameState<F>> {
        if self
            .last_present
            .is_some_and(|last_present| frame_id <= last_present)
        {
            return None;
        }
        let entry = self.reflex_id_to_frame.entry(frame_id);
        let entry = match entry {
            hash_map::Entry::Occupied(entry) => return Some(entry.into_mut()),
            hash_map::Entry::Vacant(entry) => entry,
        };
        let frame = self.frame_queue.pop_back();
        let tracked = match frame {
            Some(frame) => FrameState::Tracked(frame.clone()),
            None => {
                if allow_untracked {
                    FrameState::Untracked
                } else {
                    self.need_recalibrate = true;
                    return None;
                }
            }
        };
        Some(entry.insert(tracked.clone()))
    }

    pub fn add_frame(&mut self, frame: F) {
        self.frame_queue.push_back(frame);
        if self.frame_queue.len() > 8 && !self.need_recalibrate {
            eprintln!("Frame queue is too long");
            self.need_recalibrate = true;
        }
    }

    pub fn mark_simulation_begin(&mut self, frame_id: ReflexId) {
        self.bind(frame_id, true);
    }

    pub fn mark_render_begin(&mut self, _frame_id: ReflexId) {}

    pub fn present(&mut self, frame_id: ReflexId) {
        self.bind(frame_id, false);
        self.reflex_id_to_frame.retain(|k, _| k > &frame_id);
        self.last_present = Some(frame_id);
    }

    pub fn get(&mut self, frame_id: ReflexId) -> Option<F> {
        self.bind(frame_id, false).and_then(|frame| match frame {
            FrameState::Tracked(frame) => Some(frame.clone()),
            FrameState::Untracked => None,
        })
    }
}
