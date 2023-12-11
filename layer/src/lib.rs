extern crate core;

use core::slice;
use std::collections::HashSet;
use std::ffi::CStr;
use std::mem::MaybeUninit;
use std::ops::DerefMut;
use std::sync::{Arc, Mutex};
use std::{iter, mem};

use bumpalo::collections::CollectIn;
use bumpalo::collections::Vec as BVec;
use bumpalo::Bump;
use cstr::cstr;
use spark::{vk, Builder};

use latencyflex2_core::vulkan::FenceWorkerResult;
use latencyflex2_core::{ReflexId, StageId};
use state::{
    DeviceState, InstanceState, LayerFrame, LayerFrameInner, QueueState, SleepJob, SwapchainState,
};

use crate::spark_ext::SparkResultExt;

mod spark_ext;
mod state;
mod thread_pool;
mod vk_layer;

type VkResult<T> = Result<T, vk::Result>;
fn convert_vk_result(f: impl FnOnce() -> VkResult<()>) -> vk::Result {
    match f() {
        Ok(()) => vk::Result::SUCCESS,
        Err(err) => err,
    }
}

fn convert_vk_result_multi_success(f: impl FnOnce() -> VkResult<vk::Result>) -> vk::Result {
    match f() {
        Ok(result) => result,
        Err(err) => err,
    }
}

unsafe fn get_instance_override(name: &str) -> Option<vk::FnVoidFunction> {
    match name {
        "vkCreateInstance" => Some(mem::transmute(create_instance as vk::FnCreateInstance)),
        "vkDestroyInstance" => Some(mem::transmute(destroy_instance as vk::FnDestroyInstance)),
        "vk_layerGetPhysicalDeviceProcAddr" => Some(mem::transmute(
            gpdpa as vk_layer::FnGetPhysicalDeviceProcAddr,
        )),
        "vkGetInstanceProcAddr" => Some(mem::transmute(gipa as vk::FnGetInstanceProcAddr)),
        _ => None,
    }
}

unsafe fn get_physical_device_override(name: &str) -> Option<vk::FnVoidFunction> {
    match name {
        "vkCreateDevice" => Some(mem::transmute(create_device as vk::FnCreateDevice)),
        "vkGetPhysicalDeviceSurfaceCapabilities2KHR" => Some(mem::transmute(
            get_physical_device_surface_capabilities2_khr
                as vk::FnGetPhysicalDeviceSurfaceCapabilities2KHR,
        )),
        _ => None,
    }
}

unsafe fn get_device_override(name: &str) -> Option<vk::FnVoidFunction> {
    match name {
        "vkGetDeviceProcAddr" => Some(mem::transmute(gdpa as vk::FnGetDeviceProcAddr)),
        "vkDestroyDevice" => Some(mem::transmute(destroy_device as vk::FnDestroyDevice)),
        "vkCreateSwapchainKHR" => Some(mem::transmute(
            create_swapchain_khr as vk::FnCreateSwapchainKHR,
        )),
        "vkDestroySwapchainKHR" => Some(mem::transmute(
            destroy_swapchain_khr as vk::FnDestroySwapchainKHR,
        )),
        "vkQueueSubmit" => Some(mem::transmute(queue_submit as vk::FnQueueSubmit)),
        "vkQueueSubmit2" | "vkQueueSubmit2KHR" => {
            Some(mem::transmute(queue_submit2 as vk::FnQueueSubmit2))
        }
        "vkQueuePresentKHR" => Some(mem::transmute(queue_present_khr as vk::FnQueuePresentKHR)),
        "vkGetLatencyTimingsNV" => Some(mem::transmute(
            get_latency_timings_nv as vk::FnGetLatencyTimingsNV,
        )),
        "vkLatencySleepNV" => Some(mem::transmute(latency_sleep_nv as vk::FnLatencySleepNV)),
        "vkQueueNotifyOutOfBandNV" => Some(mem::transmute(
            queue_notify_out_of_band_nv as vk::FnQueueNotifyOutOfBandNV,
        )),
        "vkSetLatencyMarkerNV" => Some(mem::transmute(
            set_latency_marker_nv as vk::FnSetLatencyMarkerNV,
        )),
        "vkSetLatencySleepModeNV" => Some(mem::transmute(
            set_latency_sleep_mode_nv as vk::FnSetLatencySleepModeNV,
        )),
        _ => None,
    }
}

unsafe extern "system" fn gipa(
    instance: Option<vk::Instance>,
    p_name: *const std::os::raw::c_char,
) -> Option<vk::FnVoidFunction> {
    let name = CStr::from_ptr(p_name);
    let name = name.to_str().unwrap();
    if let Some(f) = get_instance_override(name)
        .or_else(|| get_physical_device_override(name))
        .or_else(|| get_device_override(name))
    {
        return Some(f);
    }

    let global_state = state::get_global_state();
    let instance_state = global_state.instance_table.get(&instance.unwrap()).unwrap();
    (instance_state.gipa)(instance, p_name)
}

unsafe extern "system" fn gpdpa(
    _instance: Option<vk::Instance>,
    p_name: *const std::os::raw::c_char,
) -> Option<vk::FnVoidFunction> {
    let name = CStr::from_ptr(p_name);
    let name = name.to_str().unwrap();
    if let Some(f) = get_physical_device_override(name) {
        return Some(f);
    }

    None
}

unsafe extern "system" fn gdpa(
    device: Option<vk::Device>,
    p_name: *const std::os::raw::c_char,
) -> Option<vk::FnVoidFunction> {
    let name = CStr::from_ptr(p_name);
    let name = name.to_str().unwrap();
    if let Some(f) = get_device_override(name) {
        return Some(f);
    }

    let device = device.unwrap();
    let global_state = state::get_global_state();
    let device_state = global_state.device_table.get(&device).unwrap();
    (device_state.gdpa)(Some(device), p_name)
}

unsafe extern "system" fn create_instance(
    p_create_info: *const vk::InstanceCreateInfo,
    p_allocator: *const vk::AllocationCallbacks,
    p_instance: *mut vk::Instance,
) -> vk::Result {
    convert_vk_result(move || {
        let p_next = ((*p_create_info).p_next as *mut vk::BaseOutStructure).as_mut();
        let mut p_next_chain = iter::successors(p_next, |p_next| p_next.p_next.as_mut());
        let layer_create_info = p_next_chain
            .find_map(|out_struct| {
                let out_struct = out_struct as *mut vk::BaseOutStructure;
                let layer_create_info = match (*out_struct).s_type {
                    vk::StructureType::LOADER_INSTANCE_CREATE_INFO => unsafe {
                        &mut *(out_struct as *mut vk_layer::VkLayerInstanceCreateInfo)
                    },
                    _ => {
                        return None;
                    }
                };
                if layer_create_info.function == vk_layer::VkLayerFunction::VK_LAYER_LINK_INFO {
                    Some(layer_create_info)
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                eprintln!("failed to find the VkLayerInstanceCreateInfo struct in the chain.");
                vk::Result::ERROR_INITIALIZATION_FAILED
            })?;
        let get_instance_proc_addr =
            (*layer_create_info.u.p_layer_info).pfn_next_get_instance_proc_addr;
        let mut static_fn = spark::Loader {
            fp_create_instance: None,
            fp_get_instance_proc_addr: Some(get_instance_proc_addr),
            fp_enumerate_instance_version: None,
            fp_enumerate_instance_layer_properties: None,
            fp_enumerate_instance_extension_properties: None,
        };
        static_fn.fp_create_instance =
            mem::transmute(static_fn.get_instance_proc_addr(None, cstr!("vkCreateInstance")));

        layer_create_info.u.p_layer_info =
            layer_create_info.u.p_layer_info.as_ref().unwrap().p_next;

        let instance =
            (static_fn.fp_create_instance.unwrap())(p_create_info, p_allocator, p_instance)
                .assume_init_on_success(*(p_instance as *mut MaybeUninit<vk::Instance>))?;

        let instance =
            spark::Instance::load(&static_fn, instance, &*p_create_info).map_err(|err| {
                eprintln!("failed to load instance: {:?}", err);
                vk::Result::ERROR_INITIALIZATION_FAILED
            })?;

        let physical_devices = instance
            .enumerate_physical_devices_to_vec()
            .map_err(|err| {
                eprintln!("failed to enumerate physical devices: {:?}", err);
                err
            })?;

        {
            let mut global_state = state::get_global_state();
            for physical_device in &physical_devices {
                global_state
                    .physical_device_table
                    .insert(*physical_device, instance.handle);
            }
            global_state.instance_table.insert(
                instance.handle,
                InstanceState {
                    instance,
                    version: (*p_create_info)
                        .p_application_info
                        .as_ref()
                        .map(|i| i.api_version)
                        .unwrap_or(vk::Version::from_raw_parts(1, 0, 0)),
                    gipa: get_instance_proc_addr,
                    physical_devices,
                },
            );
        }
        Ok(())
    })
}

unsafe extern "system" fn destroy_instance(
    instance: Option<vk::Instance>,
    p_allocator: *const vk::AllocationCallbacks,
) {
    if let Some(instance) = instance {
        let mut global_state = state::get_global_state();
        let instance_state = global_state.instance_table.remove(&instance).unwrap();
        instance_state
            .instance
            .destroy_instance(p_allocator.as_ref());
        for physical_device in instance_state.physical_devices {
            global_state.physical_device_table.remove(&physical_device);
        }
    }
}

unsafe extern "system" fn create_device(
    physical_device: Option<vk::PhysicalDevice>,
    p_create_info: *const vk::DeviceCreateInfo,
    p_allocator: *const vk::AllocationCallbacks,
    p_device: *mut vk::Device,
) -> vk::Result {
    convert_vk_result(move || {
        let physical_device = physical_device.unwrap();
        let p_next = ((*p_create_info).p_next as *mut vk::BaseOutStructure).as_mut();
        let mut p_next_chain = iter::successors(p_next, |p_next| p_next.p_next.as_mut());
        let layer_create_info = p_next_chain
            .find_map(|out_struct| {
                let out_struct = out_struct as *mut vk::BaseOutStructure;
                let layer_create_info = match (*out_struct).s_type {
                    vk::StructureType::LOADER_DEVICE_CREATE_INFO => unsafe {
                        &mut *(out_struct as *mut vk_layer::VkLayerDeviceCreateInfo)
                    },
                    _ => {
                        return None;
                    }
                };
                if layer_create_info.function == vk_layer::VkLayerFunction::VK_LAYER_LINK_INFO {
                    Some(layer_create_info)
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                eprintln!("failed to find the VkLayerDeviceCreateInfo struct in the chain.");
                vk::Result::ERROR_INITIALIZATION_FAILED
            })?;
        let device_gdpa = (*layer_create_info.u.p_layer_info).pfn_next_get_device_proc_addr;

        layer_create_info.u.p_layer_info = (*layer_create_info.u.p_layer_info).p_next;

        let mut global_state = state::get_global_state();
        let global_state = global_state.deref_mut();
        let instance = global_state
            .physical_device_table
            .get(&physical_device)
            .unwrap();
        let instance = global_state.instance_table.get_mut(&instance).unwrap();

        let patch_alloc = Bump::new();
        let extensions = slice::from_raw_parts(
            (*p_create_info).pp_enabled_extension_names as *const *const std::os::raw::c_char,
            (*p_create_info).enabled_extension_count as usize,
        );
        let extension_set = extensions
            .iter()
            .map(|x| CStr::from_ptr(*x))
            .collect::<HashSet<_>>();
        let add_ext = |name: &'static CStr| {
            iter::once(name.as_ptr()).filter(|_| !extension_set.contains(name))
        };
        let add_calibrated_timestamps = add_ext(cstr!("VK_EXT_calibrated_timestamps"));
        let add_host_reset = add_ext(cstr!("VK_EXT_host_query_reset"))
            .filter(|_| instance.version < vk::Version::from_raw_parts(1, 2, 0));
        let new_extensions = extensions
            .iter()
            .copied()
            .chain(add_calibrated_timestamps)
            .chain(add_host_reset)
            .collect_in::<BVec<_>>(&patch_alloc)
            .into_bump_slice();

        let new_create_info = vk::DeviceCreateInfo {
            enabled_extension_count: new_extensions.len() as u32,
            pp_enabled_extension_names: new_extensions.as_ptr(),
            ..(*p_create_info)
        };

        let device = instance.instance.fp_create_device.unwrap()(
            Some(physical_device),
            &new_create_info,
            p_allocator,
            p_device,
        )
        .assume_init_on_success(*(p_device as *mut MaybeUninit<vk::Device>))?;

        // HACK: Temporarily override the instance's GDPA to use the device's GDPA. Layers are not
        //       allowed to use the trampoline functions as they violate the layering model.
        let old_gdpa = mem::replace(
            &mut instance.instance.fp_get_device_proc_addr,
            Some(device_gdpa),
        );
        let device = spark::Device::load(
            &instance.instance,
            device,
            &new_create_info,
            instance.version,
        );
        instance.instance.fp_get_device_proc_addr = old_gdpa;
        let device = device.map_err(|err| {
            eprintln!("failed to load device: {:?}", err);
            vk::Result::ERROR_INITIALIZATION_FAILED
        })?;

        let mut queues = vec![];
        for i in 0..(*p_create_info).queue_create_info_count as usize {
            let queue_create_info = &*((*p_create_info).p_queue_create_infos.add(i));
            let queue_family_index = queue_create_info.queue_family_index;
            let mut queues_in_family = vec![];
            for queue_index in 0..queue_create_info.queue_count {
                let queue = device.get_device_queue(queue_family_index, queue_index);
                global_state.queue_table.insert(
                    queue,
                    QueueState {
                        device: device.handle,
                        queue_family_index,
                        queue_index,
                        stats: Default::default(),
                        frame: None,
                    },
                );
                queues_in_family.push((queue_index, queue));
            }
            queues.push((queue_family_index, queues_in_family));
        }

        let state = DeviceState::new(
            device_gdpa,
            instance.instance.clone(),
            physical_device,
            device,
            queues,
        );
        let state = match state {
            Ok(state) => state,
            Err(err) => {
                eprintln!("failed to create device state: {:?}", err);
                device.destroy_device(p_allocator.as_ref());
                return Err(err);
            }
        };
        global_state.device_table.insert(device.handle, state);

        Ok(())
    })
}

unsafe extern "system" fn destroy_device(
    device: Option<vk::Device>,
    p_allocator: *const vk::AllocationCallbacks,
) {
    if let Some(device) = device {
        let mut global_state = state::get_global_state();
        let device_state = global_state.device_table.remove(&device).unwrap();
        device_state.device.destroy_device(p_allocator.as_ref());
        for queue in device_state.queues {
            global_state.queue_table.remove(&queue);
        }
    }
}

unsafe extern "system" fn create_swapchain_khr(
    device: Option<vk::Device>,
    p_create_info: *const vk::SwapchainCreateInfoKHR,
    p_allocator: *const vk::AllocationCallbacks,
    p_swapchain: *mut vk::SwapchainKHR,
) -> vk::Result {
    convert_vk_result(move || {
        let device = device.unwrap();
        let mut global_state = state::get_global_state();
        let global_state = global_state.deref_mut();
        let device_state = global_state.device_table.get_mut(&device).unwrap();

        let p_next = ((*p_create_info).p_next as *const vk::BaseInStructure).as_ref();
        let mut p_next_chain = iter::successors(p_next, |p_next| p_next.p_next.as_ref());
        let latency_create_info = p_next_chain.find_map(|in_struct| {
            let in_struct = in_struct as *const vk::BaseInStructure;
            match (*in_struct).s_type {
                vk::StructureType::SWAPCHAIN_LATENCY_CREATE_INFO_NV => unsafe {
                    Some(&mut *(in_struct as *mut vk::SwapchainLatencyCreateInfoNV))
                },
                _ => None,
            }
        });

        let swapchain = device_state
            .device
            .create_swapchain_khr(&*p_create_info, p_allocator.as_ref())?;

        if latency_create_info.is_some_and(|i| i.latency_mode_enable != vk::FALSE) {
            let state = SwapchainState::new(device);
            let state = match state {
                Ok(state) => state,
                Err(err) => {
                    eprintln!("failed to create swapchain state: {:?}", err);
                    device_state
                        .device
                        .destroy_swapchain_khr(Some(swapchain), p_allocator.as_ref());
                    return Err(err);
                }
            };
            device_state.swapchains.insert(swapchain, state);
        }

        p_swapchain.write(swapchain);
        Ok(())
    })
}

unsafe extern "system" fn destroy_swapchain_khr(
    device: Option<vk::Device>,
    swapchain: Option<vk::SwapchainKHR>,
    p_allocator: *const vk::AllocationCallbacks,
) {
    if let Some(device) = device {
        let mut global_state = state::get_global_state();
        let device_state = global_state.device_table.get_mut(&device).unwrap();
        device_state
            .device
            .destroy_swapchain_khr(swapchain, p_allocator.as_ref());
        if let Some(swapchain) = swapchain {
            device_state.swapchains.remove(&swapchain);
        }
    }
}

unsafe extern "system" fn queue_submit(
    queue: Option<vk::Queue>,
    submit_count: u32,
    p_submits: *const vk::SubmitInfo,
    fence: Option<vk::Fence>,
) -> vk::Result {
    convert_vk_result(move || {
        let queue = queue.unwrap();
        let device = {
            let mut global_state = state::get_global_state();
            let global_state = global_state.deref_mut();
            let queue_state = global_state.queue_table.get(&queue).unwrap();
            let device_state = global_state
                .device_table
                .get_mut(&queue_state.device)
                .unwrap();
            device_state.device.clone()
        };

        let submits = slice::from_raw_parts(p_submits, submit_count as usize);
        eprintln!("vkQueueSubmit is not currently supported for instrumentation yet! Consider using vkQueueSubmit2.");
        device.queue_submit(queue, &submits, fence)
    })
}

unsafe extern "system" fn queue_submit2(
    queue: Option<vk::Queue>,
    submit_count: u32,
    p_submits: *const vk::SubmitInfo2,
    fence: Option<vk::Fence>,
) -> vk::Result {
    convert_vk_result(move || {
        let queue = queue.unwrap();
        let device;
        let patch_alloc = Bump::new();
        let new_submits;
        {
            let mut global_state = state::get_global_state();
            let global_state = global_state.deref_mut();
            let queue_state = global_state.queue_table.get_mut(&queue).unwrap();
            let device_state = global_state
                .device_table
                .get_mut(&queue_state.device)
                .unwrap();

            let submits = slice::from_raw_parts(p_submits, submit_count as usize);
            new_submits = submits
                .iter()
                .map(|submit| {
                    let p_next = (submit.p_next as *const vk::BaseInStructure).as_ref();
                    let mut p_next_chain =
                        iter::successors(p_next, |p_next| p_next.p_next.as_ref());
                    let latency_present_id = p_next_chain.find_map(|in_struct| {
                        let in_struct = in_struct as *const vk::BaseInStructure;
                        match (*in_struct).s_type {
                            vk::StructureType::LATENCY_SUBMISSION_PRESENT_ID_NV => unsafe {
                                Some(&mut *(in_struct as *mut vk::LatencySubmissionPresentIdNV))
                            },
                            _ => None,
                        }
                    });

                    let reflex_id = latency_present_id.map(|i| ReflexId(i.present_id));

                    let new_submit = if let Some(reflex_id) = reflex_id {
                        if queue_state.frame != Some(reflex_id) {
                            if let Some(queue_frame_id) = queue_state.frame {
                                assert!(queue_frame_id < reflex_id);
                            }

                            if let Some(current_id) = queue_state.frame {
                                let current_frame = device_state.frame_tracker.get(current_id);
                                device_state.vulkan_tracker.notify(
                                    queue_state.queue_family_index,
                                    queue_state.queue_index,
                                    current_frame,
                                );
                            }

                            queue_state.frame = Some(reflex_id);
                        }
                        let data = device_state
                            .vulkan_tracker
                            .submit(queue_state.queue_family_index, queue_state.queue_index)?;
                        let cmd_bufs = slice::from_raw_parts(
                            submit.p_command_buffer_infos,
                            submit.command_buffer_info_count as usize,
                        );
                        let new_cmd_bufs = iter::once(
                            *vk::CommandBufferSubmitInfo::builder()
                                .command_buffer(data.submit_before),
                        )
                        .chain(
                            cmd_bufs.iter().copied().chain(iter::once(
                                *vk::CommandBufferSubmitInfo::builder()
                                    .command_buffer(data.submit_after),
                            )),
                        )
                        .collect_in::<BVec<_>>(&patch_alloc)
                        .into_bump_slice();
                        let signal_sem_info = slice::from_raw_parts(
                            submit.p_signal_semaphore_infos,
                            submit.signal_semaphore_info_count as usize,
                        );
                        let new_signal_sem_info = signal_sem_info
                            .iter()
                            .copied()
                            .chain(iter::once(
                                *vk::SemaphoreSubmitInfo::builder()
                                    .semaphore(data.signal_sem)
                                    .value(data.signal_sem_value),
                            ))
                            .collect_in::<BVec<_>>(&patch_alloc)
                            .into_bump_slice();
                        vk::SubmitInfo2 {
                            command_buffer_info_count: new_cmd_bufs.len() as u32,
                            p_command_buffer_infos: new_cmd_bufs.as_ptr(),
                            signal_semaphore_info_count: new_signal_sem_info.len() as u32,
                            p_signal_semaphore_infos: new_signal_sem_info.as_ptr(),
                            ..*submit
                        }
                    } else {
                        *submit
                    };
                    Ok(new_submit)
                })
                .collect_in::<VkResult<BVec<_>>>(&patch_alloc)?;
            // TODO: Fix potential leak on error
            device = device_state.device.clone();
        }

        let result = device.queue_submit2(queue, &new_submits, fence);
        result
    })
}

unsafe extern "system" fn queue_present_khr(
    queue: Option<vk::Queue>,
    p_present_info: *const vk::PresentInfoKHR,
) -> vk::Result {
    convert_vk_result_multi_success(move || {
        let queue = queue.unwrap();
        let device = {
            let mut global_state = state::get_global_state();
            let global_state = global_state.deref_mut();
            let queue_state = global_state.queue_table.get(&queue).unwrap();
            let device_state = global_state
                .device_table
                .get_mut(&queue_state.device)
                .unwrap();
            device_state.device.clone()
        };

        let p_next = ((*p_present_info).p_next as *const vk::BaseInStructure).as_ref();
        let mut p_next_chain = iter::successors(p_next, |p_next| p_next.p_next.as_ref());
        let present_id = p_next_chain.find_map(|in_struct| {
            let in_struct = in_struct as *const vk::BaseInStructure;
            match (*in_struct).s_type {
                vk::StructureType::PRESENT_ID_KHR => unsafe {
                    Some(&mut *(in_struct as *mut vk::PresentIdKHR))
                },
                _ => None,
            }
        });

        let ret = device.queue_present_khr(queue, &*p_present_info)?;

        if let Some(present_id) = present_id {
            let mut global_state = state::get_global_state();
            let global_state = global_state.deref_mut();
            let queue_state = global_state.queue_table.get(&queue).unwrap();
            let device_state = global_state
                .device_table
                .get_mut(&queue_state.device)
                .unwrap();

            let present_ids = slice::from_raw_parts(
                present_id.p_present_ids,
                present_id.swapchain_count as usize,
            );
            for present_id in present_ids {
                let present_id = ReflexId(*present_id);
                for queue in &device_state.queues {
                    let queue_state = global_state.queue_table.get_mut(&queue).unwrap();
                    if queue_state.frame == Some(present_id) {
                        queue_state.frame = None;
                        device_state.vulkan_tracker.notify(
                            queue_state.queue_family_index,
                            queue_state.queue_index,
                            device_state.frame_tracker.get(present_id),
                        );
                    }
                }
                device_state.frame_tracker.present(present_id);
            }
        }

        Ok(ret)
    })
}

unsafe extern "system" fn get_physical_device_surface_capabilities2_khr(
    physical_device: Option<vk::PhysicalDevice>,
    p_surface_info: *const vk::PhysicalDeviceSurfaceInfo2KHR,
    p_surface_capabilities: *mut vk::SurfaceCapabilities2KHR,
) -> vk::Result {
    convert_vk_result(move || {
        let p_next = ((*p_surface_capabilities).p_next as *mut vk::BaseOutStructure).as_mut();
        let mut p_next_chain = iter::successors(p_next, |p_next| p_next.p_next.as_mut());

        let physical_device = physical_device.unwrap();
        let mut global_state = state::get_global_state();
        let global_state = global_state.deref_mut();
        let instance = global_state
            .physical_device_table
            .get(&physical_device)
            .unwrap();
        let instance = global_state.instance_table.get_mut(&instance).unwrap();
        let caps = instance
            .instance
            .get_physical_device_surface_capabilities2_khr(
                physical_device,
                p_surface_info.as_ref().unwrap(),
                p_surface_capabilities.as_mut().unwrap(),
            );

        let latency_caps = p_next_chain.find_map(|out_struct| {
            let out_struct = out_struct as *mut vk::BaseOutStructure;
            match (*out_struct).s_type {
                vk::StructureType::LATENCY_SURFACE_CAPABILITIES_NV => unsafe {
                    Some(&mut *(out_struct as *mut vk::LatencySurfaceCapabilitiesNV))
                },
                _ => {
                    return None;
                }
            }
        });

        if let Some(latency_caps) = latency_caps {
            latency_caps.present_mode_count = 0;
        }

        caps
    })
}

unsafe extern "system" fn set_latency_sleep_mode_nv(
    device: Option<vk::Device>,
    swapchain: Option<vk::SwapchainKHR>,
    p_sleep_mode_info: *const vk::LatencySleepModeInfoNV,
) -> vk::Result {
    let device = device.unwrap();
    let swapchain = swapchain.unwrap();
    let mut global_state = state::get_global_state();
    let global_state = global_state.deref_mut();
    let device = global_state.device_table.get_mut(&device).unwrap();
    let swapchain_state = device.swapchains.get_mut(&swapchain).unwrap();

    swapchain_state.enabled = (*p_sleep_mode_info).low_latency_mode != vk::FALSE;

    vk::Result::SUCCESS
}

unsafe extern "system" fn latency_sleep_nv(
    device: Option<vk::Device>,
    swapchain: Option<vk::SwapchainKHR>,
    p_sleep_info: *const vk::LatencySleepInfoNV,
) -> vk::Result {
    let device = device.unwrap();
    let swapchain = swapchain.unwrap();
    let mut global_state = state::get_global_state();
    let global_state = global_state.deref_mut();
    let device_state = global_state.device_table.get_mut(&device).unwrap();
    let swapchain_state = device_state.swapchains.get_mut(&swapchain).unwrap();
    let info = p_sleep_info.as_ref().unwrap();

    for (i, queue) in device_state.queues.iter().enumerate() {
        let queue_state = global_state.queue_table.get_mut(&queue).unwrap();
        while let Some(res) = device_state
            .vulkan_tracker
            .get_result(queue_state.queue_family_index, queue_state.queue_index)
        {
            match res {
                FenceWorkerResult::Submission(stats) => {
                    queue_state.stats.accumulate(&stats);
                }
                FenceWorkerResult::Notification(frame) => {
                    if let Some(frame) = frame {
                        device_state.aggregator.lock().unwrap().mark(
                            frame.id,
                            StageId(i),
                            queue_state.stats.stats(),
                        );
                    }
                    queue_state.stats.reset();
                }
            }
        }
    }

    let (frame_id, deadline) = device_state.aggregator.lock().unwrap().new_frame();
    let vk_frame = LayerFrame {
        id: frame_id,
        inner: Arc::new(Mutex::new(LayerFrameInner {
            agg: Arc::downgrade(&device_state.aggregator),
            id: frame_id,
        })),
    };
    swapchain_state
        .sleep_thread_tx
        .as_ref()
        .unwrap()
        .send(SleepJob {
            deadline,
            semaphore: info.signal_semaphore.unwrap(),
            value: info.value,
        })
        .unwrap();
    device_state.frame_tracker.recalibrate();
    device_state.frame_tracker.add_frame(vk_frame);

    vk::Result::SUCCESS
}
unsafe extern "system" fn set_latency_marker_nv(
    device: Option<vk::Device>,
    _swapchain: Option<vk::SwapchainKHR>,
    p_latency_marker_info: *const vk::SetLatencyMarkerInfoNV,
) {
    let device = device.unwrap();
    let mut global_state = state::get_global_state();
    let global_state = global_state.deref_mut();
    let device_state = global_state.device_table.get_mut(&device).unwrap();
    let info = p_latency_marker_info.as_ref().unwrap();
    match info.marker {
        vk::LatencyMarkerNV::SIMULATION_START => {
            device_state
                .frame_tracker
                .mark_simulation_begin(ReflexId(info.present_id));
        }
        vk::LatencyMarkerNV::RENDERSUBMIT_START => {
            // Recording the render submit event is not currently useful as the upper layer already
            // puts this information in LatencySubmissionPresentIdNV.
            // device_state
            //     .frame_tracker
            //     .mark_render_begin(ReflexId(info.present_id));
        }
        _ => {}
    }
}

unsafe extern "system" fn get_latency_timings_nv(
    _device: Option<vk::Device>,
    _swapchain: Option<vk::SwapchainKHR>,
    p_timing_count: *mut u32,
    _p_latency_marker_info: *mut vk::GetLatencyMarkerInfoNV,
) {
    // Stub.
    p_timing_count.write(0);
}
unsafe extern "system" fn queue_notify_out_of_band_nv(
    _queue: Option<vk::Queue>,
    _p_queue_type_info: *const vk::OutOfBandQueueTypeInfoNV,
) {
    // Stub.
}

#[no_mangle]
pub unsafe extern "system" fn vkNegotiateLoaderLayerInterfaceVersion(
    p_version_struct: *mut vk_layer::VkNegotiateLayerInterface,
) -> vk::Result {
    let version_struct = p_version_struct.as_mut().unwrap();
    if version_struct.loader_layer_interface_version < 2 {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    }
    version_struct.loader_layer_interface_version = 2;
    version_struct.pfn_get_instance_proc_addr = gipa;
    version_struct.pfn_get_device_proc_addr = gdpa;
    version_struct.pfn_get_physical_device_proc_addr = gpdpa;
    vk::Result::SUCCESS
}
