#[cfg(feature = "async")]
use crate::condition::BranchId;
#[cfg(feature = "async")]
use crate::subflow::Subflow;
#[cfg(feature = "async")]
use crate::task::{TaskId, TaskNode, TaskWork};
#[cfg(feature = "async")]
use crate::taskflow::Taskflow;
#[cfg(feature = "async")]
use std::collections::{HashMap, VecDeque};
#[cfg(feature = "async")]
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(feature = "async")]
use std::sync::{Arc, Mutex};
#[cfg(feature = "async")]
use tokio::runtime::Handle;

/// Async executor for running taskflows with async tasks
#[cfg(feature = "async")]
pub struct AsyncExecutor {
    #[allow(dead_code)]
    handle: Handle,
}

#[cfg(feature = "async")]
impl AsyncExecutor {
    /// Create a new async executor that uses the current Tokio runtime
    ///
    /// # Panics
    ///
    /// Panics if called outside of a Tokio runtime context
    pub fn new(_num_workers: usize) -> Self {
        let handle = Handle::current();
        Self { handle }
    }

    /// Create a new async executor with its own runtime
    /// Use this when you're NOT already in an async context
    pub fn new_with_runtime(num_workers: usize) -> Self {
        let num_workers = if num_workers == 0 {
            num_cpus::get()
        } else {
            num_workers
        };

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(num_workers)
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime");

        let handle = runtime.handle().clone();

        // Leak the runtime to avoid drop issues
        // This is acceptable since this is meant for app-lifetime executors
        std::mem::forget(runtime);

        Self { handle }
    }

    /// Run a taskflow asynchronously with parallel execution
    pub async fn run_async(&self, taskflow: &Taskflow) {
        let graph = taskflow.get_graph();
        let conditional_branches = taskflow.get_conditional_branches();

        // Build dependency tracking
        let dep_count = Arc::new(Mutex::new(HashMap::new()));
        let tasks_remaining = Arc::new(AtomicUsize::new(0));
        let completed_tasks = Arc::new(Mutex::new(Vec::new()));

        {
            let graph_guard = graph.lock().unwrap();
            let mut dep_map = dep_count.lock().unwrap();

            for node in graph_guard.iter() {
                dep_map.insert(node.id, AtomicUsize::new(node.dependents.len()));
            }

            tasks_remaining.store(graph_guard.len(), Ordering::Release);
        }

        // Find initial tasks (no dependencies)
        let ready_queue = Arc::new(Mutex::new(VecDeque::new()));
        {
            let graph_guard = graph.lock().unwrap();
            for node in graph_guard.iter() {
                if node.dependents.is_empty() {
                    ready_queue.lock().unwrap().push_back(node.id);
                }
            }
        }

        // Use JoinSet for parallel execution
        let mut join_set = tokio::task::JoinSet::new();

        // Process tasks in parallel
        loop {
            // Spawn all ready tasks
            while let Some(task_id) = ready_queue.lock().unwrap().pop_front() {
                let graph_clone = Arc::clone(&graph);
                let conditional_branches_clone = Arc::clone(&conditional_branches);
                let dep_count_clone = Arc::clone(&dep_count);
                let ready_queue_clone = Arc::clone(&ready_queue);
                let tasks_remaining_clone = Arc::clone(&tasks_remaining);
                let completed_tasks_clone = Arc::clone(&completed_tasks);

                join_set.spawn(async move {
                    Self::execute_task(
                        task_id,
                        graph_clone,
                        conditional_branches_clone,
                        dep_count_clone,
                        ready_queue_clone,
                        tasks_remaining_clone,
                        completed_tasks_clone,
                    )
                    .await
                });
            }

            // Wait for at least one task to complete
            if let Some(result) = join_set.join_next().await {
                if let Err(e) = result {
                    eprintln!("Task execution error: {:?}", e);
                }

                // Check if we're done
                if tasks_remaining.load(Ordering::Acquire) == 0 {
                    break;
                }
            } else {
                // No more tasks in join set
                break;
            }
        }

        // Wait for any remaining tasks
        while join_set.join_next().await.is_some() {}
    }

    /// Execute a single task and update dependencies
    async fn execute_task(
        task_id: TaskId,
        graph: Arc<Mutex<Vec<TaskNode>>>,
        conditional_branches: Arc<Mutex<HashMap<TaskId, HashMap<BranchId, Vec<TaskId>>>>>,
        dep_count: Arc<Mutex<HashMap<TaskId, AtomicUsize>>>,
        ready_queue: Arc<Mutex<VecDeque<TaskId>>>,
        tasks_remaining: Arc<AtomicUsize>,
        completed_tasks: Arc<Mutex<Vec<TaskId>>>,
    ) {
        // Get task work and successors
        let (work, successors) = {
            let mut graph_guard = graph.lock().unwrap();
            let node = graph_guard.iter_mut().find(|n| n.id == task_id).unwrap();
            let work = node.work.take();
            let successors = node.successors.clone();
            (work, successors)
        };

        // Execute work
        let branch_taken = if let Some(work) = work {
            match work {
                TaskWork::Static(func) => {
                    func();
                    None
                }
                TaskWork::Async(func) => {
                    let future = func();
                    future.await;
                    None
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

                        for i in initial_graph_size..(initial_graph_size + new_tasks) {
                            let node = &graph_guard[i];
                            dep_map.insert(node.id, AtomicUsize::new(node.dependents.len()));

                            if node.dependents.is_empty() {
                                ready_queue.lock().unwrap().push_back(node.id);
                            }
                        }
                    }
                    None
                }
                TaskWork::Condition(func) => {
                    let branch_result = func();
                    Some(branch_result)
                }
            }
        } else {
            None
        };

        // Mark task as completed
        completed_tasks.lock().unwrap().push(task_id);
        tasks_remaining.fetch_sub(1, Ordering::Release);

        // Handle conditional branching
        let successors_to_activate: Vec<TaskId> = if let Some(branch) = branch_taken {
            let branches_map = conditional_branches.lock().unwrap();
            if let Some(task_branches) = branches_map.get(&task_id) {
                let branch_id = BranchId(branch);

                // Count unreachable tasks
                let mut unreachable_count = 0;
                for (other_branch_id, other_successors) in task_branches.iter() {
                    if *other_branch_id != branch_id {
                        unreachable_count += Self::count_reachable_tasks(&graph, other_successors);
                    }
                }

                if unreachable_count > 0 {
                    tasks_remaining.fetch_sub(unreachable_count, Ordering::Release);
                }

                if let Some(branch_successors) = task_branches.get(&branch_id) {
                    branch_successors.clone()
                } else {
                    successors.into_iter().collect()
                }
            } else {
                successors.into_iter().collect()
            }
        } else {
            successors.into_iter().collect()
        };

        // Update successor dependencies
        let dep_map = dep_count.lock().unwrap();
        for succ_id in successors_to_activate {
            if let Some(count) = dep_map.get(&succ_id) {
                let prev = count.fetch_sub(1, Ordering::AcqRel);
                if prev == 1 {
                    ready_queue.lock().unwrap().push_back(succ_id);
                }
            }
        }
    }

    /// Run a taskflow and block until completion
    ///
    /// This creates a temporary runtime to block on the async execution.
    /// If you're already in an async context, use `run_async().await` instead.
    pub fn run(&self, taskflow: &Taskflow) {
        // Create a temporary runtime for blocking
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create temporary runtime");

        rt.block_on(self.run_async(taskflow));
    }

    /// Run a taskflow asynchronously and return when complete
    ///
    /// This is an alias for `run_async` for consistency with the sync executor API
    pub async fn run_await(&self, taskflow: &Taskflow) {
        self.run_async(taskflow).await
    }

    /// Run N instances of a taskflow concurrently (async version)
    ///
    /// Creates N taskflows and runs them in parallel asynchronously.
    /// All instances execute concurrently using the async executor.
    ///
    /// # Example
    /// ```
    /// use taskflow_rs::{AsyncExecutor, Taskflow};
    /// use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let executor = AsyncExecutor::new(4);
    ///     let counter = Arc::new(AtomicUsize::new(0));
    ///     
    ///     executor.run_n_async(3, || {
    ///         let c = counter.clone();
    ///         let mut taskflow = Taskflow::new();
    ///         taskflow.emplace_async(move || async move {
    ///             c.fetch_add(1, Ordering::Relaxed);
    ///         });
    ///         taskflow
    ///     }).await;
    /// }
    /// ```
    #[cfg(feature = "async")]
    pub async fn run_n_async<F>(&self, n: usize, factory: F)
    where
        F: Fn() -> Taskflow + Send + Sync,
    {
        if n == 0 {
            return self.run_async(&factory()).await;
        }

        if n == 1 {
            return self.run_async(&factory()).await;
        }

        // Create N taskflows and run them concurrently
        let mut handles = tokio::task::JoinSet::new();

        for _ in 0..n {
            let taskflow = factory();
            let handle_clone = self.handle.clone();

            handles.spawn(async move {
                let executor = AsyncExecutor {
                    handle: handle_clone,
                };
                executor.run_async(&taskflow).await;
            });
        }

        // Wait for all instances to complete
        while handles.join_next().await.is_some() {}
    }

    /// Run a taskflow repeatedly until a condition is met (async version)
    ///
    /// Executes the taskflow repeatedly, checking the predicate after each execution.
    /// Stops when the predicate returns true.
    ///
    /// # Example
    /// ```
    /// use taskflow_rs::{AsyncExecutor, Taskflow};
    /// use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let executor = AsyncExecutor::new(4);
    ///     let counter = Arc::new(AtomicUsize::new(0));
    ///     
    ///     let c = counter.clone();
    ///     executor.run_until_async(
    ///         || {
    ///             let c2 = c.clone();
    ///             let mut taskflow = Taskflow::new();
    ///             taskflow.emplace_async(move || async move {
    ///                 c2.fetch_add(1, Ordering::Relaxed);
    ///             });
    ///             taskflow
    ///         },
    ///         || counter.load(Ordering::Relaxed) >= 5
    ///     ).await;
    ///     
    ///     assert_eq!(counter.load(Ordering::Relaxed), 5);
    /// }
    /// ```
    #[cfg(feature = "async")]
    pub async fn run_until_async<F, P>(&self, factory: F, mut predicate: P)
    where
        F: Fn() -> Taskflow,
        P: FnMut() -> bool,
    {
        loop {
            let taskflow = factory();
            self.run_async(&taskflow).await;

            if predicate() {
                break;
            }
        }
    }

    /// Run a taskflow N times sequentially (async version)
    ///
    /// Executes the taskflow N times, one after another.
    ///
    /// # Example
    /// ```
    /// let executor = AsyncExecutor::new(4);
    /// executor.run_n_sequential_async(3, || {
    ///     let mut taskflow = Taskflow::new();
    ///     taskflow.emplace_async(|| async {
    ///         println!("Task");
    ///     });
    ///     taskflow
    /// }).await;
    /// ```
    #[cfg(feature = "async")]
    pub async fn run_n_sequential_async<F>(&self, n: usize, factory: F)
    where
        F: Fn() -> Taskflow,
    {
        for _ in 0..n {
            let taskflow = factory();
            self.run_async(&taskflow).await;
        }
    }

    /// Count all tasks reachable from a set of starting tasks
    fn count_reachable_tasks(graph: &Arc<Mutex<Vec<TaskNode>>>, start_tasks: &[TaskId]) -> usize {
        let graph_guard = graph.lock().unwrap();
        let mut visited = std::collections::HashSet::new();
        let mut to_visit: Vec<TaskId> = start_tasks.to_vec();

        while let Some(task_id) = to_visit.pop() {
            if visited.insert(task_id) {
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
}

#[cfg(test)]
#[cfg(feature = "async")]
mod tests {
    use super::*;
    use crate::Taskflow;

    #[tokio::test]
    async fn test_async_task() {
        let executor = AsyncExecutor::new(4);
        let mut taskflow = Taskflow::new();

        let task = taskflow.emplace_async(|| async {
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            println!("Async task completed");
        });

        task.name("async_task");

        executor.run(&taskflow);
    }

    #[tokio::test]
    async fn test_mixed_sync_async() {
        let executor = AsyncExecutor::new(4);
        let mut taskflow = Taskflow::new();

        let sync_task = taskflow
            .emplace(|| {
                println!("Sync task");
            })
            .name("sync");

        let async_task = taskflow
            .emplace_async(|| async {
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                println!("Async task");
            })
            .name("async");

        sync_task.precede(&async_task);

        executor.run(&taskflow);
    }
}
