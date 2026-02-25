use std::sync::{Arc, Mutex, Condvar};
use std::sync::atomic::{AtomicUsize, AtomicBool, Ordering};
use std::thread;
use std::collections::{HashMap, VecDeque};
use crate::taskflow::Taskflow;
use crate::task::{TaskWork, TaskId, TaskNode};
use crate::future::TaskflowFuture;
use crate::subflow::Subflow;
use crate::condition::BranchId;

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
    
    /// Count all tasks reachable from a set of starting tasks
    fn count_reachable_tasks(graph: &Arc<Mutex<Vec<TaskNode>>>, start_tasks: &[TaskId]) -> usize {
        let graph_guard = graph.lock().unwrap();
        let mut visited = std::collections::HashSet::new();
        let mut to_visit: Vec<TaskId> = start_tasks.to_vec();
        
        while let Some(task_id) = to_visit.pop() {
            if visited.insert(task_id) {
                // Find this task's successors
                if let Some(node) = graph_guard.iter().find(|n| n.id == task_id) {
                    for succ in &node.successors {
                        if !visited.contains(succ) {
                            to_visit.push(*succ);
                        }
                    }
                }
            }
        }
        
        visited.len()
    }

    /// Run a taskflow once
    pub fn run(&mut self, taskflow: &Taskflow) -> TaskflowFuture {
        let graph = taskflow.get_graph();
        let conditional_branches = taskflow.get_conditional_branches();
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
                let conditional_branches = Arc::clone(&conditional_branches);
                let queue = Arc::clone(&queue);
                let condvar = Arc::clone(&condvar);
                let shutdown = Arc::clone(&shutdown);
                let dep_count = Arc::clone(&dep_count);
                let tasks_remaining = Arc::clone(&tasks_remaining);

                thread::spawn(move || {
                    Self::worker_loop(
                        worker_id,
                        graph,
                        conditional_branches,
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
    
    /// Run a taskflow N times sequentially
    /// 
    /// **Note**: Creates a new taskflow for each iteration using the provided factory function.
    /// This is necessary because task closures (FnOnce) can only execute once.
    /// 
    /// # Example
    /// ```
    /// use taskflow_rs::{Executor, Taskflow};
    /// use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
    /// 
    /// let mut executor = Executor::new(4);
    /// let counter = Arc::new(AtomicUsize::new(0));
    /// 
    /// executor.run_n(5, || {
    ///     let mut taskflow = Taskflow::new();
    ///     let c = counter.clone();
    ///     taskflow.emplace(move || {
    ///         c.fetch_add(1, Ordering::Relaxed);
    ///         println!("Processing...");
    ///     });
    ///     taskflow
    /// }).wait();
    /// 
    /// assert_eq!(counter.load(Ordering::Relaxed), 5);
    /// ```
    pub fn run_n<F>(&mut self, n: usize, mut factory: F) -> TaskflowFuture
    where
        F: FnMut() -> Taskflow,
    {
        if n == 0 {
            return self.run(&factory());
        }
        
        // Run n-1 times and wait
        for _ in 0..n-1 {
            let taskflow = factory();
            self.run(&taskflow).wait();
        }
        
        // Return the future for the last run
        let taskflow = factory();
        self.run(&taskflow)
    }
    
    /// Run a taskflow repeatedly until a predicate returns true
    /// 
    /// Creates a new taskflow for each iteration using the provided factory function.
    /// The predicate is checked after each execution.
    /// 
    /// # Example
    /// ```
    /// use taskflow_rs::{Executor, Taskflow};
    /// use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
    /// 
    /// let mut executor = Executor::new(4);
    /// let sum = Arc::new(AtomicUsize::new(0));
    /// 
    /// let s = sum.clone();
    /// executor.run_until(
    ///     || {
    ///         let mut taskflow = Taskflow::new();
    ///         let s = sum.clone();
    ///         taskflow.emplace(move || {
    ///             s.fetch_add(5, Ordering::Relaxed);
    ///         });
    ///         taskflow
    ///     },
    ///     move || s.load(Ordering::Relaxed) >= 50
    /// ).wait();
    /// ```
    pub fn run_until<F, P>(&mut self, mut factory: F, mut predicate: P) -> TaskflowFuture
    where
        F: FnMut() -> Taskflow,
        P: FnMut() -> bool,
    {
        loop {
            let taskflow = factory();
            self.run(&taskflow).wait();
            if predicate() {
                break;
            }
        }
        
        // Return a completed future (last iteration already waited)
        let taskflow = factory();
        self.run(&taskflow)
    }
    
    /// Run multiple taskflows concurrently
    /// 
    /// Executes all taskflows in parallel and returns a vector of futures.
    /// 
    /// # Example
    /// ```
    /// let mut executor = Executor::new(4);
    /// let flow1 = create_taskflow_1();
    /// let flow2 = create_taskflow_2();
    /// let flow3 = create_taskflow_3();
    /// 
    /// let futures = executor.run_many(&[&flow1, &flow2, &flow3]);
    /// 
    /// // Wait for all to complete
    /// for future in futures {
    ///     future.wait();
    /// }
    /// ```
    pub fn run_many(&mut self, taskflows: &[&Taskflow]) -> Vec<TaskflowFuture> {
        taskflows.iter()
            .map(|tf| self.run(tf))
            .collect()
    }
    
    /// Run multiple taskflows concurrently and wait for all to complete
    /// 
    /// This is a convenience method that runs all taskflows and waits for completion.
    /// 
    /// # Example
    /// ```
    /// let mut executor = Executor::new(4);
    /// executor.run_many_and_wait(&[&flow1, &flow2, &flow3]);
    /// ```
    pub fn run_many_and_wait(&mut self, taskflows: &[&Taskflow]) {
        let futures = self.run_many(taskflows);
        for future in futures {
            future.wait();
        }
    }

    fn worker_loop(
        _worker_id: usize,
        graph: Arc<Mutex<Vec<TaskNode>>>,
        conditional_branches: Arc<Mutex<HashMap<TaskId, HashMap<BranchId, Vec<TaskId>>>>>,
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
                let branch_taken = if let Some(work) = work {
                    match work {
                        TaskWork::Static(func) => {
                            func();
                            None  // Not a conditional task
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
                            None  // Not a conditional task
                        }
                        TaskWork::Condition(func) => {
                            let branch_result = func();
                            Some(branch_result)  // Return which branch was taken
                        }
                        #[cfg(feature = "async")]
                        TaskWork::Async(_) => {
                            panic!("Async tasks require AsyncExecutor. Use AsyncExecutor::new() instead of Executor::new()");
                        }
                    }
                } else {
                    None
                };

                // Decrement tasks remaining
                tasks_remaining.fetch_sub(1, Ordering::Release);

                // Determine which successors to activate
                let successors_to_activate: Vec<TaskId> = if let Some(branch) = branch_taken {
                    // This was a conditional task - check if branches are registered
                    let branches_map = conditional_branches.lock().unwrap();
                    if let Some(task_branches) = branches_map.get(&task_id) {
                        let branch_id = BranchId(branch);
                        
                        // Count how many tasks are in branches that were NOT selected
                        // These tasks will never execute, so we need to decrement tasks_remaining for them
                        let mut unreachable_count = 0;
                        for (other_branch_id, other_successors) in task_branches.iter() {
                            if *other_branch_id != branch_id {
                                // This branch was not selected - count its tasks as unreachable
                                unreachable_count += Self::count_reachable_tasks(&graph, other_successors);
                            }
                        }
                        
                        // Decrement tasks_remaining for unreachable tasks
                        if unreachable_count > 0 {
                            tasks_remaining.fetch_sub(unreachable_count, Ordering::Release);
                        }
                        
                        if let Some(branch_successors) = task_branches.get(&branch_id) {
                            // Use the specific branch successors
                            branch_successors.clone()
                        } else {
                            // Branch not found, use default successors (convert from HashSet)
                            successors.into_iter().collect()
                        }
                    } else {
                        // No branches registered, use default successors (convert from HashSet)
                        successors.into_iter().collect()
                    }
                } else {
                    // Not a conditional task, use all successors (convert from HashSet)
                    successors.into_iter().collect()
                };

                // Update successor dependencies and enqueue ready tasks
                let dep_map = dep_count.lock().unwrap();
                let mut ready_tasks = Vec::new();
                
                for succ_id in successors_to_activate {
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
