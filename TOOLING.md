# Tooling

TaskFlow-RS provides comprehensive tooling for profiling, visualization, performance monitoring, and debugging.

## Features

- **Profiler** - Collect detailed execution statistics
- **Visualization** - Generate DOT graphs, timelines, and HTML reports
- **Performance Monitoring** - Real-time metrics and worker utilization
- **Debug Logging** - Structured logging with multiple levels

## Profiler

### Basic Usage

```rust
use taskflow_rs::{Executor, Taskflow, Profiler};

let mut executor = Executor::new(4);
let profiler = Profiler::new(4);

// Enable profiling
profiler.enable();
profiler.start_run();

// Run your taskflow
executor.run(&taskflow).wait();

// Get execution profile
if let Some(profile) = profiler.get_profile() {
    println!("{}", profile.summary());
}
```

### Execution Profile

```rust
// Profile contains rich statistics
let profile = profiler.get_profile().unwrap();

// Basic metrics
println!("Total duration: {:?}", profile.total_duration);
println!("Tasks executed: {}", profile.task_stats.len());
println!("Workers used: {}", profile.num_workers);

// Performance analysis
println!("Average task: {:?}", profile.average_task_duration());
println!("Longest task: {:?}", profile.longest_task());
println!("Shortest task: {:?}", profile.shortest_task());
println!("Parallelism: {:.2}%", profile.parallelism_efficiency());

// Worker timeline
let timeline = profile.worker_timeline();
for (worker_id, tasks) in timeline {
    println!("Worker {}: {} tasks", worker_id, tasks.len());
}
```

### Profile Summary

```rust
// Generate text summary
let summary = profile.summary();
println!("{}", summary);

// Output:
// === Execution Profile Summary ===
// Total Duration: 523ms
// Tasks Executed: 10
// Workers Used: 4
// Average Task Duration: 52ms
// Parallelism Efficiency: 85.23%
// Longest Task: compute_heavy (200ms)
// Shortest Task: init (10ms)
```

## Visualization

### DOT Graphs

Generate Graphviz DOT files for task graph visualization:

```rust
use taskflow_rs::generate_dot_graph;

let tasks = vec![
    (1, "Load".to_string()),
    (2, "Process".to_string()),
    (3, "Save".to_string()),
];

let dependencies = vec![(1, 2), (2, 3)];

let dot = generate_dot_graph(&tasks, &dependencies);
std::fs::write("taskflow.dot", dot)?;
```

Convert to image:
```bash
dot -Tpng taskflow.dot -o taskflow.png
dot -Tsvg taskflow.dot -o taskflow.svg
dot -Tpdf taskflow.dot -o taskflow.pdf
```

### Timeline SVG

Generate execution timeline visualization:

```rust
use taskflow_rs::generate_timeline_svg;

let svg = generate_timeline_svg(&profile);
std::fs::write("timeline.svg", svg)?;
```

The timeline shows:
- Each worker as a horizontal lane
- Tasks as colored rectangles (by duration)
- Task names and timing information
- Time axis with markers

### HTML Reports

Generate comprehensive HTML reports:

```rust
use taskflow_rs::generate_html_report;

let html = generate_html_report(&profile);
std::fs::write("report.html", html)?;
```

Reports include:
- Executive summary with key metrics
- Interactive timeline visualization
- Detailed task table with durations
- Worker utilization charts
- Professional styling

## Performance Monitoring

### Real-Time Metrics

```rust
use taskflow_rs::PerformanceMetrics;

let metrics = PerformanceMetrics::new(4);
metrics.start();

// Record events
metrics.record_task_completion(Duration::from_millis(10));
metrics.record_task_steal();
metrics.record_worker_busy(0, Duration::from_secs(8));
metrics.record_worker_idle(0, Duration::from_secs(2));

// Query metrics
println!("Completed: {}", metrics.tasks_completed());
println!("Stolen: {} ({:.2}%)", 
         metrics.tasks_stolen(), 
         metrics.steal_rate());
println!("Tasks/sec: {:.2}", metrics.tasks_per_second());
println!("Worker 0 util: {:.2}%", metrics.worker_utilization(0));
```

### Available Metrics

**Task Metrics:**
- `tasks_completed()` - Total tasks finished
- `tasks_stolen()` - Tasks stolen by work-stealing
- `tasks_failed()` - Tasks that failed
- `steal_rate()` - Percentage of tasks stolen

**Timing Metrics:**
- `total_execution_time()` - Sum of all task durations
- `average_task_duration()` - Mean task duration
- `tasks_per_second()` - Throughput rate

**Worker Metrics:**
- `worker_utilization(id)` - Percentage busy
- `average_worker_utilization()` - Mean across all workers

### Metrics Summary

```rust
// Generate summary report
let summary = metrics.summary();
println!("{}", summary);

// Output:
// === Performance Metrics ===
// Tasks Completed: 1000
// Tasks Stolen: 150 (15.00%)
// Tasks Failed: 0
// Average Task Duration: 5.2ms
// Tasks/Second: 192.31
// Average Worker Utilization: 87.50%
//
// Worker Utilization:
//   Worker 0: 92.30%
//   Worker 1: 88.50%
//   Worker 2: 85.20%
//   Worker 3: 84.00%
```

## Debug Logging

### Basic Logging

```rust
use taskflow_rs::{DebugLogger, LogLevel};

let logger = DebugLogger::new();
logger.enable();
logger.set_log_level(LogLevel::Debug);

// Log messages
logger.info("Executor", "Starting execution");
logger.debug("Worker", "Processing task");
logger.warn("Scheduler", "Queue imbalance detected");
logger.error("Task", "Execution failed");
```

### Log Levels

```rust
pub enum LogLevel {
    Trace = 0,   // Verbose debugging
    Debug = 1,   // Debug information
    Info = 2,    // Informational messages
    Warn = 3,    // Warnings
    Error = 4,   // Errors
}
```

### Structured Logging

```rust
// Log with context
logger.log(
    LogLevel::Debug,
    "Task-42",
    "Task completed successfully",
    Some(2),      // Worker ID
    Some(42)      // Task ID
);

// Output:
// [  1.234] [DEBUG] [W2] [T42] [Task-42] Task completed successfully
```

### Log Management

```rust
// Get all logs
let logs = logger.get_logs();
for entry in logs {
    println!("{}", entry);
}

// Export to string
let log_text = logger.export_logs();

// Save to file
logger.save_to_file("execution.log")?;

// Clear logs
logger.clear();
```

### Log Filtering

```rust
// Set minimum level
logger.set_log_level(LogLevel::Warn);

logger.debug("test", "Not logged");  // Filtered out
logger.info("test", "Not logged");   // Filtered out
logger.warn("test", "Logged");       // Shown
logger.error("test", "Logged");      // Shown
```

## Integration Examples

### Complete Profiling Workflow

```rust
use taskflow_rs::*;

fn profile_workflow() -> std::io::Result<()> {
    // Setup
    let mut executor = Executor::new(4);
    let profiler = Profiler::new(4);
    profiler.enable();
    
    // Build taskflow
    let mut taskflow = build_complex_workflow();
    
    // Profile execution
    profiler.start_run();
    executor.run(&taskflow).wait();
    
    // Analyze results
    if let Some(profile) = profiler.get_profile() {
        // Print summary
        println!("{}", profile.summary());
        
        // Generate visualizations
        let html = generate_html_report(&profile);
        std::fs::write("report.html", html)?;
        
        // Check performance
        if profile.parallelism_efficiency() < 70.0 {
            println!("Warning: Low parallelism efficiency!");
        }
    }
    
    Ok(())
}
```

### Production Monitoring

```rust
use taskflow_rs::*;
use std::time::Duration;

fn monitor_production() {
    let mut executor = Executor::new(8);
    let metrics = PerformanceMetrics::new(8);
    let logger = DebugLogger::new();
    
    // Enable in production
    metrics.start();
    logger.enable();
    logger.set_log_level(LogLevel::Warn); // Only warnings/errors
    
    loop {
        // Process batch
        let taskflow = create_batch_workflow();
        executor.run(&taskflow).wait();
        
        // Check metrics
        if metrics.average_worker_utilization() < 50.0 {
            logger.warn("Monitor", "Low worker utilization detected");
        }
        
        if metrics.steal_rate() > 30.0 {
            logger.warn("Monitor", "High steal rate - check load balance");
        }
        
        // Periodic report
        if metrics.tasks_completed() % 1000 == 0 {
            logger.info("Monitor", &format!(
                "Processed {} tasks @ {:.2} tasks/sec",
                metrics.tasks_completed(),
                metrics.tasks_per_second()
            ));
        }
    }
}
```

### Debug Mode

```rust
use taskflow_rs::*;

fn debug_workflow() {
    let mut executor = Executor::new(4);
    let logger = DebugLogger::new();
    
    // Full debug mode
    logger.enable();
    logger.set_log_level(LogLevel::Trace);
    
    logger.info("Main", "Building workflow");
    
    let mut taskflow = Taskflow::new();
    
    let task_a = taskflow.emplace(|| {
        // Simulate work
    }).name("Task A");
    
    logger.debug("Builder", "Created Task A");
    
    let task_b = taskflow.emplace(|| {
        // Simulate work
    }).name("Task B");
    
    logger.debug("Builder", "Created Task B");
    
    task_a.precede(&task_b);
    logger.debug("Builder", "Added dependency A -> B");
    
    logger.info("Main", "Starting execution");
    executor.run(&taskflow).wait();
    logger.info("Main", "Execution complete");
    
    // Save debug log
    logger.save_to_file("debug.log").ok();
}
```

## Performance Tips

### 1. Profiling Overhead

Profiling adds ~5-10% overhead:
- Use in development and testing
- Disable in production for max performance
- Enable selectively for troubleshooting

```rust
// Development
#[cfg(debug_assertions)]
profiler.enable();

// Production
#[cfg(not(debug_assertions))]
profiler.disable();
```

### 2. Logging Overhead

Logging overhead depends on level:
- Trace/Debug: ~10-15% overhead
- Info: ~5% overhead
- Warn/Error: <1% overhead

```rust
// Production logging
logger.set_log_level(LogLevel::Warn); // Minimal overhead
```

### 3. Metrics Collection

Metrics use atomic operations (minimal overhead):
- Safe to enable in production
- ~1% overhead
- Real-time monitoring without profiling cost

### 4. Visualization

Generate visualizations offline:
- Profile execution
- Save profile data
- Generate reports later

## Best Practices

1. **Profile Early**: Profile during development to find bottlenecks
2. **Monitor Production**: Use metrics for real-time monitoring
3. **Log Strategically**: Use appropriate log levels
4. **Visualize Results**: HTML reports for analysis and presentation
5. **Baseline Performance**: Profile before optimization
6. **Compare Results**: Profile after optimization to measure improvement

## Command-Line Tools

### Generate Visualizations

```bash
# Generate PNG from DOT
dot -Tpng taskflow.dot -o taskflow.png

# Generate SVG (scalable)
dot -Tsvg taskflow.dot -o taskflow.svg

# Generate PDF
dot -Tpdf taskflow.dot -o taskflow.pdf

# Interactive layout
dot -Tx11 taskflow.dot  # X11 viewer
```

### View Reports

```bash
# Open HTML report in browser
open report.html           # macOS
xdg-open report.html      # Linux
start report.html         # Windows

# View SVG timeline
open timeline.svg
```

### Analyze Logs

```bash
# Filter logs by level
grep "ERROR" execution.log
grep "WARN" execution.log

# Count events
grep -c "Task completed" execution.log

# Extract timing
grep "Worker" execution.log | head -20
```

## Examples

Run the tooling demo:

```bash
cargo run --example tooling_demo
```

Expected output:
```
=== TaskFlow Tooling Demo ===

1. Profiling
   === Execution Profile Summary ===
   Total Duration: 225ms
   Tasks Executed: 3
   Workers Used: 4
   ...
   ✓ Profiling complete

2. Performance Monitoring
   Tasks completed: 100
   Tasks stolen: 10 (10.00%)
   Average task duration: 500µs
   Worker 0 utilization: 80.00%
   ...
   ✓ Performance monitoring complete

3. Debug Logging
   [  0.001] [INFO ] [Executor] Starting execution
   [  0.005] [DEBUG] [Worker-0] Popped task from queue
   ...
   ✓ Logs saved to execution.log

4. Visualization
   ✓ DOT graph saved to taskflow.dot
   ✓ HTML report saved to report.html
   ✓ Visualization complete
```

## Limitations

1. **Profiler**: Requires manual integration with executor
2. **Visualization**: DOT graphs require Graphviz installation
3. **Logging**: Console output may slow down high-throughput systems
4. **Metrics**: Worker-level metrics require executor instrumentation

## Future Enhancements

- Automatic profiler integration
- Real-time dashboard
- Flamegraph generation
- JSON export format
- Integration with external monitoring systems
- Automated performance regression detection
