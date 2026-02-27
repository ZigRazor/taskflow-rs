pub mod executor;
pub mod task;
pub mod taskflow;
pub mod subflow;
pub mod future;
pub mod algorithms;
pub mod condition;
pub mod pipeline;
pub mod composition;
pub mod advanced;
pub mod scheduler;
pub mod numa;
pub mod profiler;
pub mod visualization;
pub mod monitoring;
pub mod debug;

#[cfg(feature = "async")]
pub mod async_executor;

// GPU module - always exists but contents depend on feature flag
pub mod gpu;


pub use executor::Executor;
pub use task::{Task, TaskHandle};
pub use taskflow::Taskflow;
pub use subflow::Subflow;
pub use future::TaskflowFuture;
pub use condition::{ConditionalHandle, BranchId, Loop};
pub use pipeline::{ConcurrentPipeline, Token, StageType};
pub use composition::{Composition, CompositionBuilder, ComposedInstance, TaskflowComposable};
pub use advanced::{Priority, CancellationToken, TaskMetadata};
pub use scheduler::{Scheduler, FifoScheduler, PriorityScheduler, RoundRobinScheduler};
pub use numa::{NumaTopology, NumaNode, NumaPinning};
pub use profiler::{Profiler, ExecutionProfile, TaskStats};
pub use visualization::{
    generate_dot_graph, save_dot_graph,
    generate_timeline_svg, save_timeline_svg,
    generate_html_report, save_html_report
};
pub use monitoring::PerformanceMetrics;
pub use debug::{DebugLogger, LogLevel, LogEntry};

#[cfg(feature = "async")]
pub use async_executor::AsyncExecutor;

// GPU types are always exported (stubs when feature is disabled)
pub use gpu::{GpuDevice, GpuBuffer, GpuTaskConfig};

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
    
    #[test]
    fn test_pipeline_exports() {
        // Verify types are exported
        let _pipeline: ConcurrentPipeline<i32> = ConcurrentPipeline::new(10, 100);
        let _token: Token<i32> = Token::new(42, 0);
        let _stage_type: StageType = StageType::Serial;
    }
}
