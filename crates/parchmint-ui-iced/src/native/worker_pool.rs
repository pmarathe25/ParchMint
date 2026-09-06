use std::sync::{Arc, Mutex, OnceLock, mpsc};

type Job = Box<dyn FnOnce() + Send + 'static>;

const WORKERS: usize = 4;
const QUEUED_JOBS: usize = 128;

struct WorkerPool {
    sender: mpsc::SyncSender<Job>,
}

impl WorkerPool {
    fn new(workers: usize, capacity: usize) -> Result<Self, String> {
        let (sender, receiver) = mpsc::sync_channel::<Job>(capacity);
        let receiver = Arc::new(Mutex::new(receiver));
        for index in 0..workers {
            let receiver = Arc::clone(&receiver);
            std::thread::Builder::new()
                .name(format!("parchmint-worker-{index}"))
                .spawn(move || {
                    loop {
                        let job = receiver.lock().expect("worker queue lock").recv();
                        let Ok(job) = job else { break };
                        // A failed task drops its result sender, informing its
                        // caller, without permanently losing a worker slot.
                        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job));
                    }
                })
                .map_err(|error| error.to_string())?;
        }
        Ok(Self { sender })
    }

    fn submit(&self, job: Job) -> Result<(), String> {
        self.sender.try_send(job).map_err(|error| match error {
            mpsc::TrySendError::Full(_) => {
                "Background work queue is full; please retry the action.".to_owned()
            }
            mpsc::TrySendError::Disconnected(_) => "Background workers are unavailable.".to_owned(),
        })
    }
}

pub(super) fn submit(job: impl FnOnce() + Send + 'static) -> Result<(), String> {
    static POOL: OnceLock<Result<WorkerPool, String>> = OnceLock::new();
    POOL.get_or_init(|| WorkerPool::new(WORKERS, QUEUED_JOBS))
        .as_ref()
        .map_err(Clone::clone)?
        .submit(Box::new(job))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_and_queue_limits_do_not_block_the_submitter() {
        let pool = WorkerPool::new(1, 1).unwrap();
        let (entered, started) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        pool.submit(Box::new(move || {
            entered.send(()).unwrap();
            wait.recv().unwrap();
        }))
        .unwrap();
        started
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        let (done, finished) = mpsc::channel();
        pool.submit(Box::new(move || {
            done.send(()).unwrap();
        }))
        .unwrap();
        assert!(pool.submit(Box::new(|| panic!("must not run"))).is_err());
        release.send(()).unwrap();
        finished
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
    }

    #[test]
    fn a_panicking_job_does_not_retire_its_worker() {
        let pool = WorkerPool::new(1, 2).unwrap();
        pool.submit(Box::new(|| panic!("injected worker failure")))
            .unwrap();
        let (done, finished) = mpsc::channel();
        pool.submit(Box::new(move || {
            done.send(()).unwrap();
        }))
        .unwrap();
        finished
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
    }
}
