use std::collections::HashMap;
use std::mem;
use std::sync::{Arc, Weak};

use parking_lot::Mutex;
use spark::vk::{CommandBufferUsageFlags, QueryResultFlags};
use spark::{vk, Builder};

pub use crate::fence_worker::FenceWorkerResult;
use crate::fence_worker::{FenceThread, FenceWorkerMessage};
use crate::task::TaskStats;
use crate::time;
use crate::Timestamp;

mod entrypoint;

type VkResult<T> = Result<T, vk::Result>;

struct Device {
    handle: spark::Device,
    limits: vk::PhysicalDeviceLimits,
    queue_family_properties: Vec<vk::QueueFamilyProperties>,
}

impl Device {
    fn new(
        instance: spark::Instance,
        phys_device: vk::PhysicalDevice,
        device: spark::Device,
    ) -> Arc<Device> {
        unsafe {
            let limits = instance.get_physical_device_properties(phys_device).limits;
            let queue_family_properties =
                instance.get_physical_device_queue_family_properties_to_vec(phys_device);
            Arc::new(Device {
                handle: device,
                limits,
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

pub struct VulkanContext<C> {
    inner: Mutex<VulkanContextInner<C>>,
}

struct VulkanContextInner<C> {
    device: Arc<Device>,

    query_pool: Vec<(Arc<QueryPool>, u32)>,
    queue_families: HashMap<u32, QueueFamilyContext<C>>,
}

struct QueueFamilyContext<C> {
    device: Arc<Device>,

    command_pool: vk::CommandPool,
    command_buffer: Vec<CommandBuffer>,
    queues: Vec<QueueContext<C>>,
}

struct QueueContext<C> {
    device: Arc<Device>,

    sem: vk::Semaphore,
    seq: u64,

    fence_thread: Option<FenceThread<VulkanSubmission, C>>,
}

#[repr(C)]
pub struct VulkanSubmitAux {
    pub submit_before: vk::CommandBuffer,
    pub submit_after: vk::CommandBuffer,
    pub signal_sem: vk::Semaphore,
    pub signal_sem_value: u64,
}

impl<C: Send + 'static> VulkanContext<C> {
    pub fn new(
        instance: spark::Instance,
        phys_device: vk::PhysicalDevice,
        device: spark::Device,
        queues: Vec<(u32, Vec<u32>)>,
    ) -> VkResult<Arc<VulkanContext<C>>> {
        let device = Device::new(instance, phys_device, device);
        let queue_families = queues
            .into_iter()
            .map(
                |(queue_family, queues)| -> VkResult<(u32, QueueFamilyContext<C>)> {
                    let context = QueueFamilyContext::new(
                        device.clone(),
                        queue_family,
                        queues
                            .into_iter()
                            .map(|_| QueueContext::new(device.clone()))
                            .collect::<VkResult<Vec<QueueContext<C>>>>()?,
                    )?;
                    Ok((queue_family, context))
                },
            )
            .collect::<VkResult<HashMap<u32, QueueFamilyContext<C>>>>()?;
        let context = VulkanContextInner {
            device,
            query_pool: Vec::new(),
            queue_families,
        };
        let ret = Arc::new(VulkanContext {
            inner: Mutex::new(context),
        });
        let weak = Arc::downgrade(&ret);
        for (_, queues) in &mut ret.inner.lock().queue_families {
            for queue in &mut queues.queues {
                queue.init_fence_thread(weak.clone());
            }
        }
        Ok(ret)
    }

    pub fn submit(&self, queue_family_index: u32, queue_index: u32) -> VkResult<VulkanSubmitAux> {
        self.inner.lock().submit(queue_family_index, queue_index)
    }

    pub fn notify(&self, queue_family_index: u32, queue_index: u32, context: C) {
        self.inner
            .lock()
            .notify(queue_family_index, queue_index, context);
    }

    pub fn get_result(
        &self,
        queue_family_index: u32,
        queue_index: u32,
    ) -> Option<FenceWorkerResult<C>> {
        self.inner
            .lock()
            .get_result(queue_family_index, queue_index)
    }
}

impl<C> Drop for VulkanContextInner<C> {
    fn drop(&mut self) {
        self.queue_families.clear();
        self.query_pool.clear();
    }
}

impl<C: Send + 'static> VulkanContextInner<C> {
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

    fn submit(&mut self, queue_family_index: u32, queue_index: u32) -> VkResult<VulkanSubmitAux> {
        let queries = (0..2)
            .map(|_| self.get_query_pool())
            .collect::<VkResult<Vec<_>>>()?;
        let qf = self.queue_families.get_mut(&queue_family_index).unwrap();
        let command_buffers = (0..2)
            .map(|_| qf.get_command_buffer())
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

        let queue_ctx = &mut qf.queues[queue_index as usize];
        let seq = queue_ctx.seq;
        queue_ctx.seq += 1;

        let ret = VulkanSubmitAux {
            submit_before: command_buffers[0].handle,
            submit_after: command_buffers[1].handle,
            signal_sem: queue_ctx.sem,
            signal_sem_value: seq,
        };

        queue_ctx
            .fence_thread
            .as_mut()
            .unwrap()
            .send(FenceWorkerMessage::Submission(VulkanSubmission {
                queue_family_index,
                queue_index,
                submission_ts: time::now(),
                queries: queries.try_into().map_err(|_| ()).unwrap(),
                command_buffers: command_buffers.try_into().map_err(|_| ()).unwrap(),
                seq,
            }));

        Ok(ret)
    }

    fn notify(&mut self, queue_family_index: u32, queue_index: u32, context: C) {
        let queue_ctx = &mut self
            .queue_families
            .get_mut(&queue_family_index)
            .unwrap()
            .queues[queue_index as usize];
        queue_ctx
            .fence_thread
            .as_mut()
            .unwrap()
            .send(FenceWorkerMessage::Notification(context));
    }

    fn get_result(
        &mut self,
        queue_family_index: u32,
        queue_index: u32,
    ) -> Option<FenceWorkerResult<C>> {
        let queue_ctx = &mut self
            .queue_families
            .get_mut(&queue_family_index)
            .unwrap()
            .queues[queue_index as usize];
        queue_ctx.fence_thread.as_mut().unwrap().recv()
    }
}

impl<C: Send> QueueFamilyContext<C> {
    fn new(
        device: Arc<Device>,
        queue_family_index: u32,
        queues: Vec<QueueContext<C>>,
    ) -> VkResult<Self> {
        let command_pool = unsafe {
            device.handle.create_command_pool(
                &vk::CommandPoolCreateInfo::builder()
                    .queue_family_index(queue_family_index)
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                None,
            )
        }
        .unwrap();
        Ok(Self {
            device: device.clone(),
            command_pool,
            command_buffer: Vec::new(),
            queues,
        })
    }

    fn get_command_buffer(&mut self) -> VkResult<CommandBuffer> {
        if let Some(cmd) = self.command_buffer.pop() {
            Ok(cmd)
        } else {
            CommandBuffer::new(self.device.clone(), self.command_pool)
        }
    }
}

impl<C> Drop for QueueFamilyContext<C> {
    fn drop(&mut self) {
        self.command_buffer.clear();
        unsafe {
            self.device
                .handle
                .destroy_command_pool(Some(self.command_pool), None);
        }
    }
}

impl<C: Send + 'static> QueueContext<C> {
    fn new(device: Arc<Device>) -> VkResult<Self> {
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
            fence_thread: None,
            sem,
            seq: 1,
        })
    }

    fn init_fence_thread(&mut self, context: Weak<VulkanContext<C>>) {
        self.fence_thread = Some(FenceThread::new(move |submission: VulkanSubmission| {
            let context = context.upgrade().unwrap();
            submission.complete(&context).unwrap()
        }));
    }
}

impl<C> Drop for QueueContext<C> {
    fn drop(&mut self) {
        unsafe {
            self.device.handle.destroy_semaphore(Some(self.sem), None);
        }
    }
}

struct VulkanSubmission {
    queue_family_index: u32,
    queue_index: u32,

    queries: [(Arc<QueryPool>, u32); 2],
    command_buffers: [CommandBuffer; 2],
    seq: u64,

    submission_ts: Timestamp,
}

impl VulkanSubmission {
    fn complete<C>(self, context: &VulkanContext<C>) -> VkResult<TaskStats> {
        let qfi = self.queue_family_index;
        let qi = self.queue_index;
        let device;
        let sem;
        {
            let lock = context.inner.lock();
            device = lock.device.clone();
            sem = lock.queue_families[&qfi].queues[qi as usize].sem;
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
            *vk::CalibratedTimestampInfoEXT::builder().time_domain(time::VULKAN_TIMESTAMP_DOMAIN),
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
            let cpu_calibration = time::timestamp_from_vulkan(calibration[1]);
            let valid_shift =
                64 - device.queue_family_properties[qfi as usize].timestamp_valid_bits;
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
                lock.queue_families
                    .get_mut(&qfi)
                    .unwrap()
                    .command_buffer
                    .push(buf);
            }
        }
        Ok(TaskStats {
            queued: self.submission_ts,
            start: begin_ts,
            end: end_ts,
        })
    }
}
