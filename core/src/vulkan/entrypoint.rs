use crate::time::timestamp_now;
use crate::vulkan::{Device, VulkanContext, VulkanSubmitAux};
use crate::{Frame, MarkType};
use spark::{vk, Builder};
use std::sync::Arc;

#[no_mangle]
pub unsafe extern "C" fn lfx2VulkanContextCreate(
    gipa: vk::FnGetInstanceProcAddr,
    instance: vk::Instance,
    physical_device: vk::PhysicalDevice,
    device: vk::Device,
    queue_family_index: u32,
) -> *mut VulkanContext {
    let loader = spark::Loader {
        fp_create_instance: None,
        fp_get_instance_proc_addr: Some(gipa),
        fp_enumerate_instance_version: None,
        fp_enumerate_instance_layer_properties: None,
        fp_enumerate_instance_extension_properties: None,
    };
    let stub_instance_create_info = vk::InstanceCreateInfo::builder();
    let mut device_extensions = spark::DeviceExtensions::new(vk::Version::from_raw_parts(1, 3, 0));
    device_extensions.enable_ext_calibrated_timestamps();
    let device_extension_names = device_extensions.to_name_vec();
    let device_extension_names = device_extension_names
        .iter()
        .map(|s| s.as_ptr())
        .collect::<Vec<_>>();
    let stub_device_create_info =
        vk::DeviceCreateInfo::builder().pp_enabled_extension_names(&device_extension_names);
    let device = Device::new(
        loader,
        instance,
        &stub_instance_create_info,
        physical_device,
        device,
        &stub_device_create_info,
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
