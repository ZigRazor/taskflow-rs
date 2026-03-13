// ── New module declarations (add to src/lib.rs) ──────────────────────────────
//
// Add these three lines to the module declarations section:

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
