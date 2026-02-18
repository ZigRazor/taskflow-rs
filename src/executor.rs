use std::sync::{Arc, Mutex, Condvar};
use std::sync::atomic::{AtomicUsize, AtomicBool, Ordering};
use std::thread;
use std::collections::{HashMap, VecDeque};
use crate::taskflow::Taskflow;
use crate::task::{TaskWork, TaskId, TaskNode};
use crate::future::TaskflowFuture;
use crate::subflow::Subflow;

/// Executor - Thread pool for executing taskflows
pub struct Executor {
    num_workers: usize,
}

impl Executor {
    /// Create a new executor with specified number of worker threads
    pub fn new(num_workers: usize) -> Self {
        let num_workers = if num_workers == 0 {
            num_cpus::get()
        } else {
            num_workers
        };

        Self {
            num_workers,
        }
    }

    /// Run a taskflow once
    pub fn run(&mut self, taskflow: &Taskflow) -> TaskflowFuture {
        let graph = taskflow.get_graph();
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let condvar = Arc::new(Condvar::new());
        let shutdown = Arc::new(AtomicBool::new(false));
        let tasks_remaining = Arc::new(AtomicUsize::new(0));
        let dep_count = Arc::new(Mutex::new(HashMap::new()));
        
        // Build dependency count map and enqueue initial tasks
        {
            let graph_guard = graph.lock().unwrap();
            let mut dep_map = dep_count.lock().unwrap();
            let mut queue_guard = queue.lock().unwrap();
            
            for node in graph_guard.iter() {
                dep_map.insert(node.id, AtomicUsize::new(node.dependents.len()));
            }
            
            let total = graph_guard.len();
            tasks_remaining.store(total, Ordering::Release);
            
            // Find initial tasks (no dependencies)
            for node in graph_guard.iter() {
                if node.dependents.is_empty() {
                    queue_guard.push_back(node.id);
                }
            }
        }

        // Spawn worker threads
        let handles: Vec<_> = (0..self.num_workers)
            .map(|worker_id| {
                let graph = Arc::clone(&graph);
                let queue = Arc::clone(&queue);
                let condvar = Arc::clone(&condvar);
                let shutdown = Arc::clone(&shutdown);
                let dep_count = Arc::clone(&dep_count);
                let tasks_remaining = Arc::clone(&tasks_remaining);

                thread::spawn(move || {
                    Self::worker_loop(
                        worker_id,
                        graph,
                        queue,
                        condvar,
                        shutdown,
                        dep_count,
                        tasks_remaining,
                    );
                })
            })
            .collect();

        // Wake up all workers
        condvar.notify_all();

        TaskflowFuture::new(handles, shutdown)
    }

    fn worker_loop(
        _worker_id: usize,
        graph: Arc<Mutex<Vec<TaskNode>>>,
        queue: Arc<Mutex<VecDeque<TaskId>>>,
        condvar: Arc<Condvar>,
        shutdown: Arc<AtomicBool>,
        dep_count: Arc<Mutex<HashMap<TaskId, AtomicUsize>>>,
        tasks_remaining: Arc<AtomicUsize>,
    ) {
        loop {
            // Get next task
            let task_id = {
                let mut queue_guard = queue.lock().unwrap();
                
                loop {
                    if let Some(id) = queue_guard.pop_front() {
                        break Some(id);
                    }
                    
                    // Check if we should shutdown
                    if shutdown.load(Ordering::Acquire) {
                        return;
                    }
                    
                    // Check if all tasks are done
                    let remaining = tasks_remaining.load(Ordering::Acquire);
                    if remaining == 0 && queue_guard.is_empty() {
                        shutdown.store(true, Ordering::Release);
                        drop(queue_guard);
                        condvar.notify_all();
                        return;
                    }
                    
                    // Wait for work
                    queue_guard = condvar.wait(queue_guard).unwrap();
                }
            };

            if let Some(task_id) = task_id {
                // Get task work and successors
                let (work, successors) = {
                    let mut graph_guard = graph.lock().unwrap();
                    let node = graph_guard.iter_mut().find(|n| n.id == task_id).unwrap();
                    let work = node.work.take();
                    let successors = node.successors.clone();
                    (work, successors)
                };

                // Execute work outside the lock
                if let Some(work) = work {
                    match work {
                        TaskWork::Static(func) => {
                            func();
                        }
                        TaskWork::Subflow(func) => {
                            let next_id = Arc::new(Mutex::new({
                                let g = graph.lock().unwrap();
                                g.len()
                            }));
                            
                            let initial_graph_size = {
                                let g = graph.lock().unwrap();
                                g.len()
                            };
                            
                            let mut subflow = Subflow::new(Arc::clone(&graph), next_id);
                            func(&mut subflow);
                            
                            let new_tasks = {
                                let g = graph.lock().unwrap();
                                g.len() - initial_graph_size
                            };
                            
                            if new_tasks > 0 {
                                tasks_remaining.fetch_add(new_tasks, Ordering::Release);
                                
                                let mut dep_map = dep_count.lock().unwrap();
                                let graph_guard = graph.lock().unwrap();
                                let mut queue_guard = queue.lock().unwrap();
                                
                                for i in initial_graph_size..(initial_graph_size + new_tasks) {
                                    let node = &graph_guard[i];
                                    dep_map.insert(node.id, AtomicUsize::new(node.dependents.len()));
                                    
                                    if node.dependents.is_empty() {
                                        queue_guard.push_back(node.id);
                                    }
                                }
                                drop(queue_guard);
                                condvar.notify_all();
                            }
                        }
                        TaskWork::Condition(func) => {
                            let _result = func();
                        }
                    }
                }

                // Decrement tasks remaining
                tasks_remaining.fetch_sub(1, Ordering::Release);

                // Update successor dependencies and enqueue ready tasks
                let dep_map = dep_count.lock().unwrap();
                let mut ready_tasks = Vec::new();
                
                for succ_id in successors {
                    if let Some(count) = dep_map.get(&succ_id) {
                        let prev = count.fetch_sub(1, Ordering::AcqRel);
                        if prev == 1 {
                            ready_tasks.push(succ_id);
                        }
                    }
                }
                drop(dep_map);

                if !ready_tasks.is_empty() {
                    let mut queue_guard = queue.lock().unwrap();
                    for task_id in ready_tasks {
                        queue_guard.push_back(task_id);
                    }
                    drop(queue_guard);
                    condvar.notify_all();
                }
            }
        }
    }

    /// Block until all submitted taskflows complete
    pub fn wait_for_all(&self) {
        // This executor doesn't maintain state between runs
    }
}

impl Drop for Executor {
    fn drop(&mut self) {
        // Executor doesn't maintain persistent state
    }
}
