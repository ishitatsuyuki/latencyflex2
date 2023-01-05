use crate::time::{sleep_until, timestamp_now};
use crate::{Context, Frame, ImplicitContext, MarkType, SectionId, Timestamp};
use std::ptr::NonNull;
use std::sync::Arc;

#[no_mangle]
pub unsafe extern "C" fn lfx2TimestampNow() -> Timestamp {
    timestamp_now()
}

#[cfg(target_os = "windows")]
#[no_mangle]
pub unsafe extern "C" fn lfx2TimestampFromQpc(qpc: u64) -> Timestamp {
    use crate::timestamp_from_qpc;
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
