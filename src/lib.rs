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

// ── New advanced scheduling modules ──────────────────────────────────────────

/// Preemptive task cancellation (watchdog + signal-based).
/// See [`preemptive::PreemptiveCancellationToken`] and [`preemptive::with_deadline`].
pub mod preemptive;

/// Dynamic priority adjustment for queued tasks.
/// See [`dynamic_priority::SharedDynamicScheduler`] and [`dynamic_priority::PriorityHandle`].
pub mod dynamic_priority;

/// Hardware topology via hwloc (or sysfs fallback).
/// See [`hwloc_topology::TopologyProvider`] and [`hwloc_topology::HwlocWorkerAffinity`].
pub mod hwloc_topology;

// ─────────────────────────────────────────────────────────────────────────────

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

// ── New advanced scheduling re-exports ───────────────────────────────────────
pub use preemptive::{
    PreemptiveCancellationToken,
    DeadlineGuard,
    Preempted,
    with_deadline,
};
pub use dynamic_priority::{
    DynamicPriorityScheduler,
    SharedDynamicScheduler,
    PriorityHandle,
    EscalationPolicy,
};
pub use hwloc_topology::{
    TopologyProvider,
    HwTopology,
    HwlocWorkerAffinity,
    AffinityStrategy,
    CacheInfo,
    PackageInfo,
    BindError,
};

#[cfg(feature = "async")]
pub use async_executor::AsyncExecutor;

// GPU types are always exported (stubs when feature is disabled)
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

        a.precede(&b);
        executor.run(&taskflow).wait();
    }

    #[test]
    fn test_preemptive_cancellation_export() {
        let token = PreemptiveCancellationToken::new();
        assert!(!token.is_cancelled());
        token.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn test_dynamic_priority_export() {
        let sched = SharedDynamicScheduler::new();
        let handle = sched.push(1, Priority::Low);
        handle.reprioritize(Priority::Critical);
        assert_eq!(sched.pop(), Some(1));
    }

    #[test]
    fn test_hwloc_topology_export() {
        let topo = TopologyProvider::detect();
        assert!(topo.cpu_count() >= 1);
    }
}
