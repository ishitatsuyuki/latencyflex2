use crate::dx12::{Dx12Context, Dx12SubmitAux};
use crate::time::timestamp_now;
use crate::{Frame, MarkType};
use std::mem::ManuallyDrop;
use std::sync::Arc;
use windows::Win32::Graphics::Direct3D12::{ID3D12CommandQueue, ID3D12Device};

#[no_mangle]
pub unsafe extern "C" fn lfx2Dx12ContextCreate(
    device: ManuallyDrop<ID3D12Device>,
) -> *mut Dx12Context {
    let context = Dx12Context::new(&device);
    Arc::into_raw(context) as _
}

#[no_mangle]
pub unsafe extern "C" fn lfx2Dx12ContextAddRef(context: *mut Dx12Context) {
    Arc::increment_strong_count(context);
}

#[no_mangle]
pub unsafe extern "C" fn lfx2Dx12ContextRelease(context: *mut Dx12Context) {
    Arc::decrement_strong_count(context);
}

#[no_mangle]
pub unsafe extern "C" fn lfx2Dx12ContextBeforeSubmit(
    context: *mut Dx12Context,
    queue: ManuallyDrop<ID3D12CommandQueue>,
) -> Dx12SubmitAux {
    (*context).inner.lock().submit(&queue)
}

#[no_mangle]
pub unsafe extern "C" fn lfx2Dx12ContextBeginFrame(context: *mut Dx12Context, frame: *mut Frame) {
    let frame = Arc::from_raw(frame);
    frame.mark(800, MarkType::Begin, timestamp_now());
    (*context).inner.lock().begin(&frame);
    let _ = Arc::into_raw(frame);
}

#[no_mangle]
pub unsafe extern "C" fn lfx2Dx12ContextEndFrame(context: *mut Dx12Context, frame: *mut Frame) {
    let frame = Arc::from_raw(frame);
    (*context).inner.lock().end(&frame);
    frame.mark(800, MarkType::End, timestamp_now());
    let _ = Arc::into_raw(frame);
}
