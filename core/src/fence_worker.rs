use crate::{Frame, Interval, MarkType, Timestamp};
use std::sync::{mpsc, Arc, Weak};

pub struct FenceThread<S> {
    thread: Option<std::thread::JoinHandle<()>>,
    tx: Option<mpsc::Sender<FenceWorkerMessage<S>>>,
}

impl<S: Send + 'static> FenceThread<S> {
    pub fn new<F: FnMut(S) -> (Timestamp, Timestamp, Timestamp) + Send + 'static>(
        callback: F,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let mut worker = FenceWorker {
            rx,
            tracker: None,
            last_finish: 0,
            callback,
        };
        let thread = std::thread::spawn(move || worker.run());
        Self {
            thread: Some(thread),
            tx: Some(tx),
        }
    }

    pub fn send(&mut self, msg: FenceWorkerMessage<S>) {
        self.tx.as_mut().unwrap().send(msg).unwrap();
    }
}

impl<S> Drop for FenceThread<S> {
    fn drop(&mut self) {
        let _ = self.tx.take();
        let _ = self.thread.take().unwrap().join();
    }
}

struct FenceWorker<S, F: FnMut(S) -> (Timestamp, Timestamp, Timestamp)> {
    rx: mpsc::Receiver<FenceWorkerMessage<S>>,
    tracker: Option<Tracker>,

    last_finish: Timestamp,

    callback: F,
}

struct Tracker {
    frame: Weak<Frame>,
    begin_ts: Option<Timestamp>,
    end_ts: Option<Timestamp>,
    duration: Interval,
    queuing_delay: Option<Interval>,
}

pub enum FenceWorkerMessage<S> {
    BeginFrame(Weak<Frame>),
    Wait(S),
    EndFrame(Arc<Frame>),
}

impl<S, F: FnMut(S) -> (Timestamp, Timestamp, Timestamp)> FenceWorker<S, F> {
    fn run(&mut self) {
        while let Ok(job) = self.rx.recv() {
            match job {
                FenceWorkerMessage::BeginFrame(frame) => {
                    self.tracker = Some(Tracker {
                        frame,
                        begin_ts: None,
                        end_ts: None,
                        queuing_delay: None,
                        duration: 0,
                    });
                }
                FenceWorkerMessage::Wait(job) => {
                    let (submission_ts, begin_ts, end_ts) = (self.callback)(job);
                    if let Some(tr) = self.tracker.as_mut() {
                        tr.begin_ts =
                            Some(tr.begin_ts.map(|ts| ts.min(begin_ts)).unwrap_or(begin_ts));
                        tr.end_ts = Some(tr.end_ts.map(|ts| ts.max(end_ts)).unwrap_or(end_ts));

                        let queueing_delay = self.last_finish.saturating_sub(submission_ts);
                        tr.queuing_delay = Some(
                            tr.queuing_delay
                                .map(|ts| ts.min(queueing_delay))
                                .unwrap_or(queueing_delay),
                        );

                        let duration = end_ts.saturating_sub(self.last_finish.max(submission_ts));
                        tr.duration += duration;

                        self.last_finish = end_ts;
                    }
                }
                FenceWorkerMessage::EndFrame(frame) => {
                    let tracker = self.tracker.take().unwrap();
                    assert_eq!(Arc::as_ptr(&frame), Weak::as_ptr(&tracker.frame));
                    if let Some(begin_ts) = tracker.begin_ts {
                        frame.mark(1000, MarkType::Begin, begin_ts);
                    }
                    if let Some(end_ts) = tracker.end_ts {
                        frame.mark(1000, MarkType::End, end_ts);
                    }
                    frame.set_inv_throughput(1000, tracker.duration);
                    if let Some(queueing_delay) = tracker.queuing_delay {
                        frame.set_queueing_delay(800, queueing_delay);
                    }
                }
            }
        }
    }
}
