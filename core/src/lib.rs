use std::collections::BTreeMap;
use std::num::NonZeroU64;
use std::sync::{Arc, Mutex};
use std::{cmp, hint, ptr};

#[cfg(target_os = "linux")]
use nix::libc::clock_nanosleep;
#[cfg(target_os = "linux")]
use nix::sys::time::{TimeSpec, TimeValLike};
#[cfg(target_os = "linux")]
use nix::time::{clock_gettime, ClockId};
use once_cell::sync::Lazy;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::CloseHandle;
#[cfg(target_os = "windows")]
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
#[cfg(target_os = "windows")]
use windows::Win32::System::Threading::{
    CreateWaitableTimerExW, SetWaitableTimer, CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
    TIMER_ALL_ACCESS,
};

use crate::ewma::EwmaEstimator;
use crate::profiler::Profiler;

mod ewma;
mod profiler;

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

    profiler: Profiler
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
    predicted_duration: u64,
    marks: BTreeMap<(SectionId, MarkType), Timestamp>,
}

impl ContextInner {
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

    fn prepare_frame(&mut self, context: Arc<Context>) -> (Arc<Frame>, Option<Timestamp>) {
        let predicted_duration = self
            .bandwidth_estimator
            .iter()
            .map(|(_, e)| e.get() as u64)
            .max()
            .unwrap_or(0);
        let bias = 1000000;
        let target = self.last_predicted_frame_end().map(|predicted_frame_end| {
            predicted_frame_end + predicted_duration - self.optimal_latency_estimator.get() as u64 - bias
        });

        let id = self.next_frame_id;
        self.next_frame_id.0 += 1;

        self.frames.insert(
            id,
            FrameImpl {
                writer_count: 1,
                predicted_duration,
                marks: Default::default(),
            },
        );

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
                dbg!(real_latency, optimal_latency, self.optimal_latency_estimator.get());
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
    fn add_ref(&self) {
        let mut inner = self.context.inner.lock().unwrap();
        let frame = inner.frames.get_mut(&self.id).unwrap();
        frame.writer_count += 1;
    }

    fn release(&self) {
        let mut inner = self.context.inner.lock().unwrap();
        let frame = inner.frames.get_mut(&self.id).unwrap();
        frame.writer_count -= 1;
        if frame.writer_count == 0 {
            inner.update_estimates();
        }
    }

    fn mark(&self, section_id: SectionId, mark_type: MarkType, timestamp: Timestamp) {
        let mut inner = self.context.inner.lock().unwrap();
        inner
            .frames
            .get_mut(&self.id)
            .unwrap()
            .mark(section_id, mark_type, timestamp);
        inner.profiler.mark(self.id, section_id, mark_type, timestamp);
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

#[cfg(target_os = "linux")]
fn timestamp_now() -> Timestamp {
    let ts = clock_gettime(ClockId::CLOCK_MONOTONIC_RAW).unwrap();
    ts.num_nanoseconds() as _
}

#[cfg(target_os = "windows")]
fn timestamp_from_qpc(qpc: u64) -> Timestamp {
    static QPF: Lazy<NonZeroU64> = Lazy::new(|| {
        let mut qpf = 0i64;
        unsafe {
            QueryPerformanceFrequency(&mut qpf);
        }
        NonZeroU64::new(qpf as u64).unwrap()
    });

    let denom = 1_000_000_000;
    let whole = qpc / QPF.get() * denom;
    let part = qpc % QPF.get() * denom / QPF.get();
    (whole + part) as _
}

#[cfg(target_os = "windows")]
fn timestamp_now() -> Timestamp {
    let mut qpc = 0i64;
    unsafe {
        QueryPerformanceCounter(&mut qpc);
    }
    timestamp_from_qpc(qpc as u64)
}

#[cfg(target_os = "linux")]
fn sleep_until(target: Timestamp) {
    let ts = TimeSpec::nanoseconds(target as i64);
    unsafe {
        clock_nanosleep(
            ClockId::CLOCK_MONOTONIC_RAW.into(),
            nix::libc::TIMER_ABSTIME,
            ts.as_ref(),
            ptr::null_mut(),
        );
    }
}

#[cfg(target_os = "windows")]
fn sleep_until(target: Timestamp) {
    const MIN_SPIN_PERIOD: u64 = 500_000;
    let mut now = timestamp_now();

    let timer = unsafe {
        CreateWaitableTimerExW(
            None,
            None,
            CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
            TIMER_ALL_ACCESS.0,
        )
    }
    .unwrap();

    dbg!(now, target);

    while now + MIN_SPIN_PERIOD < target {
        let sleep_duration = -((target - now - MIN_SPIN_PERIOD) as i64) / 100;
        unsafe {
            SetWaitableTimer(timer, &sleep_duration, 0, None, None, false);
        }
        now = timestamp_now();
    }

    while now < target {
        hint::spin_loop();
        now = timestamp_now();
    }

    unsafe { CloseHandle(timer) }.unwrap();
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
pub unsafe extern "C" fn lfx2ContextCreate() -> *const Context {
    Arc::into_raw(Arc::new(Context::default()))
}

#[no_mangle]
pub unsafe extern "C" fn lfx2ContextAddRef(context: *const Context) {
    Arc::increment_strong_count(context);
}

#[no_mangle]
pub unsafe extern "C" fn lfx2ContextRelease(context: *const Context) {
    Arc::decrement_strong_count(context);
}

#[no_mangle]
pub unsafe extern "C" fn lfx2FrameCreate(
    context: *const Context,
    out_timestamp: *mut Timestamp,
) -> *const Frame {
    let context = Arc::from_raw(context);
    let (frame, timestamp) = context.inner.lock().unwrap().prepare_frame(context.clone());
    *out_timestamp = timestamp.unwrap_or(timestamp_now());
    let _ = Arc::into_raw(context);
    Arc::into_raw(frame)
}

#[no_mangle]
pub unsafe extern "C" fn lfx2FrameAddRef(frame: *const Frame) {
    (*frame).add_ref();
    Arc::increment_strong_count(frame);
}

#[no_mangle]
pub unsafe extern "C" fn lfx2FrameRelease(frame: *const Frame) {
    (*frame).release();
    Arc::decrement_strong_count(frame);
}

#[no_mangle]
pub unsafe extern "C" fn lfx2MarkSection(
    frame: *const Frame,
    section_id: SectionId,
    mark_type: MarkType,
    timestamp: Timestamp,
) {
    (*frame).mark(section_id, mark_type, timestamp);
}
