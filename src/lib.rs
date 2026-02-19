pub mod executor;
pub mod task;
pub mod taskflow;
pub mod subflow;
pub mod future;
pub mod algorithms;
pub mod condition;

pub use executor::Executor;
pub use task::{Task, TaskHandle};
pub use taskflow::Taskflow;
pub use subflow::Subflow;
pub use future::TaskflowFuture;
pub use condition::{ConditionalHandle, BranchId, Loop};

// Re-export parallel algorithms
pub use algorithms::{parallel_for_each, parallel_reduce, parallel_transform, parallel_sort};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_taskflow() {
        let mut executor = Executor::new(4);
        let mut taskflow = Taskflow::new();

        let a = taskflow.emplace(|| {
            println!("Task A");
        }).name("A");

        let b = taskflow.emplace(|| {
            println!("Task B");
        }).name("B");

        let c = taskflow.emplace(|| {
            println!("Task C");
        }).name("C");

        let d = taskflow.emplace(|| {
            println!("Task D");
        }).name("D");

        a.precede(&b);
        a.precede(&c);
        d.succeed(&b);  // d runs after b (b -> d)
        d.succeed(&c);  // d runs after c (c -> d)

        executor.run(&taskflow).wait();
    }
}
