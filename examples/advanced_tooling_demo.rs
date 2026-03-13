// examples/advanced_tooling_demo.rs
//
// Demonstrates the three new planned tooling features:
//
//   1. Real-time Dashboard  (http://localhost:9090)
//   2. Flamegraph Generation (flamegraph.svg)
//   3. Automated Regression Detection
//
// Run with:
//   cargo run --example advanced_tooling_demo

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use taskflowrs::{
    // Existing
    Executor, Taskflow, Profiler,
    monitoring::PerformanceMetrics,
    // New
    dashboard::{DashboardConfig, DashboardServer},
    flamegraph::{FlamegraphConfig, FlamegraphGenerator},
    regression::{Baseline, RegressionDetector, RegressionThresholds},
};

fn main() {
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║         Advanced Tooling Demo — TaskFlow RS              ║");
    println!("╚══════════════════════════════════════════════════════════╝\n");

    // ── 1. Real-time Dashboard ────────────────────────────────────────────────
    println!("━━━ 1. Real-time Dashboard ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let metrics = Arc::new(PerformanceMetrics::new(8));
    metrics.start();

    let cfg = DashboardConfig {
        port: 9090,
        push_interval_ms: 200,
        history_len: 60,
        title: "TaskFlow Demo Dashboard".to_string(),
    };

    let server = DashboardServer::new(Arc::clone(&metrics), 8, cfg);
    let dashboard_handle = server.start();

    println!("   ✓ Dashboard server started");
    println!("   → Open http://localhost:9090 in your browser");

    // Simulate some task activity so the dashboard has data to show.
    let m: Arc<PerformanceMetrics> = Arc::clone(&metrics);
    let sim_handle = thread::spawn(move || {
        for _ in 0..1000 {
            m.record_task_completion(Duration::from_micros(400 + rand_micros()));
            if rand_bool() { m.record_task_steal(); }
            m.record_worker_busy(0, Duration::from_micros(800));
            m.record_worker_busy(1, Duration::from_micros(600));
            m.record_worker_busy(2, Duration::from_micros(300));
            m.record_worker_busy(3, Duration::from_micros(900));
            thread::sleep(Duration::from_millis(25));
        }
    });

    // Let the dashboard serve for a few seconds so you can inspect it.
    println!("   (Simulating workload for 5 seconds…)");
    thread::sleep(Duration::from_secs(5));
    sim_handle.join().ok();
    dashboard_handle.stop();
    println!("   ✓ Dashboard server stopped\n");

    // ── 2. Flamegraph Generation ──────────────────────────────────────────────
    println!("━━━ 2. Flamegraph Generation ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Run a real taskflow and capture a profile.
    let mut executor = Executor::new(8);
    let profiler = Profiler::new(8);
    profiler.enable();
    profiler.start_run();

    let mut taskflow = Taskflow::new();

    let load   = taskflow.emplace(|| { spin_ms(100); }).name("Load Data");
    let parse  = taskflow.emplace(|| { spin_ms(80);  }).name("Parse");
    let comp_a = taskflow.emplace(|| { spin_ms(200); }).name("Compute A");
    let comp_b = taskflow.emplace(|| { spin_ms(150); }).name("Compute B");
    let merge  = taskflow.emplace(|| { spin_ms(60);  }).name("Merge Results");
    let output = taskflow.emplace(|| { spin_ms(40);  }).name("Write Output");

    parse.succeed(&load);
    comp_a.succeed(&parse);
    comp_b.succeed(&parse);
    merge.succeed(&comp_a);
    merge.succeed(&comp_b);
    output.succeed(&merge);

    executor.run(&taskflow).wait();

    // Manually record representative profiling data (in production this would
    // be auto-captured by an instrumented executor).
    let start = std::time::Instant::now();
    profiler.record_task(1, Some("Load Data".into()),    start, Duration::from_millis(100), 0, 0);
    profiler.record_task(2, Some("Parse".into()),        start, Duration::from_millis(80),  0, 1);
    profiler.record_task(3, Some("Compute A".into()),    start, Duration::from_millis(200), 1, 1);
    profiler.record_task(4, Some("Compute B".into()),    start, Duration::from_millis(150), 2, 1);
    profiler.record_task(5, Some("Merge Results".into()),start, Duration::from_millis(60),  0, 2);
    profiler.record_task(6, Some("Write Output".into()), start, Duration::from_millis(40),  1, 1);

    if let Some(profile) = profiler.get_profile() {
        // 2a. Flamegraph from ExecutionProfile.
        let fg_cfg = FlamegraphConfig {
            title: "TaskFlow Execution Flamegraph".to_string(),
            palette: "cool".to_string(),
            ..FlamegraphConfig::default()
        };
        let gen = FlamegraphGenerator::new(fg_cfg);
        gen.save_from_profile(&profile, "flamegraph.svg")
            .expect("failed to write flamegraph.svg");
        println!("   ✓ Flamegraph from profile saved → flamegraph.svg");
        println!("     Open flamegraph.svg in any browser (click to zoom, search supported)");

        // 2b. Flamegraph from folded stacks (e.g. from perf / dtrace).
        let folded = "\
Worker0;Load Data 100000\n\
Worker0;Parse 80000\n\
Worker1;Compute A;Inner Loop 180000\n\
Worker1;Compute A;Tail Work 20000\n\
Worker2;Compute B 150000\n\
Worker0;Merge Results 60000\n\
Worker1;Write Output 40000\n";

        let gen2 = FlamegraphGenerator::new(FlamegraphConfig {
            title: "Folded-Stacks Flamegraph".to_string(),
            palette: "hot".to_string(),
            ..FlamegraphConfig::default()
        });
        gen2.save_from_folded(folded, "flamegraph_folded.svg")
            .expect("failed to write flamegraph_folded.svg");
        println!("   ✓ Flamegraph from folded stacks saved → flamegraph_folded.svg\n");

        // ── 3. Automated Regression Detection ────────────────────────────────
        println!("━━━ 3. Automated Regression Detection ━━━━━━━━━━━━━━━━━━━━━━");

        // 3a. Record the current run as a baseline.
        let baseline = Baseline::from_profile(&profile, "demo-baseline-v1");
        baseline.save("perf_baseline.json").expect("save baseline");
        println!("   ✓ Baseline saved → perf_baseline.json");
        println!(
            "     total_duration={} µs  avg_task={} µs  p95={} µs",
            baseline.total_duration_us, baseline.avg_task_us, baseline.p95_us
        );

        // 3b. Simulate a "regressed" run (tasks take ~30% longer).
        let mut regressed = profile.clone();
        for stat in regressed.task_stats.iter_mut() {
            stat.duration = stat.duration.mul_f64(1.30);
        }
        regressed.total_duration = regressed.total_duration.mul_f64(1.30);

        let loaded_baseline = Baseline::load("perf_baseline.json")
            .expect("load baseline");
        let detector = RegressionDetector::new(loaded_baseline, RegressionThresholds::default());
        let report = detector.detect(&regressed);

        println!("\n   Regressed run result:");
        println!("{}", report.report());

        // 3c. Save the report JSON for CI artefacts.
        std::fs::write("regression_report.json", report.to_json())
            .expect("write report");
        println!("   ✓ JSON report saved → regression_report.json");

        // 3d. Show that an improved run passes.
        let mut improved = profile.clone();
        for stat in improved.task_stats.iter_mut() {
            stat.duration = stat.duration.mul_f64(0.85);
        }
        improved.total_duration = improved.total_duration.mul_f64(0.85);

        let baseline2 = Baseline::from_profile(&profile, "demo-baseline-v1");
        let detector2 = RegressionDetector::new(baseline2, RegressionThresholds::default());
        let report2 = detector2.detect(&improved);
        println!("\n   Improved run result: {}", report2.summary());

        // 3e. Demonstrate strict thresholds catching a small (7%) regression.
        let mut small_regress = profile.clone();
        for stat in small_regress.task_stats.iter_mut() {
            stat.duration = stat.duration.mul_f64(1.07);
        }
        small_regress.total_duration = small_regress.total_duration.mul_f64(1.07);

        let baseline3 = Baseline::from_profile(&profile, "strict-baseline");
        let d_strict  = RegressionDetector::new(baseline3.clone(), RegressionThresholds::strict());
        let d_default = RegressionDetector::new(baseline3, RegressionThresholds::default());
        let r_strict  = d_strict.detect(&small_regress);
        let r_default = d_default.detect(&small_regress);
        println!(
            "\n   7% regression — strict thresholds: {}  |  default thresholds: {}",
            if r_strict.passed  { "PASS ✓" } else { "FAIL ✗" },
            if r_default.passed { "PASS ✓" } else { "FAIL ✗" },
        );
    }

    println!("\n╔══════════════════════════════════════════════════════════╗");
    println!("║                    Demo complete ✓                       ║");
    println!("║                                                          ║");
    println!("║  Artefacts generated:                                    ║");
    println!("║    flamegraph.svg          (profile-based flamegraph)    ║");
    println!("║    flamegraph_folded.svg   (folded-stacks flamegraph)    ║");
    println!("║    perf_baseline.json      (serialised baseline)         ║");
    println!("║    regression_report.json  (CI-friendly report)          ║");
    println!("╚══════════════════════════════════════════════════════════╝\n");
}

// ── Tiny helpers ──────────────────────────────────────────────────────────────

/// Busy-spin for approximately `ms` milliseconds (avoids OS sleep jitter).
fn spin_ms(ms: u64) {
    let deadline = std::time::Instant::now() + Duration::from_millis(ms);
    while std::time::Instant::now() < deadline {
        std::hint::spin_loop();
    }
}

/// Very simple LCG pseudo-random (avoids pulling in rand crate).
fn rand_micros() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static STATE: AtomicU64 = AtomicU64::new(12345);
    let s = STATE.fetch_add(6364136223846793005, Ordering::Relaxed);
    (s >> 33) % 600
}

fn rand_bool() -> bool {
    rand_micros() % 5 == 0
}
