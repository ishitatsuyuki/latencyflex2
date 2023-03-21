use crate::time::timestamp_now;
use crate::vulkan::{Device, VulkanContext, VulkanSubmitAux};
use crate::{Frame, MarkType};
use ash::vk;
use std::sync::Arc;

#[no_mangle]
pub unsafe extern "C" fn lfx2VulkanContextCreate(
    gipa: vk::PFN_vkGetInstanceProcAddr,
    instance: vk::Instance,
    physical_device: vk::PhysicalDevice,
    device: vk::Device,
    queue_family_index: u32,
) -> *mut VulkanContext {
    let device = Device::new(
        vk::StaticFn {
            get_instance_proc_addr: gipa,
        },
        instance,
        physical_device,
        device,
        queue_family_index,
    );
    let context = VulkanContext::new(device).unwrap();
    Arc::into_raw(context) as _
}

#[no_mangle]
pub unsafe extern "C" fn lfx2VulkanContextAddRef(context: *mut VulkanContext) {
    Arc::increment_strong_count(context);
}

#[no_mangle]
pub unsafe extern "C" fn lfx2VulkanContextRelease(context: *mut VulkanContext) {
    Arc::decrement_strong_count(context);
}

#[no_mangle]
pub unsafe extern "C" fn lfx2VulkanContextBeforeSubmit(
    context: *mut VulkanContext,
) -> VulkanSubmitAux {
    (*context).inner.lock().submit().unwrap()
}

#[no_mangle]
pub unsafe extern "C" fn lfx2VulkanContextBeginFrame(
    context: *mut VulkanContext,
    frame: *mut Frame,
) {
    let frame = Arc::from_raw(frame);
    frame.mark(800, MarkType::Begin, timestamp_now());
    (*context).inner.lock().begin(&frame);
    let _ = Arc::into_raw(frame);
}

#[no_mangle]
pub unsafe extern "C" fn lfx2VulkanContextEndFrame(context: *mut VulkanContext, frame: *mut Frame) {
    let frame = Arc::from_raw(frame);
    (*context).inner.lock().end(&frame);
    frame.mark(800, MarkType::End, timestamp_now());
    let _ = Arc::into_raw(frame);
}
