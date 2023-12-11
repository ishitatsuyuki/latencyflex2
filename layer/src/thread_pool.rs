use std::sync::mpsc::Sender;
use std::thread::JoinHandle;

/// A simple, single-threaded thread pool for deferred work.
pub struct ThreadPool {
    thread: Option<JoinHandle<()>>,
    tx: Sender<Box<dyn FnOnce() + 'static>>,
}

impl ThreadPool {
    fn 

    pub fn new() -> Self {
        let thread =
        Self {

        }
    }
}