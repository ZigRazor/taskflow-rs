pub mod executor;
pub mod task;
pub mod taskflow;
pub mod subflow;
pub mod future;
pub mod algorithms;
pub mod condition;
pub mod cycle_detection;
pub mod pipeline;
pub mod typed_pipeline;
pub mod composition;
pub mod advanced;
pub mod scheduler;
pub mod numa;
pub mod profiler;
pub mod visualization;
pub mod monitoring;
pub mod metrics;
pub mod debug;

#[cfg(feature = "async")]
pub mod async_executor;

// GPU module - always exists but contents depend on feature flag
pub mod gpu;

// GPU backend abstraction (trait + probe_backend + BackendKind/GpuError + stub)
pub mod gpu_backend;

// Stream management (GpuStream, StreamPool, StreamSet, StreamGuard)
pub mod gpu_stream;

// Backend implementations — compiled only when the matching feature is active
#[cfg(feature = "gpu")]
pub mod gpu_cuda_backend;

#[cfg(feature = "opencl")]
pub mod gpu_opencl;

#[cfg(feature = "rocm")]
pub mod gpu_rocm;


pub use executor::Executor;
pub use task::{Task, TaskHandle};
pub use taskflow::Taskflow;
pub use subflow::Subflow;
pub use future::TaskflowFuture;
pub use condition::{ConditionalHandle, BranchId, Loop};
pub use cycle_detection::{CycleDetector, CycleDetectionResult};
pub use pipeline::{ConcurrentPipeline, Token, StageType};
pub use composition::{
    Composition, CompositionBuilder, ComposedInstance, TaskflowComposable,
    CloneableWork, CompositionParams, ParamValue, ParameterizedComposition, CompositionFactory
};
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
pub use metrics::{Metrics, MetricsSummary};
pub use typed_pipeline::{TypeSafePipeline, PipelineBuilder, SimplePipeline};
pub use debug::{DebugLogger, LogLevel, LogEntry};

#[cfg(feature = "async")]
pub use async_executor::AsyncExecutor;

// GPU types are always exported (stubs when feature is disabled)
// NEW
pub use gpu::{GpuDevice, GpuBuffer, GpuTaskConfig};
pub use gpu_stream::{GpuStream, StreamPool, StreamSet, StreamGuard, StreamAssignment};
pub use gpu_backend::{BackendKind, GpuError};

// Re-export parallel algorithms
pub use algorithms::{
    parallel_for_each, 
    parallel_reduce, 
    parallel_transform, 
    parallel_sort,
    parallel_inclusive_scan,
    parallel_exclusive_scan,
};

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
