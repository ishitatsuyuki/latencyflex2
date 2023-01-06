use std::mem::MaybeUninit;
use std::sync::{mpsc, Arc, Weak};
use std::thread::JoinHandle;
use std::{ptr, thread};

use parking_lot::Mutex;
use windows::core::Interface;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::Win32::System::WindowsProgramming::INFINITE;

use crate::{timestamp_from_qpc, Frame, Interval, MarkType, Timestamp};

pub mod entrypoint;

pub struct Dx12Context {
    inner: Mutex<Dx12ContextInner>,
}

struct Dx12ContextInner {
    device: ID3D12Device4,
    timestamp_command_list: [ID3D12GraphicsCommandList; 2],
    command_allocator: Vec<ID3D12CommandAllocator>,
    query_heap: ID3D12QueryHeap,
    query_staging: Vec<(ID3D12Resource, u32)>,

    // We expect the application to hold a reference to the frame until the call the "end" marker,
    // where we'll finalize and set this to None as well.
    // If the application releases the reference without calling the "end" marker, we just skip
    // processing for events happened during that.
    current_frame: Option<Weak<Frame>>,
    current_frame_begun: bool,

    fence_thread: Option<JoinHandle<()>>,
    fence_tx: Option<mpsc::Sender<Dx12FenceMsg>>,

    fence: ID3D12Fence,
    fence_value: u64,
}

/// cbindgen:ignore
#[repr(C)]
#[derive(Default)]
pub struct Dx12SubmitAux {
    execute_before: Option<ID3D12GraphicsCommandList>,
    execute_after: Option<ID3D12GraphicsCommandList>,
    signal_fence: Option<ID3D12Fence>,
    signal_fence_value: u64,
}

impl Dx12Context {
    pub fn new(device: &ID3D12Device) -> Arc<Dx12Context> {
        let device = device.cast::<ID3D12Device4>().unwrap();
        let context = Arc::new(Dx12Context {
            inner: Mutex::new(Dx12ContextInner::new(device)),
        });
        context
            .inner
            .lock()
            .fence_tx
            .as_mut()
            .unwrap()
            .send(Dx12FenceMsg::SetContext(Arc::downgrade(&context)))
            .unwrap();
        context
    }
}

impl Dx12ContextInner {
    fn new(device: ID3D12Device4) -> Dx12ContextInner {
        let query_heap = unsafe {
            let mut query_heap = MaybeUninit::uninit();
            device
                .CreateQueryHeap(
                    &D3D12_QUERY_HEAP_DESC {
                        Type: D3D12_QUERY_HEAP_TYPE_TIMESTAMP,
                        Count: 2,
                        NodeMask: 0,
                    },
                    query_heap.as_mut_ptr(),
                )
                .unwrap();
            query_heap.assume_init().unwrap()
        };

        let timestamp_command_list = [(); 2].map(|_| unsafe {
            device
                .CreateCommandList1(
                    0,
                    D3D12_COMMAND_LIST_TYPE_DIRECT,
                    D3D12_COMMAND_LIST_FLAG_NONE,
                )
                .unwrap()
        });

        let fence: ID3D12Fence = unsafe { device.CreateFence(0, D3D12_FENCE_FLAG_NONE).unwrap() };

        let event = unsafe { CreateEventW(None, false, false, None).unwrap() };

        let (fence_tx, fence_rx) = mpsc::channel();
        let mut fence_thread_ctx = Dx12FenceWorker {
            context: None,
            rx: fence_rx,
            tracker: None,
            fence: fence.clone(),
            event,
        };
        let fence_thread = thread::spawn(move || fence_thread_ctx.run());

        Dx12ContextInner {
            device,
            timestamp_command_list,
            command_allocator: vec![],
            query_heap,
            query_staging: vec![],
            current_frame: None,
            current_frame_begun: false,
            fence_thread: Some(fence_thread),
            fence_tx: Some(fence_tx),
            fence,
            fence_value: 1,
        }
    }
}

impl Drop for Dx12ContextInner {
    fn drop(&mut self) {
        let _ = self.fence_tx.take();
        self.fence_thread.take().unwrap().join().unwrap();
    }
}

impl Dx12ContextInner {
    fn get_query(&mut self) -> (ID3D12Resource, u32) {
        if let Some(q) = self.query_staging.pop() {
            q
        } else {
            let count = 16;
            let resource_desc = D3D12_RESOURCE_DESC {
                Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
                Alignment: 0,
                Width: (count * 8) as u64,
                Height: 1,
                DepthOrArraySize: 1,
                MipLevels: 1,
                Format: DXGI_FORMAT_UNKNOWN,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
                Flags: D3D12_RESOURCE_FLAG_NONE,
            };
            let heap_properties = D3D12_HEAP_PROPERTIES {
                Type: D3D12_HEAP_TYPE_READBACK,
                ..Default::default()
            };
            let buf: ID3D12Resource = unsafe {
                let mut buf = MaybeUninit::uninit();
                self.device
                    .CreateCommittedResource(
                        &heap_properties,
                        D3D12_HEAP_FLAG_NONE,
                        &resource_desc,
                        D3D12_RESOURCE_STATE_COPY_DEST,
                        None,
                        buf.as_mut_ptr(),
                    )
                    .unwrap();
                buf.assume_init().unwrap()
            };
            unsafe {
                buf.Map(0, None, None).unwrap();
            }
            for i in 0..count {
                self.query_staging.push((buf.clone(), i));
            }
            self.query_staging.pop().unwrap()
        }
    }

    fn get_allocator(&mut self) -> ID3D12CommandAllocator {
        if let Some(a) = self.command_allocator.pop() {
            a
        } else {
            unsafe {
                self.device
                    .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
                    .unwrap()
            }
        }
    }

    fn begin(&mut self, frame: &Arc<Frame>) {
        let weak = Arc::downgrade(frame);
        self.current_frame = Some(weak.clone());
        self.current_frame_begun = false;

        self.fence_tx
            .as_mut()
            .unwrap()
            .send(Dx12FenceMsg::BeginFrame(weak))
            .unwrap();
    }

    fn end(&mut self, frame: &Arc<Frame>) {
        self.current_frame = None;

        self.fence_tx
            .as_mut()
            .unwrap()
            .send(Dx12FenceMsg::EndFrame(frame.clone()))
            .unwrap();
    }

    fn submit(&mut self, queue: &ID3D12CommandQueue) -> Dx12SubmitAux {
        if self.current_frame.is_none() {
            return Dx12SubmitAux::default();
        }

        let allocator = self.get_allocator();

        let build_timestamp_command_list = |command_list: &ID3D12GraphicsCommandList,
                                            query_heap: (&ID3D12QueryHeap, u32),
                                            staging: &(ID3D12Resource, u32)|
         -> windows::core::Result<()> {
            unsafe {
                command_list.Reset(&allocator, None)?;
                command_list.EndQuery(query_heap.0, D3D12_QUERY_TYPE_TIMESTAMP, query_heap.1);
                command_list.ResolveQueryData(
                    query_heap.0,
                    D3D12_QUERY_TYPE_TIMESTAMP,
                    query_heap.1,
                    1,
                    &staging.0,
                    (staging.1 * 8) as u64,
                );
                command_list.Close()?;
            };
            Ok(())
        };

        let (execute_before, begin_query) = if !self.current_frame_begun {
            let query = self.get_query();
            build_timestamp_command_list(
                &self.timestamp_command_list[0],
                (&self.query_heap, 0),
                &query,
            )
            .unwrap();
            self.current_frame_begun = true;
            (Some(self.timestamp_command_list[0].clone()), Some(query))
        } else {
            (None, None)
        };

        let end_query = self.get_query();
        build_timestamp_command_list(
            &self.timestamp_command_list[1],
            (&self.query_heap, 1),
            &end_query,
        )
        .unwrap();

        let fence_value = self.fence_value;
        self.fence_value += 1;

        self.fence_tx
            .as_mut()
            .unwrap()
            .send(Dx12FenceMsg::Wait(Dx12FenceWait {
                queue: queue.clone(),
                value: fence_value,
                allocator,
                begin_ts: begin_query,
                end_ts: end_query,
            }))
            .unwrap();

        Dx12SubmitAux {
            execute_before,
            execute_after: Some(self.timestamp_command_list[1].clone()),
            signal_fence: Some(self.fence.clone()),
            signal_fence_value: fence_value,
        }
    }
}

struct Dx12FenceWait {
    value: u64,
    queue: ID3D12CommandQueue,
    allocator: ID3D12CommandAllocator,
    begin_ts: Option<(ID3D12Resource, u32)>,
    end_ts: (ID3D12Resource, u32),
}

enum Dx12FenceMsg {
    SetContext(Weak<Dx12Context>),
    BeginFrame(Weak<Frame>),
    Wait(Dx12FenceWait),
    EndFrame(Arc<Frame>),
}

struct Dx12FenceWorker {
    context: Option<Weak<Dx12Context>>,
    rx: mpsc::Receiver<Dx12FenceMsg>,

    tracker: Option<Dx12FenceWorkerTracker>,

    fence: ID3D12Fence,
    event: HANDLE,
}

struct Dx12FenceWorkerTracker {
    frame: Weak<Frame>,
    end_ts: Option<Timestamp>,
    queuing_delay: Interval,
}

impl Dx12FenceWorker {
    fn run(&mut self) {
        while let Ok(job) = self.rx.recv() {
            match job {
                Dx12FenceMsg::SetContext(ctx) => {
                    self.context = Some(ctx);
                }
                Dx12FenceMsg::BeginFrame(frame) => {
                    self.tracker = Some(Dx12FenceWorkerTracker {
                        frame,
                        end_ts: None,
                        queuing_delay: u64::MAX,
                    });
                }
                Dx12FenceMsg::Wait(job) => {
                    self.process_fence_wait(job).unwrap();
                }
                Dx12FenceMsg::EndFrame(frame) => {
                    self.process_end_frame(frame);
                }
            }
        }
    }

    fn process_fence_wait(&mut self, job: Dx12FenceWait) -> windows::core::Result<()> {
        unsafe {
            self.fence.SetEventOnCompletion(job.value, self.event)?;
            WaitForSingleObject(self.event, INFINITE).ok()?;
            job.allocator.Reset()?;
        }

        let mut gpu_calibration = 0;
        let mut cpu_qpc = 0;
        let timestamp_frequency;
        unsafe {
            job.queue
                .GetClockCalibration(&mut gpu_calibration, &mut cpu_qpc)?;
            timestamp_frequency = job.queue.GetTimestampFrequency()?;
        }
        let cpu_calibration = timestamp_from_qpc(cpu_qpc);

        let context = self.context.as_mut().and_then(|weak| weak.upgrade());
        let context = match context {
            Some(context) => context,
            None => return Ok(()),
        };
        let mut context = context.inner.lock();
        context.command_allocator.push(job.allocator);

        let process_timestamp =
            |(buf, index): &(ID3D12Resource, u32)| -> windows::core::Result<u64> {
                let gpu_ts = unsafe {
                    let mut base = ptr::null_mut();
                    buf.Map(0, None, Some(&mut base as _))?;
                    let ret = ptr::read((base as *const u64).add(*index as usize));
                    buf.Unmap(0, None);
                    ret
                };
                let gpu_delta = gpu_ts as i64 - gpu_calibration as i64;
                let calibrated =
                    cpu_calibration as i64 + gpu_delta * 1_000_000_000 / timestamp_frequency as i64;
                Ok(calibrated as u64)
            };

        // The application might end up mismatching call pairs, fail gracefully in such cases.
        let mut tracker = self.tracker.as_mut();

        if let Some(res) = job.begin_ts {
            let begin = process_timestamp(&res)?;
            if let Some(frame) = tracker.as_mut().and_then(|t| t.frame.upgrade()) {
                frame.mark(1000, MarkType::Begin, begin);
            }

            context.query_staging.push(res);
        }
        if let Some(tracker) = &mut tracker {
            tracker.end_ts = Some(process_timestamp(&job.end_ts)?);
        }
        context.query_staging.push(job.end_ts);

        Ok(())
    }

    fn process_end_frame(&mut self, frame: Arc<Frame>) {
        let tracker = self.tracker.take().unwrap();
        assert_eq!(Arc::as_ptr(&frame), Weak::as_ptr(&tracker.frame));
        if let Some(end_ts) = tracker.end_ts {
            frame.mark(1000, MarkType::End, end_ts);
        }
        // TODO: queueing delay
    }
}

impl Drop for Dx12FenceWorker {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.event);
        }
    }
}
