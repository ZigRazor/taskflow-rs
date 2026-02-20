#[cfg(feature = "async")]
use tokio::runtime::Handle;
#[cfg(feature = "async")]
use std::sync::{Arc, Mutex};
#[cfg(feature = "async")]
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(feature = "async")]
use std::collections::{HashMap, VecDeque};
#[cfg(feature = "async")]
use crate::taskflow::Taskflow;
#[cfg(feature = "async")]
use crate::task::{TaskWork, TaskId, TaskNode};
#[cfg(feature = "async")]
use crate::condition::BranchId;
#[cfg(feature = "async")]
use crate::subflow::Subflow;

/// Async executor for running taskflows with async tasks
#[cfg(feature = "async")]
pub struct AsyncExecutor {
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
    
    /// Run a taskflow asynchronously
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
        let mut ready_queue = VecDeque::new();
        {
            let graph_guard = graph.lock().unwrap();
            for node in graph_guard.iter() {
                if node.dependents.is_empty() {
                    ready_queue.push_back(node.id);
                }
            }
        }
        
        // Process tasks
        while let Some(task_id) = ready_queue.pop_front() {
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
                                    ready_queue.push_back(node.id);
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
                        ready_queue.push_back(succ_id);
                    }
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
        
        let sync_task = taskflow.emplace(|| {
            println!("Sync task");
        }).name("sync");
        
        let async_task = taskflow.emplace_async(|| async {
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            println!("Async task");
        }).name("async");
        
        sync_task.precede(&async_task);
        
        executor.run(&taskflow);
    }
}
