// src/regression.rs
//
// Automated performance regression detection.

use crate::profiler::ExecutionProfile;
use std::fmt;
use std::io::{self, Read, Write};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// ──────────────────────────────────────────────────────────────────────────────
// Baseline
// ──────────────────────────────────────────────────────────────────────────────

/// A serialisable snapshot of known-good performance characteristics.
#[derive(Debug, Clone, PartialEq)]
pub struct Baseline {
    pub label: String,
    pub recorded_at_ms: u128,
    pub total_duration_us: u64,
    pub avg_task_us: u64,
    pub p50_us: u64,
    pub p95_us: u64,
    pub p99_us: u64,
    pub parallelism_efficiency: f64,
    pub steal_rate: f64,
    pub task_count: usize,
}

impl Baseline {
    /// Derive a baseline from an [`ExecutionProfile`].
    pub fn from_profile(profile: &ExecutionProfile, label: impl Into<String>) -> Self {
        let durations_us: Vec<u64> = profile
            .task_stats
            .iter()
            .map(|t| t.duration.as_micros() as u64)
            .collect();

        let avg = if durations_us.is_empty() { 0 } else {
            durations_us.iter().sum::<u64>() / durations_us.len() as u64
        };

        Self {
            label: label.into(),
            recorded_at_ms: unix_now_ms(),
            total_duration_us: profile.total_duration.as_micros() as u64,
            avg_task_us: avg,
            p50_us: percentile_us(&durations_us, 50),
            p95_us: percentile_us(&durations_us, 95),
            p99_us: percentile_us(&durations_us, 99),
            parallelism_efficiency: profile.parallelism_efficiency(),
            steal_rate: 0.0,
            task_count: profile.task_stats.len(),
        }
    }

    /// Serialise to a JSON string (no external deps).
    pub fn to_json(&self) -> String {
        format!(
            "{{\n  \"version\": 1,\n  \"label\": {label},\n  \"recorded_at_ms\": {ts},\n  \
             \"total_duration_us\": {total},\n  \"avg_task_us\": {avg},\n  \
             \"p50_us\": {p50},\n  \"p95_us\": {p95},\n  \"p99_us\": {p99},\n  \
             \"parallelism_efficiency\": {pe:.6},\n  \"steal_rate\": {sr:.6},\n  \
             \"task_count\": {tc}\n}}",
            label = json_string(&self.label),
            ts    = self.recorded_at_ms,
            total = self.total_duration_us,
            avg   = self.avg_task_us,
            p50   = self.p50_us,
            p95   = self.p95_us,
            p99   = self.p99_us,
            pe    = self.parallelism_efficiency,
            sr    = self.steal_rate,
            tc    = self.task_count,
        )
    }

    /// Deserialise from a JSON string.
    pub fn from_json(s: &str) -> Result<Self, String> {
        let get_str = |key: &str| -> Option<&str> {
            let needle = format!("\"{}\":", key);
            let start  = s.find(&needle)? + needle.len();
            let s      = s[start..].trim_start_matches(|c: char| c.is_whitespace());
            let end    = s.find(|c: char| c == ',' || c == '}' || c == '\n').unwrap_or(s.len());
            Some(s[..end].trim())
        };

        let u64_val = |key: &str| -> Result<u64, String> {
            get_str(key)
                .ok_or_else(|| format!("missing key: {}", key))?
                .trim_matches('"')
                .parse::<u64>()
                .map_err(|e| format!("parse error for {}: {}", key, e))
        };
        let f64_val = |key: &str| -> Result<f64, String> {
            get_str(key)
                .ok_or_else(|| format!("missing key: {}", key))?
                .trim_matches('"')
                .parse::<f64>()
                .map_err(|e| format!("parse error for {}: {}", key, e))
        };
        let usize_val = |key: &str| -> Result<usize, String> {
            get_str(key)
                .ok_or_else(|| format!("missing key: {}", key))?
                .trim_matches('"')
                .parse::<usize>()
                .map_err(|e| format!("parse error for {}: {}", key, e))
        };

        let label = get_str("label")
            .ok_or("missing key: label")?
            .trim_matches('"')
            .to_string();

        let recorded_at_ms = get_str("recorded_at_ms")
            .ok_or("missing key: recorded_at_ms")?
            .parse::<u128>()
            .map_err(|e| format!("parse error for recorded_at_ms: {}", e))?;

        Ok(Self {
            label,
            recorded_at_ms,
            total_duration_us:      u64_val("total_duration_us")?,
            avg_task_us:            u64_val("avg_task_us")?,
            p50_us:                 u64_val("p50_us")?,
            p95_us:                 u64_val("p95_us")?,
            p99_us:                 u64_val("p99_us")?,
            parallelism_efficiency: f64_val("parallelism_efficiency")?,
            steal_rate:             f64_val("steal_rate")?,
            task_count:             usize_val("task_count")?,
        })
    }

    /// Save this baseline to a file.
    pub fn save(&self, path: &str) -> io::Result<()> {
        let json = self.to_json();
        std::fs::File::create(path)?.write_all(json.as_bytes())
    }

    /// Load a baseline from a file.
    pub fn load(path: &str) -> io::Result<Self> {
        let mut buf = String::new();
        std::fs::File::open(path)?.read_to_string(&mut buf)?;
        Baseline::from_json(&buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Thresholds
// ──────────────────────────────────────────────────────────────────────────────

/// Configurable regression thresholds (fractional values; e.g. 0.10 = 10%).
#[derive(Debug, Clone)]
pub struct RegressionThresholds {
    pub total_duration:         f64,
    pub avg_task_duration:      f64,
    pub p50_duration:           f64,
    pub p95_duration:           f64,
    pub p99_duration:           f64,
    pub parallelism_efficiency: f64,
    pub steal_rate:             f64,
}

impl Default for RegressionThresholds {
    fn default() -> Self {
        Self {
            total_duration:         0.10,
            avg_task_duration:      0.10,
            p50_duration:           0.10,
            p95_duration:           0.15,
            p99_duration:           0.20,
            parallelism_efficiency: 0.05,
            steal_rate:             0.50,
        }
    }
}

impl RegressionThresholds {
    pub fn strict() -> Self {
        Self {
            total_duration:         0.05,
            avg_task_duration:      0.05,
            p50_duration:           0.05,
            p95_duration:           0.08,
            p99_duration:           0.10,
            parallelism_efficiency: 0.03,
            steal_rate:             0.25,
        }
    }

    pub fn lenient() -> Self {
        Self {
            total_duration:         0.20,
            avg_task_duration:      0.20,
            p50_duration:           0.20,
            p95_duration:           0.30,
            p99_duration:           0.40,
            parallelism_efficiency: 0.10,
            steal_rate:             1.00,
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Violation
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    Warning,
    Critical,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Warning  => write!(f, "WARNING"),
            Severity::Critical => write!(f, "CRITICAL"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Violation {
    pub severity:        Severity,
    pub metric:          String,
    pub baseline_value:  f64,
    pub current_value:   f64,
    pub relative_change: f64,
    pub threshold:       f64,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {}: {:.2} -> {:.2}  ({:+.1}% vs threshold {:.1}%)",
            self.severity,
            self.metric,
            self.baseline_value,
            self.current_value,
            self.relative_change * 100.0,
            self.threshold * 100.0,
        )
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// MetricComparison
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MetricComparison {
    pub name:            String,
    pub baseline:        f64,
    pub current:         f64,
    pub relative_change: f64,
    pub improved:        bool,
}

impl MetricComparison {
    fn new(name: &str, baseline: f64, current: f64, higher_is_better: bool) -> Self {
        let rel = if baseline == 0.0 { 0.0 } else { (current - baseline) / baseline };
        let worse_delta = if higher_is_better { -rel } else { rel };
        MetricComparison {
            name: name.to_string(),
            baseline,
            current,
            relative_change: worse_delta,
            improved: worse_delta < 0.0,
        }
    }
}

impl fmt::Display for MetricComparison {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let arrow = if self.improved { "v" } else { "^" };
        write!(
            f,
            "  {:.<40} {:>10.2}  ->  {:>10.2}  {} {:+.1}%",
            self.name,
            self.baseline,
            self.current,
            arrow,
            self.relative_change * 100.0,
        )
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// RegressionReport
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RegressionReport {
    pub passed:               bool,
    pub comparisons:          Vec<MetricComparison>,
    pub violations:           Vec<Violation>,
    pub baseline_label:       String,
    pub baseline_task_count:  usize,
    pub current_task_count:   usize,
}

impl RegressionReport {
    pub fn summary(&self) -> String {
        if self.passed {
            format!("OK No regressions detected (baseline: {})", self.baseline_label)
        } else {
            let crits = self.violations.iter().filter(|v| v.severity == Severity::Critical).count();
            let warns = self.violations.iter().filter(|v| v.severity == Severity::Warning).count();
            format!(
                "FAIL {} regression(s) detected [{} critical, {} warning] (baseline: {})",
                self.violations.len(), crits, warns, self.baseline_label
            )
        }
    }

    pub fn report(&self) -> String {
        let mut out = String::new();
        out.push_str("=== PERFORMANCE REGRESSION REPORT ===\n\n");
        out.push_str(&format!(
            "  Baseline : {}\n  Tasks    : {} (baseline) vs {} (current)\n\n",
            self.baseline_label, self.baseline_task_count, self.current_task_count
        ));
        out.push_str("  Metric Comparisons\n");
        out.push_str("  ---------------------------------------------------------------\n");
        for cmp in &self.comparisons {
            out.push_str(&format!("{}\n", cmp));
        }
        if self.violations.is_empty() {
            out.push_str("\n  All metrics within thresholds.\n");
        } else {
            out.push_str("\n  Violations\n");
            out.push_str("  ---------------------------------------------------------------\n");
            for v in &self.violations {
                out.push_str(&format!("  {}\n", v));
            }
        }
        out.push_str(&format!("\n  Result: {}\n", self.summary()));
        out
    }

    pub fn to_json(&self) -> String {
        let violations_json: Vec<String> = self.violations.iter().map(|v| {
            format!(
                "{{\"severity\":{sev},\"metric\":{met},\"baseline\":{bl:.4},\"current\":{cur:.4},\"change_pct\":{chg:.2}}}",
                sev = json_string(&v.severity.to_string()),
                met = json_string(&v.metric),
                bl  = v.baseline_value,
                cur = v.current_value,
                chg = v.relative_change * 100.0,
            )
        }).collect();

        format!(
            "{{\n  \"passed\": {},\n  \"baseline_label\": {},\n  \
             \"baseline_task_count\": {},\n  \"current_task_count\": {},\n  \
             \"violations\": [{}]\n}}",
            self.passed,
            json_string(&self.baseline_label),
            self.baseline_task_count,
            self.current_task_count,
            violations_json.join(","),
        )
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// RegressionDetector
// ──────────────────────────────────────────────────────────────────────────────

pub struct RegressionDetector {
    baseline:   Baseline,
    thresholds: RegressionThresholds,
}

impl RegressionDetector {
    pub fn new(baseline: Baseline, thresholds: RegressionThresholds) -> Self {
        Self { baseline, thresholds }
    }

    pub fn detect(&self, current: &ExecutionProfile) -> RegressionReport {
        let bl = &self.baseline;
        let th = &self.thresholds;

        let durations_us: Vec<u64> = current.task_stats
            .iter()
            .map(|t| t.duration.as_micros() as u64)
            .collect();

        let avg = if durations_us.is_empty() { 0 } else {
            durations_us.iter().sum::<u64>() / durations_us.len() as u64
        };

        let cur_total   = current.total_duration.as_micros() as u64;
        let cur_p50     = percentile_us(&durations_us, 50);
        let cur_p95     = percentile_us(&durations_us, 95);
        let cur_p99     = percentile_us(&durations_us, 99);
        let cur_par_eff = current.parallelism_efficiency();

        let comparisons = vec![
            MetricComparison::new("Total Duration (us)",      bl.total_duration_us as f64,      cur_total   as f64, false),
            MetricComparison::new("Avg Task Duration (us)",   bl.avg_task_us        as f64,      avg         as f64, false),
            MetricComparison::new("P50 Task Duration (us)",   bl.p50_us             as f64,      cur_p50     as f64, false),
            MetricComparison::new("P95 Task Duration (us)",   bl.p95_us             as f64,      cur_p95     as f64, false),
            MetricComparison::new("P99 Task Duration (us)",   bl.p99_us             as f64,      cur_p99     as f64, false),
            MetricComparison::new("Parallelism Efficiency",   bl.parallelism_efficiency,         cur_par_eff,        true),
        ];

        let mut violations: Vec<Violation> = Vec::new();
        let thresholds_list = [
            th.total_duration,
            th.avg_task_duration,
            th.p50_duration,
            th.p95_duration,
            th.p99_duration,
            th.parallelism_efficiency,
        ];

        for (cmp, &threshold) in comparisons.iter().zip(thresholds_list.iter()) {
            if cmp.relative_change > threshold {
                let severity = if cmp.relative_change > threshold * 2.0 {
                    Severity::Critical
                } else {
                    Severity::Warning
                };
                violations.push(Violation {
                    severity,
                    metric:          cmp.name.clone(),
                    baseline_value:  cmp.baseline,
                    current_value:   cmp.current,
                    relative_change: cmp.relative_change,
                    threshold,
                });
            }
        }

        RegressionReport {
            passed: violations.is_empty(),
            comparisons,
            violations,
            baseline_label:      bl.label.clone(),
            baseline_task_count: bl.task_count,
            current_task_count:  current.task_stats.len(),
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

fn percentile_us(durations: &[u64], pct: u8) -> u64 {
    if durations.is_empty() { return 0; }
    let mut sorted = durations.to_vec();
    sorted.sort_unstable();
    let idx = ((pct as usize) * sorted.len()) / 100;
    sorted[idx.min(sorted.len() - 1)]
}

fn unix_now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis()
}

fn json_string(s: &str) -> String {
    format!(
        "\"{}\"",
        s.replace('\\', "\\\\")
         .replace('"',  "\\\"")
         .replace('\n', "\\n")
         .replace('\r', "\\r")
    )
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiler::{ExecutionProfile, TaskStats};
    use std::time::{Duration, Instant};

    fn profile_with_durations(durations_ms: &[u64], num_workers: usize) -> ExecutionProfile {
        let start = Instant::now();
        let total = Duration::from_millis(durations_ms.iter().sum());
        ExecutionProfile {
            start_time: start,
            total_duration: total,
            task_stats: durations_ms
                .iter()
                .enumerate()
                .map(|(i, &d)| TaskStats {
                    task_id: (i + 1) as u64,
                    name: Some(format!("task_{}", i)),
                    start_time: start,
                    duration: Duration::from_millis(d),
                    worker_id: i % num_workers,
                    num_dependencies: 0,
                })
                .collect(),
            num_workers,
        }
    }

    #[test]
    fn no_regression_on_identical() {
        let profile  = profile_with_durations(&[100, 200, 150, 50], 2);
        let baseline = Baseline::from_profile(&profile, "test");
        let detector = RegressionDetector::new(baseline, RegressionThresholds::default());
        let report   = detector.detect(&profile);
        assert!(report.passed, "identical profile should pass: {:?}", report.violations);
    }

    #[test]
    fn detects_total_duration_regression() {
        let bl_profile = profile_with_durations(&[100, 100, 100, 100], 2);
        let rg_profile = profile_with_durations(&[200, 200, 200, 200], 2);
        let baseline   = Baseline::from_profile(&bl_profile, "baseline");
        let detector   = RegressionDetector::new(baseline, RegressionThresholds::default());
        let report     = detector.detect(&rg_profile);
        assert!(!report.passed);
        assert!(
            report.violations.iter().any(|v| v.metric.contains("Total Duration")),
            "expected total duration violation"
        );
    }

    #[test]
    fn improvement_does_not_fail() {
        let bl_profile = profile_with_durations(&[200, 200, 200, 200], 2);
        let im_profile = profile_with_durations(&[100, 100, 100, 100], 2);
        let baseline   = Baseline::from_profile(&bl_profile, "slow");
        let detector   = RegressionDetector::new(baseline, RegressionThresholds::default());
        assert!(detector.detect(&im_profile).passed, "improvement should not flag violations");
    }

    #[test]
    fn critical_severity_at_2x_threshold() {
        let bl_profile = profile_with_durations(&[100], 1);
        let rg_profile = profile_with_durations(&[400], 1); // 300% regression
        let baseline   = Baseline::from_profile(&bl_profile, "bl");
        let detector   = RegressionDetector::new(baseline, RegressionThresholds::default());
        let report     = detector.detect(&rg_profile);
        assert!(!report.passed);
        let v = report.violations.iter().find(|v| v.metric.contains("Total")).unwrap();
        assert_eq!(v.severity, Severity::Critical);
    }

    #[test]
    fn baseline_json_roundtrip() {
        let profile  = profile_with_durations(&[100, 200, 300], 2);
        let bl       = Baseline::from_profile(&profile, "roundtrip-test");
        let json     = bl.to_json();
        let restored = Baseline::from_json(&json).expect("deserialise failed");
        assert_eq!(bl.label,             restored.label);
        assert_eq!(bl.total_duration_us, restored.total_duration_us);
        assert_eq!(bl.avg_task_us,       restored.avg_task_us);
        assert_eq!(bl.p95_us,            restored.p95_us);
        assert_eq!(bl.task_count,        restored.task_count);
    }

    #[test]
    fn report_to_json_is_valid() {
        let profile  = profile_with_durations(&[100, 200], 2);
        let baseline = Baseline::from_profile(&profile, "ci");
        let detector = RegressionDetector::new(baseline, RegressionThresholds::default());
        let json     = detector.detect(&profile).to_json();
        assert!(json.contains("\"passed\":"));
        assert!(json.contains("\"violations\":"));
    }

    #[test]
    fn percentile_edge_cases() {
        assert_eq!(percentile_us(&[], 95), 0);
        assert_eq!(percentile_us(&[42], 50), 42);
        assert_eq!(percentile_us(&[1, 2, 3, 4, 100], 99), 100);
    }

    #[test]
    fn strict_thresholds_flag_small_regressions() {
        let bl_profile = profile_with_durations(&[100, 100, 100, 100], 2);
        let rg_profile = profile_with_durations(&[107, 107, 107, 107], 2); // 7% regression

        let bl_default = Baseline::from_profile(&bl_profile, "bl");
        let bl_strict  = Baseline::from_profile(&bl_profile, "bl");

        let d_default = RegressionDetector::new(bl_default, RegressionThresholds::default());
        assert!(d_default.detect(&rg_profile).passed, "should pass default thresholds");

        let d_strict = RegressionDetector::new(bl_strict, RegressionThresholds::strict());
        assert!(!d_strict.detect(&rg_profile).passed, "should fail strict thresholds");
    }
}
