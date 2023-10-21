use std::sync::mpsc;

use crate::task::TaskStats;

pub struct FenceThread<S, C> {
    thread: Option<std::thread::JoinHandle<()>>,
    tx: Option<mpsc::Sender<FenceWorkerMessage<S, C>>>,
    rx: mpsc::Receiver<FenceWorkerResult<C>>,
}

impl<S: Send + 'static, C: Send + 'static> FenceThread<S, C> {
    pub fn new<F: FnMut(S) -> TaskStats + Send + 'static>(
        callback: F,
    ) -> Self {
        let (req_tx, req_rx) = mpsc::channel();
        let (res_tx, res_rx) = mpsc::channel();
        let mut worker = FenceWorker {
            rx: req_rx,
            tx: res_tx,
            callback,
        };
        let thread = std::thread::spawn(move || worker.run());
        Self {
            thread: Some(thread),
            tx: Some(req_tx),
            rx: res_rx,
        }
    }

    pub fn send(&mut self, msg: FenceWorkerMessage<S, C>) {
        self.tx.as_mut().unwrap().send(msg).unwrap();
    }
    
    pub fn recv(&mut self) -> Option<FenceWorkerResult<C>> {
        self.rx.try_recv().ok()
    }
}

impl<S, C> Drop for FenceThread<S, C> {
    fn drop(&mut self) {
        let _ = self.tx.take();
        let _ = self.thread.take().unwrap().join();
    }
}

struct FenceWorker<S, C, F: FnMut(S) -> TaskStats> {
    rx: mpsc::Receiver<FenceWorkerMessage<S, C>>,
    tx: mpsc::Sender<FenceWorkerResult<C>>,

    callback: F,
}

pub enum FenceWorkerMessage<S, C> {
    Submission(S),
    Notification(C),
}

pub enum FenceWorkerResult<C> {
    Submission(TaskStats),
    Notification(C),
}

impl<S, C, F: FnMut(S) -> TaskStats> FenceWorker<S, C, F> {
    fn run(&mut self) {
        while let Ok(job) = self.rx.recv() {
            match job {
                FenceWorkerMessage::Submission(submission) => {
                    let stats = (self.callback)(submission);
                    let _ = self.tx.send(FenceWorkerResult::Submission(stats));
                }
                FenceWorkerMessage::Notification(context) => {
                    let _ = self.tx.send(FenceWorkerResult::Notification(context));
                }
            }
        }
    }
}
