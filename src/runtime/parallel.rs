use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionStrategy {
    Sequential,
    TaskQueue,
    WorkerPool,
}

pub struct TaskHandle<T> {
    pub receiver: mpsc::Receiver<T>,
}

impl<T> TaskHandle<T> {
    pub fn join(self) -> Result<T, String> {
        self.receiver.recv().map_err(|e| e.to_string())
    }
}

type Job = Box<dyn FnOnce() + Send + 'static>;

pub struct ParallelRuntime {
    workers: Vec<Worker>,
    sender: Option<mpsc::Sender<Job>>,
}

#[allow(dead_code)]
struct Worker {
    id: usize,
    thread: Option<JoinHandle<()>>,
}

impl ParallelRuntime {
    pub fn new(num_threads: usize) -> Self {
        let (sender, receiver) = mpsc::channel::<Job>();
        let receiver = Arc::new(Mutex::new(receiver));
        let mut workers = Vec::with_capacity(num_threads);

        for id in 0..num_threads {
            let rx = Arc::clone(&receiver);
            let thread = thread::spawn(move || {
                loop {
                    let message = {
                        let lock = rx.lock().unwrap();
                        lock.recv()
                    };

                    match message {
                        Ok(job) => job(),
                        Err(_) => break,
                    }
                }
            });

            workers.push(Worker {
                id,
                thread: Some(thread),
            });
        }

        Self {
            workers,
            sender: Some(sender),
        }
    }

    pub fn spawn<F, R>(&self, f: F) -> TaskHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let (res_sender, res_receiver) = mpsc::channel();
        let job = Box::new(move || {
            let result = f();
            let _ = res_sender.send(result);
        });

        if let Some(ref sender) = self.sender {
            let _ = sender.send(job);
        }

        TaskHandle {
            receiver: res_receiver,
        }
    }

    pub fn run_parallel<F1, R1, F2, R2>(&self, f1: F1, f2: F2) -> (R1, R2)
    where
        F1: FnOnce() -> R1 + Send + 'static,
        R1: Send + 'static,
        F2: FnOnce() -> R2 + Send + 'static,
        R2: Send + 'static,
    {
        let handle1 = self.spawn(f1);
        let res2 = f2();
        let res1 = handle1.join().expect("Task 1 execution failed");
        (res1, res2)
    }

    pub fn par_map<T, R, F>(&self, items: Vec<T>, f: F) -> Vec<R>
    where
        T: Clone + Send + 'static,
        R: Send + 'static,
        F: Fn(T) -> R + Sync + Send + Clone + 'static,
    {
        if items.is_empty() {
            return Vec::new();
        }

        let num_workers = self.workers.len().max(1);
        let chunk_size = (items.len() + num_workers - 1) / num_workers;
        let mut handles = Vec::new();

        for chunk in items.chunks(chunk_size) {
            let chunk_vec: Vec<T> = chunk.to_vec();
            let f_clone = f.clone();
            let handle = self.spawn(move || {
                chunk_vec
                    .into_iter()
                    .map(|item| f_clone(item))
                    .collect::<Vec<R>>()
            });
            handles.push(handle);
        }

        let mut results = Vec::new();
        for h in handles {
            let chunk_res = h.join().expect("Parallel chunk failed");
            results.extend(chunk_res);
        }
        results
    }

    pub fn evaluate_strategy(
        trip_count: usize,
        is_pure: bool,
        cost_estimate: usize,
    ) -> ExecutionStrategy {
        if !is_pure || trip_count < 100 || cost_estimate < 500 {
            ExecutionStrategy::Sequential
        } else if trip_count < 1000 {
            ExecutionStrategy::TaskQueue
        } else {
            ExecutionStrategy::WorkerPool
        }
    }
}

impl Drop for ParallelRuntime {
    fn drop(&mut self) {
        drop(self.sender.take());
        for worker in &mut self.workers {
            if let Some(thread) = worker.thread.take() {
                let _ = thread.join();
            }
        }
    }
}
