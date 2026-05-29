use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

type Job = Box<dyn FnOnce() + Send + 'static>;

/// Fixed-size pool of OS threads for CPU-bound jobs. Jobs are FIFO-queued and
/// run on the next free worker; cancellation is the caller's responsibility
/// (e.g. an `AtomicBool` the job polls), not something this pool tracks.
///
/// Worker handles are detached: the threads block on the job channel and exit
/// on their own when the pool (and thus the `Sender`) is dropped.
#[derive(Debug)]
pub struct ComputePool {
    sender: Sender<Job>,
}

impl ComputePool {
    pub fn new(threads: usize) -> Self {
        assert!(threads > 0);
        let (sender, receiver) = channel::<Job>();
        let receiver = Arc::new(Mutex::new(receiver));
        for _ in 0..threads {
            let receiver = receiver.clone();
            std::thread::spawn(move || worker_loop(&receiver));
        }
        Self { sender }
    }

    pub fn spawn(&self, job: impl FnOnce() + Send + 'static) {
        self.sender
            .send(Box::new(job))
            .expect("compute pool workers gone");
    }
}

fn worker_loop(receiver: &Mutex<Receiver<Job>>) {
    loop {
        // Holding the lock across the blocking recv is the standard work-queue
        // pattern: a worker only blocks here when the queue is empty, so it
        // never starves others of pending work.
        let job = receiver.lock().unwrap().recv();
        match job {
            Ok(job) => job(),
            Err(_) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn runs_every_job_across_workers() {
        let pool = ComputePool::new(4);
        let counter = Arc::new(AtomicUsize::new(0));
        let (done_tx, done_rx) = channel();

        for _ in 0..100 {
            let counter = counter.clone();
            let done_tx = done_tx.clone();
            pool.spawn(move || {
                counter.fetch_add(1, Ordering::Relaxed);
                done_tx.send(()).unwrap();
            });
        }
        drop(done_tx);

        // Block until exactly 100 completions arrive — no sleeps, no flakiness.
        for _ in 0..100 {
            done_rx.recv().unwrap();
        }
        assert_eq!(counter.load(Ordering::Relaxed), 100);
    }
}
