use crate::VkResult;
use latencyflex2_core::vulkan::VulkanContext;
use latencyflex2_core::{
    time, FrameAggregator, FrameId, ReflexMappingTracker, TaskAccumulator, Timestamp,
};
use spark::{vk, Builder};
use std::collections::HashMap;
use std::sync::{mpsc, Arc, Mutex, MutexGuard, OnceLock, Weak};
use std::thread;
use std::thread::JoinHandle;

pub struct SleepJob {
    pub deadline: Timestamp,
    pub semaphore: vk::Semaphore,
    pub value: u64,
}

pub struct GlobalState {
    pub instance_table: HashMap<vk::Instance, InstanceState>,
    pub physical_device_table: HashMap<vk::PhysicalDevice, vk::Instance>,
    pub device_table: HashMap<vk::Device, DeviceState>,
    pub queue_table: HashMap<vk::Queue, QueueState>,
}

pub struct InstanceState {
    pub gipa: vk::FnGetInstanceProcAddr,
    pub instance: spark::Instance,
    pub version: vk::Version,
    pub physical_devices: Vec<vk::PhysicalDevice>,
}

pub struct DeviceState {
    pub gdpa: vk::FnGetDeviceProcAddr,
    pub device: spark::Device,
    pub queues: Vec<vk::Queue>,
    pub swapchains: HashMap<vk::SwapchainKHR, SwapchainState>,

    // Unfortunately most of the state here ends up being device-wide instead of per-swapchain,
    // as we need to map present IDs without swapchain information when handling VkLatencySubmissionPresentIdNV.
    pub vulkan_tracker: Arc<VulkanContext<Option<LayerFrame>>>,
    pub aggregator: Arc<Mutex<FrameAggregator>>,
    pub frame_tracker: ReflexMappingTracker<LayerFrame>,
}

pub struct QueueState {
    pub device: vk::Device,
    pub queue_family_index: u32,
    pub queue_index: u32,

    pub stats: TaskAccumulator,
    pub stats_frame: Option<LayerFrame>,

    pub frame: Option<LayerFrame>,
}

impl DeviceState {
    pub fn new(
        gdpa: vk::FnGetDeviceProcAddr,
        instance: spark::Instance,
        phys_device: vk::PhysicalDevice,
        device: spark::Device,
        queues: Vec<(u32, Vec<(u32, vk::Queue)>)>,
    ) -> VkResult<Self> {
        Ok(Self {
            gdpa,
            device,
            queues: queues
                .iter()
                .flat_map(|(_, queues)| queues.into_iter().map(|(_, queue)| *queue))
                .collect(),
            swapchains: HashMap::new(),
            aggregator: Arc::new(Mutex::new(FrameAggregator::new(
                Default::default(),
                queues.len(),
            ))),
            frame_tracker: ReflexMappingTracker::new(),
            vulkan_tracker: VulkanContext::new(
                instance,
                phys_device,
                device,
                queues
                    .iter()
                    .map(|(i, queues)| (*i, queues.into_iter().map(|(j, _)| *j).collect()))
                    .collect(),
            )?,
        })
    }
}

#[derive(Clone)]
pub struct LayerFrame {
    /// Same as inner.id but available without locking
    pub id: FrameId,
    pub inner: Arc<Mutex<LayerFrameInner>>,
}

impl PartialEq for LayerFrame {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl Eq for LayerFrame {}

pub struct LayerFrameInner {
    pub agg: Weak<Mutex<FrameAggregator>>,
    pub id: FrameId,
}

impl Drop for LayerFrameInner {
    fn drop(&mut self) {
        if let Some(agg) = self.agg.upgrade() {
            let mut agg = agg.lock().unwrap();
            agg.finish_frame(self.id);
        }
    }
}

pub struct SwapchainState {
    pub enabled: bool,

    pub sleep_thread: Option<JoinHandle<()>>,
    pub sleep_thread_tx: Option<mpsc::Sender<SleepJob>>,
}

impl SwapchainState {
    pub fn new(device: vk::Device) -> VkResult<Self> {
        let (sleep_thread_tx, sleep_thread_rx) = mpsc::channel::<SleepJob>();
        let sleep_thread = thread::spawn(move || {
            while let Ok(job) = sleep_thread_rx.recv() {
                time::sleep_until(job.deadline);
                {
                    let mut global_state = get_global_state();
                    let device_state = global_state.device_table.get_mut(&device).unwrap();

                    unsafe {
                        let res = device_state.device.signal_semaphore(
                            &vk::SemaphoreSignalInfo::builder()
                                .value(job.value)
                                .semaphore(job.semaphore),
                        );
                        if let Err(err) = res {
                            eprintln!("failed to signal semaphore: {:?}", err);
                        }
                    }
                }
            }
        });
        Ok(Self {
            enabled: false,
            sleep_thread: Some(sleep_thread),
            sleep_thread_tx: Some(sleep_thread_tx),
        })
    }
}

impl Drop for SwapchainState {
    fn drop(&mut self) {
        // Waiting for joining here will result in blocking in vkDestroySwapchainKHR. This might
        // not be desirable but should not matter much.
        let _ = self.sleep_thread_tx.take();
        let _ = self.sleep_thread.take().unwrap().join();
    }
}

static GLOBAL_STATE: OnceLock<Mutex<GlobalState>> = OnceLock::new();

pub fn get_global_state() -> MutexGuard<'static, GlobalState> {
    GLOBAL_STATE
        .get_or_init(|| {
            Mutex::new(GlobalState {
                instance_table: HashMap::new(),
                physical_device_table: HashMap::new(),
                device_table: HashMap::new(),
                queue_table: HashMap::new(),
            })
        })
        .lock()
        .unwrap()
}
