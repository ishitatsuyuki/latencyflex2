use std::{cmp, thread};
use std::collections::{BTreeMap, VecDeque};
use std::ptr::NonNull;
use std::sync::{Arc, Mutex, Once};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::ewma::EwmaEstimator;
use crate::profiler::Profiler;
use crate::time::*;

mod ewma;
mod profiler;
#[cfg_attr(unix, path = "time/unix.rs")]
#[cfg_attr(windows, path = "time/windows.rs")]
mod time;

type SectionId = u32;
type Timestamp = u64;

#[derive(Copy, Clone, Debug, PartialOrd, Ord, PartialEq, Eq, Hash)]
struct FrameId(u64);

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub enum MarkType {
    Begin,
    End,
}

#[derive(Default)]
pub struct Context {
    inner: Mutex<ContextInner>,
}

struct ContextInner {
    next_frame_id: FrameId,
    frames: BTreeMap<FrameId, FrameImpl>,
    reference_frame: Option<FrameImpl>,
    optimal_latency_estimator: EwmaEstimator,
    bandwidth_estimator: BTreeMap<SectionId, EwmaEstimator>,
    target_top_frame_time: Option<u64>,

    profiler: Profiler,
}

impl Default for ContextInner {
    fn default() -> Self {
        ContextInner {
            next_frame_id: FrameId(0),
            frames: BTreeMap::new(),
            reference_frame: None,
            optimal_latency_estimator: EwmaEstimator::new(0.7),
            bandwidth_estimator: BTreeMap::new(),
            profiler: Profiler::new(),
            target_top_frame_time: None,
        }
    }
}

/// A write handle for frame markers.
pub struct Frame {
    context: Arc<Context>,
    id: FrameId,
}

struct FrameImpl {
    writer_count: usize,
    predicted_begin: u64,
    predicted_duration: u64,
    marks: BTreeMap<(SectionId, MarkType), Timestamp>,
}

impl ContextInner {
    fn frames_iter(&self) -> impl DoubleEndedIterator<Item=&FrameImpl> {
        self.reference_frame.iter().chain(self.frames.values())
    }

    fn last_predicted_frame_end(&self) -> Option<Timestamp> {
        self.reference_frame.as_ref().map(|reference_frame| {
            reference_frame.end_ts()
                + self
                .frames
                .iter()
                .map(|(_, frame)| frame.predicted_duration)
                .sum::<u64>()
        })
    }

    fn prepare_frame(&mut self, context: Arc<Context>) -> (Arc<Frame>, Timestamp) {
        let predicted_duration = self
            .bandwidth_estimator
            .iter()
            .map(|(_, e)| e.get() as u64)
            .max()
            .unwrap_or(0);

        let bias = 1000000;
        let now = timestamp_now();
        let mut target = self
            .last_predicted_frame_end()
            .map(|predicted_frame_end| {
                predicted_frame_end + predicted_duration
                    - self.optimal_latency_estimator.get() as u64
                    - bias
            })
            .unwrap_or(now)
            .max(now);

        let last_frame_top = self.frames_iter().next_back().map(|f| f.predicted_begin);
        if let Some(last_frame_top) = last_frame_top {
            let top_interval = target - last_frame_top;
            if let Some(target_top_frame_time) = self.target_top_frame_time {
                const HALF_LIFE: f64 = 100_000_000.;
                let tolerance = 2f64.powf(target_top_frame_time as f64 / HALF_LIFE);
                let inv_tolerance = 2f64.powf(-(target_top_frame_time as f64) / HALF_LIFE);
                let max = (tolerance * target_top_frame_time as f64) as u64;
                let min = (inv_tolerance * target_top_frame_time as f64) as u64;
                let new_target_frame_time = top_interval.clamp(min, max);
                target = (last_frame_top + new_target_frame_time).max(now);
                self.target_top_frame_time = Some(new_target_frame_time);
            } else {
                self.target_top_frame_time = Some(top_interval);
            }
        }

        let id = self.next_frame_id;
        self.next_frame_id.0 += 1;

        self.frames.insert(
            id,
            FrameImpl {
                writer_count: 1,
                predicted_begin: target,
                predicted_duration,
                marks: Default::default(),
            },
        );

        static LEAK_WARN: Once = Once::new();
        const LEAK_WARN_THRESHOLD: usize = 16;
        if self.frames.len() > 16 {
            LEAK_WARN.call_once(|| {
                eprintln!("LFX2 WARN: More than {LEAK_WARN_THRESHOLD} frames in flight. Did you forget to call lfx2FrameRelease()?");
            });
        }

        let handle = Arc::new(Frame { context, id });

        (handle, target)
    }

    fn update_estimates(&mut self) {
        const MAX_FRAME_TIME: u64 = 50_000_000;
        const MAX_LATENCY: u64 = 200_000_000;

        while let Some((
                           _,
                           FrameImpl {
                               writer_count: 0, ..
                           },
                       )) = self.frames.first_key_value()
        {
            let (_, frame) = self.frames.pop_first().unwrap();

            if let Some(reference_frame) = &self.reference_frame {
                let queueing_delay = frame.queueing_delay(reference_frame);
                // Should not overflow, but for sanity
                let real_latency = frame.end_ts().saturating_sub(frame.begin_ts());
                // Again, should not overflow, but for sanity
                let optimal_latency = real_latency.saturating_sub(queueing_delay);
                self.optimal_latency_estimator
                    .update(cmp::min(optimal_latency, MAX_LATENCY) as f64);
                dbg!(
                    real_latency,
                    optimal_latency,
                    self.optimal_latency_estimator.get()
                );
            }

            for (section_id, duration) in frame.inverse_throughput().into_iter() {
                self.bandwidth_estimator
                    .entry(section_id)
                    .or_insert_with(|| EwmaEstimator::new(0.7))
                    .update(cmp::min(duration, MAX_FRAME_TIME) as f64);
            }

            self.reference_frame = Some(frame);
        }
    }
}

impl Frame {
    fn mark(&self, section_id: SectionId, mark_type: MarkType, timestamp: Timestamp) {
        let mut inner = self.context.inner.lock().unwrap();
        inner
            .frames
            .get_mut(&self.id)
            .unwrap()
            .mark(section_id, mark_type, timestamp);
        inner
            .profiler
            .mark(self.id, section_id, mark_type, timestamp);
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        let mut inner = self.context.inner.lock().unwrap();
        let frame = inner.frames.get_mut(&self.id).unwrap();
        frame.writer_count -= 1;
        if frame.writer_count == 0 {
            inner.update_estimates();
        }
    }
}

fn filter_marks_by_type(
    marks: &BTreeMap<(SectionId, MarkType), Timestamp>,
    mark_type: MarkType,
) -> Vec<(SectionId, Timestamp)> {
    marks
        .iter()
        .filter_map(|((section_id, mark_type_), timestamp)| {
            if *mark_type_ == mark_type {
                Some((*section_id, *timestamp))
            } else {
                None
            }
        })
        .collect()
}

impl FrameImpl {
    fn begin_ts(&self) -> Timestamp {
        self.marks.first_key_value().map(|x| *x.1).unwrap()
    }

    fn end_ts(&self) -> Timestamp {
        self.marks.last_key_value().map(|x| *x.1).unwrap()
    }

    fn mark(&mut self, section_id: SectionId, mark_type: MarkType, timestamp: Timestamp) {
        self.marks.insert((section_id, mark_type), timestamp);
    }

    fn queueing_delay(&self, reference: &FrameImpl) -> u64 {
        let ends = filter_marks_by_type(&self.marks, MarkType::End);
        let last_ends = filter_marks_by_type(&reference.marks, MarkType::End);
        let mut delays = Vec::new();
        for (section_id, handoff_time) in ends {
            let stage_after_idx =
                last_ends.partition_point(|&(other_section_id, _)| other_section_id <= section_id);
            if let Some(&(_, last_end_time)) = last_ends.get(stage_after_idx) {
                delays.push(last_end_time.saturating_sub(handoff_time));
            }
        }
        delays.into_iter().sum()
    }

    fn inverse_throughput(&self) -> BTreeMap<SectionId, u64> {
        let begins = filter_marks_by_type(&self.marks, MarkType::Begin);
        let ends = filter_marks_by_type(&self.marks, MarkType::End);
        ends.into_iter()
            .filter_map(|(section_id, timestamp)| {
                let other_timestamp_idx = begins.binary_search_by_key(&section_id, |&(id, _)| id);
                if let Ok(other_timestamp_idx) = other_timestamp_idx {
                    let (_, other_timestamp) = begins[other_timestamp_idx];
                    let duration = timestamp.saturating_sub(other_timestamp);
                    Some((section_id, duration))
                } else {
                    None
                }
            })
            .collect()
    }
}

#[derive(Default)]
pub struct ImplicitContext {
    inner: Mutex<ImplicitContextInner>,
    need_reset: AtomicBool,
}

#[derive(Default)]
struct ImplicitContextInner {
    context: Arc<Context>,
    frame_queue: VecDeque<Arc<Frame>>,
}

impl ImplicitContext {
    fn enqueue(&self) -> (Arc<Frame>, Timestamp) {
        const RESET_FLUSH_TIME: Duration = Duration::from_millis(200);
        const RENDER_DESYNC_THRESHOLD: usize = 16;

        let mut inner = if self.need_reset.load(Ordering::SeqCst) {
            thread::sleep(RESET_FLUSH_TIME);
            let mut inner = self.inner.lock().unwrap();
            self.need_reset.store(false, Ordering::SeqCst);
            inner.frame_queue.clear();
            eprintln!("LFX2: Reset implicit context done");
            inner
        } else {
            self.inner.lock().unwrap()
        };

        let mut context = inner.context.inner.lock().unwrap();
        let (frame, timestamp) = context.prepare_frame(inner.context.clone());
        drop(context);
        inner.frame_queue.push_back(frame.clone());

        if inner.frame_queue.len() > RENDER_DESYNC_THRESHOLD {
            eprintln!("LFX2: Resetting implicit context: too many inflight frames");
            self.need_reset.store(true, Ordering::SeqCst);
        }

        (frame, timestamp)
    }

    fn dequeue(&self, critical: bool) -> Option<Arc<Frame>> {
        if self.need_reset.load(Ordering::SeqCst) {
            return None;
        }
        let mut inner = self.inner.lock().unwrap();
        match inner.frame_queue.pop_front() {
            Some(frame) => Some(frame),
            None => {
                if critical {
                    eprintln!("LFX2: Resetting implicit context: too many inflight frames");
                    self.need_reset.store(true, Ordering::SeqCst);
                }
                None
            }
        }
    }

    fn reset(&self) {
        let _mutex = self.inner.lock().unwrap();
        eprintln!("LFX2: Resetting implicit context: swapchain recreated");
        self.need_reset.store(true, Ordering::SeqCst);
    }
}

#[no_mangle]
pub unsafe extern "C" fn lfx2TimestampNow() -> Timestamp {
    timestamp_now()
}

#[cfg(target_os = "windows")]
#[no_mangle]
pub unsafe extern "C" fn lfx2TimestampFromQpc(qpc: u64) -> Timestamp {
    timestamp_from_qpc(qpc)
}

#[no_mangle]
pub unsafe extern "C" fn lfx2SleepUntil(target: Timestamp) {
    sleep_until(target)
}

#[no_mangle]
pub unsafe extern "C" fn lfx2ContextCreate() -> *mut Context {
    Arc::into_raw(Arc::new(Context::default())) as _
}

#[no_mangle]
pub unsafe extern "C" fn lfx2ContextAddRef(context: *mut Context) {
    Arc::increment_strong_count(context);
}

#[no_mangle]
pub unsafe extern "C" fn lfx2ContextRelease(context: *mut Context) {
    Arc::decrement_strong_count(context);
}

#[no_mangle]
pub unsafe extern "C" fn lfx2FrameCreate(
    context: *mut Context,
    out_timestamp: *mut Timestamp,
) -> *mut Frame {
    let context = Arc::from_raw(context);
    let (frame, timestamp) = context.inner.lock().unwrap().prepare_frame(context.clone());
    *out_timestamp = timestamp;
    let _ = Arc::into_raw(context);
    Arc::into_raw(frame) as _
}

#[no_mangle]
pub unsafe extern "C" fn lfx2FrameAddRef(frame: *mut Frame) {
    Arc::increment_strong_count(frame);
}

#[no_mangle]
pub unsafe extern "C" fn lfx2FrameRelease(frame: *mut Frame) {
    Arc::decrement_strong_count(frame);
}

#[no_mangle]
pub unsafe extern "C" fn lfx2MarkSection(
    frame: *mut Frame,
    section_id: SectionId,
    mark_type: MarkType,
    timestamp: Timestamp,
) {
    (*frame).mark(section_id, mark_type, timestamp);
}

#[no_mangle]
pub unsafe extern "C" fn lfx2ImplicitContextCreate() -> *mut ImplicitContext {
    let context = Box::new(ImplicitContext::default());
    Box::into_raw(context)
}

#[no_mangle]
pub unsafe extern "C" fn lfx2ImplicitContextRelease(context: *mut ImplicitContext) {
    let _ = Box::from_raw(context);
}

#[no_mangle]
pub unsafe extern "C" fn lfx2ImplicitContextReset(context: *mut ImplicitContext) {
    (*context).reset();
}

#[no_mangle]
pub unsafe extern "C" fn lfx2FrameCreateImplicit(
    context: *mut ImplicitContext,
    out_timestamp: *mut Timestamp,
) -> *mut Frame {
    let (frame, timestamp) = (*context).enqueue();
    *out_timestamp = timestamp;
    Arc::into_raw(frame) as _
}

#[no_mangle]
pub unsafe extern "C" fn lfx2FrameDequeueImplicit(
    context: *mut ImplicitContext,
    critical: bool,
) -> Option<NonNull<Frame>> {
    let frame = (*context).dequeue(critical);
    frame.map(|f| NonNull::new(Arc::into_raw(f) as _).unwrap())
}