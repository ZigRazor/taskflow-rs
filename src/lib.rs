pub mod advanced;
pub mod algorithms;
pub mod composition;
pub mod condition;
pub mod cycle_detection;
pub mod debug;
pub mod executor;
pub mod future;
pub mod metrics;
pub mod monitoring;
pub mod numa;
pub mod pipeline;
pub mod profiler;
pub mod scheduler;
pub mod subflow;
pub mod task;
pub mod taskflow;
pub mod typed_pipeline;
pub mod visualization;

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

pub use advanced::{CancellationToken, Priority, TaskMetadata};
pub use composition::{
    CloneableWork, ComposedInstance, Composition, CompositionBuilder, CompositionFactory,
    CompositionParams, ParamValue, ParameterizedComposition, TaskflowComposable,
};
pub use condition::{BranchId, ConditionalHandle, Loop};
pub use cycle_detection::{CycleDetectionResult, CycleDetector};
pub use debug::{DebugLogger, LogEntry, LogLevel};
pub use executor::Executor;
pub use future::TaskflowFuture;
pub use metrics::{Metrics, MetricsSummary};
pub use monitoring::PerformanceMetrics;
pub use numa::{NumaNode, NumaPinning, NumaTopology};
pub use pipeline::{ConcurrentPipeline, StageType, Token};
pub use profiler::{ExecutionProfile, Profiler, TaskStats};
pub use scheduler::{FifoScheduler, PriorityScheduler, RoundRobinScheduler, Scheduler};
pub use subflow::Subflow;
pub use task::{Task, TaskHandle};
pub use taskflow::Taskflow;
pub use typed_pipeline::{PipelineBuilder, SimplePipeline, TypeSafePipeline};
pub use visualization::{
    generate_dot_graph, generate_html_report, generate_timeline_svg, save_dot_graph,
    save_html_report, save_timeline_svg,
};

// ── New advanced scheduling re-exports ───────────────────────────────────────
pub use dynamic_priority::{
    DynamicPriorityScheduler, EscalationPolicy, PriorityHandle, SharedDynamicScheduler,
};
pub use hwloc_topology::{
    AffinityStrategy, BindError, CacheInfo, HwTopology, HwlocWorkerAffinity, PackageInfo,
    TopologyProvider,
};
pub use preemptive::{with_deadline, DeadlineGuard, Preempted, PreemptiveCancellationToken};

#[cfg(feature = "async")]
pub use async_executor::AsyncExecutor;

// GPU types are always exported (stubs when feature is disabled)
pub use gpu::{GpuBuffer, GpuDevice, GpuTaskConfig};
pub use gpu_backend::{BackendKind, GpuError};
pub use gpu_stream::{GpuStream, StreamAssignment, StreamGuard, StreamPool, StreamSet};

// Re-export parallel algorithms
pub use algorithms::{
    parallel_exclusive_scan, parallel_for_each, parallel_inclusive_scan, parallel_reduce,
    parallel_sort, parallel_transform,
};

pub mod dashboard;
pub mod flamegraph;
pub mod regression;

// ── New pub use re-exports (add to the pub use section) ──────────────────────

pub use dashboard::{DashboardConfig, DashboardHandle, DashboardServer};
pub use flamegraph::{FlamegraphConfig, FlamegraphGenerator};
pub use regression::{
    Baseline, MetricComparison, RegressionDetector, RegressionReport, RegressionThresholds,
    Severity, Violation,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_taskflow() {
        let mut executor = Executor::new(4);
        let mut taskflow = Taskflow::new();

        let a = taskflow
            .emplace(|| {
                println!("Task A");
            })
            .name("A");

        let b = taskflow
            .emplace(|| {
                println!("Task B");
            })
            .name("B");

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
