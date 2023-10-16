use std::mem;
use std::sync::Arc;

use parking_lot::Mutex;
use spark::vk::{CommandBufferUsageFlags, QueryResultFlags};
use spark::{vk, Builder};

use crate::fence_worker::{FenceThread, FenceWorkerMessage};
use crate::time::{timestamp_from_vulkan, timestamp_now, VULKAN_TIMESTAMP_DOMAIN};
use crate::{Frame, Timestamp};

mod entrypoint;

type VkResult<T> = Result<T, vk::Result>;

struct Device {
    handle: spark::Device,
    limits: vk::PhysicalDeviceLimits,
    queue_family_index: u32,
    queue_family_properties: vk::QueueFamilyProperties,
}

impl Device {
    fn new(
        instance: spark::Instance,
        phys_device: vk::PhysicalDevice,
        device: spark::Device,
        queue_family_index: u32,
    ) -> Arc<Device> {
        unsafe {
            let limits = instance.get_physical_device_properties(phys_device).limits;
            let queue_family_properties = instance
                .get_physical_device_queue_family_properties_to_vec(phys_device)
                [queue_family_index as usize];
            Arc::new(Device {
                handle: device,
                limits,
                queue_family_index,
                queue_family_properties,
            })
        }
    }
}

struct QueryPool {
    device: Arc<Device>,
    handle: vk::QueryPool,
}

impl QueryPool {
    fn new(device: Arc<Device>, query_type: vk::QueryType, count: u32) -> VkResult<Arc<Self>> {
        let handle = unsafe {
            device.handle.create_query_pool(
                &vk::QueryPoolCreateInfo::builder()
                    .query_type(query_type)
                    .query_count(count),
                None,
            )?
        };
        Ok(Arc::new(Self { device, handle }))
    }
}

impl Drop for QueryPool {
    fn drop(&mut self) {
        unsafe {
            self.device
                .handle
                .destroy_query_pool(Some(self.handle), None);
        }
    }
}

struct CommandBuffer {
    device: Arc<Device>,
    pool: vk::CommandPool,
    handle: vk::CommandBuffer,
}

impl CommandBuffer {
    // NOTE: `pool` must outlive the CommandBuffer
    // TODO: Create a safe wrapper for CommandPool
    fn new(device: Arc<Device>, pool: vk::CommandPool) -> VkResult<Self> {
        let handle = unsafe {
            device.handle.allocate_command_buffers_single(
                &vk::CommandBufferAllocateInfo::builder()
                    .command_pool(pool)
                    .command_buffer_count(1),
            )?
        };
        Ok(Self {
            device,
            pool,
            handle,
        })
    }

    fn reset(&mut self) -> VkResult<()> {
        unsafe {
            self.device
                .handle
                .reset_command_buffer(self.handle, vk::CommandBufferResetFlags::empty())
        }
    }
}

impl Drop for CommandBuffer {
    fn drop(&mut self) {
        unsafe {
            self.device
                .handle
                .free_command_buffers(self.pool, &[self.handle]);
        }
    }
}

pub struct VulkanContext {
    inner: Mutex<VulkanContextInner>,
}

struct VulkanContextInner {
    device: Arc<Device>,
    command_pool: vk::CommandPool,
    command_buffer: Vec<CommandBuffer>,
    query_pool: Vec<(Arc<QueryPool>, u32)>,

    fence_thread: Option<FenceThread<VulkanSubmission>>,

    sem: vk::Semaphore,
    seq: u64,
}

#[repr(C)]
pub struct VulkanSubmitAux {
    submit_before: vk::CommandBuffer,
    submit_after: vk::CommandBuffer,
    signal_sem: vk::Semaphore,
    signal_sem_value: u64,
}

impl VulkanContext {
    fn new(device: Arc<Device>) -> VkResult<Arc<VulkanContext>> {
        let ret = Arc::new(VulkanContext {
            inner: Mutex::new(VulkanContextInner::new(device)?),
        });
        let weak = Arc::downgrade(&ret);
        ret.inner.lock().fence_thread =
            Some(FenceThread::new(move |submission: VulkanSubmission| {
                let ctx = weak.upgrade().unwrap();
                submission.complete(&ctx).unwrap()
            }));
        Ok(ret)
    }
}

impl VulkanContextInner {
    fn new(device: Arc<Device>) -> VkResult<Self> {
        let command_pool = unsafe {
            device.handle.create_command_pool(
                &vk::CommandPoolCreateInfo::builder()
                    .queue_family_index(device.queue_family_index)
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                None,
            )
        }
        .unwrap();

        let sem = unsafe {
            device.handle.create_semaphore(
                &vk::SemaphoreCreateInfo::builder().insert_next(
                    &mut vk::SemaphoreTypeCreateInfo::builder()
                        .semaphore_type(vk::SemaphoreType::TIMELINE),
                ),
                None,
            )?
        };

        Ok(Self {
            device,
            command_pool,
            command_buffer: Vec::new(),
            query_pool: Vec::new(),
            fence_thread: None,
            sem,
            seq: 1,
        })
    }
}

impl Drop for VulkanContextInner {
    fn drop(&mut self) {
        // Drop existing references to command pool first
        self.command_buffer.clear();
        // Optional, but for sanity
        self.query_pool.clear();

        unsafe {
            self.device.handle.destroy_semaphore(Some(self.sem), None);
            self.device
                .handle
                .destroy_command_pool(Some(self.command_pool), None);
        }
    }
}

impl VulkanContextInner {
    fn get_query_pool(&mut self) -> VkResult<(Arc<QueryPool>, u32)> {
        let (pool, idx) = if let Some((pool, idx)) = self.query_pool.pop() {
            (pool, idx)
        } else {
            let count = 16;
            let pool = QueryPool::new(self.device.clone(), vk::QueryType::TIMESTAMP, count)?;
            self.query_pool
                .extend((0..count).map(|i| (pool.clone(), i)));
            self.query_pool.pop().unwrap()
        };
        unsafe {
            self.device.handle.reset_query_pool(pool.handle, idx, 1);
        }
        Ok((pool, idx))
    }

    fn get_command_buffer(&mut self) -> VkResult<CommandBuffer> {
        if let Some(cmd) = self.command_buffer.pop() {
            Ok(cmd)
        } else {
            CommandBuffer::new(self.device.clone(), self.command_pool)
        }
    }

    fn begin(&mut self, frame: &Arc<Frame>) {
        self.fence_thread
            .as_mut()
            .unwrap()
            .send(FenceWorkerMessage::BeginFrame(Arc::downgrade(frame)));
    }

    fn end(&mut self, frame: &Arc<Frame>) {
        self.fence_thread
            .as_mut()
            .unwrap()
            .send(FenceWorkerMessage::EndFrame(frame.clone()));
    }

    fn submit(&mut self) -> VkResult<VulkanSubmitAux> {
        let queries = (0..2)
            .map(|_| self.get_query_pool())
            .collect::<VkResult<Vec<_>>>()?;
        let command_buffers = (0..2)
            .map(|_| self.get_command_buffer())
            .collect::<VkResult<Vec<_>>>()?;
        for i in 0..2 {
            unsafe {
                self.device.handle.begin_command_buffer(
                    command_buffers[i].handle,
                    &vk::CommandBufferBeginInfo::builder()
                        .flags(CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )?;
                self.device.handle.cmd_write_timestamp2(
                    command_buffers[i].handle,
                    vk::PipelineStageFlags2::ALL_COMMANDS,
                    queries[i].0.handle,
                    queries[i].1,
                );
                self.device
                    .handle
                    .end_command_buffer(command_buffers[i].handle)?;
            }
        }
        let seq = self.seq;
        self.seq += 1;

        let ret = VulkanSubmitAux {
            submit_before: command_buffers[0].handle,
            submit_after: command_buffers[1].handle,
            signal_sem: self.sem,
            signal_sem_value: seq,
        };

        self.fence_thread
            .as_mut()
            .unwrap()
            .send(FenceWorkerMessage::Wait(VulkanSubmission {
                submission_ts: timestamp_now(),
                queries: queries.try_into().map_err(|_| ()).unwrap(),
                command_buffers: command_buffers.try_into().map_err(|_| ()).unwrap(),
                seq,
            }));

        Ok(ret)
    }
}

struct VulkanSubmission {
    queries: [(Arc<QueryPool>, u32); 2],
    command_buffers: [CommandBuffer; 2],
    seq: u64,

    submission_ts: Timestamp,
}

impl VulkanSubmission {
    fn complete(self, context: &VulkanContext) -> VkResult<(Timestamp, Timestamp, Timestamp)> {
        let device;
        let sem;
        {
            let lock = context.inner.lock();
            device = lock.device.clone();
            sem = lock.sem;
        }
        unsafe {
            device.handle.wait_semaphores(
                &vk::SemaphoreWaitInfo::builder().p_semaphores(&[sem], &[self.seq]),
                u64::MAX,
            )?;
        }

        let mut calibration = [0u64; 2];
        let mut deviation = 0u64;
        let timestamp_info = [
            *vk::CalibratedTimestampInfoEXT::builder().time_domain(vk::TimeDomainEXT::DEVICE),
            *vk::CalibratedTimestampInfoEXT::builder().time_domain(VULKAN_TIMESTAMP_DOMAIN),
        ];
        unsafe {
            device.handle.get_calibrated_timestamps_ext(
                &timestamp_info,
                calibration.as_mut_ptr(),
                &mut deviation,
            )?;
        }
        let process_timestamp = |(pool, index): &(Arc<QueryPool>, u32)| -> VkResult<u64> {
            let mut gpu_ts = [0u64; 1];
            unsafe {
                device.handle.get_query_pool_results(
                    pool.handle,
                    *index,
                    1,
                    &mut gpu_ts,
                    mem::size_of::<u64>() as _,
                    QueryResultFlags::N64 | QueryResultFlags::WAIT,
                )?;
            }
            let gpu_calibration = calibration[0];
            let cpu_calibration = timestamp_from_vulkan(calibration[1]);
            let valid_shift = 64 - device.queue_family_properties.timestamp_valid_bits;
            let gpu_delta = (gpu_ts[0] as i64 - gpu_calibration as i64)
                .wrapping_shl(valid_shift)
                .wrapping_shr(valid_shift);
            let calibrated = cpu_calibration as i64
                + (gpu_delta as f64 * device.limits.timestamp_period as f64) as i64;
            Ok(calibrated as u64)
        };
        let begin_ts = process_timestamp(&self.queries[0])?;
        let end_ts = process_timestamp(&self.queries[1])?;
        {
            let mut lock = context.inner.lock();
            for query in self.queries {
                lock.query_pool.push(query);
            }
            for mut buf in self.command_buffers {
                buf.reset()?;
                lock.command_buffer.push(buf);
            }
        }
        Ok((self.submission_ts, begin_ts, end_ts))
    }
}
