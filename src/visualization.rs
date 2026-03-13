use crate::profiler::ExecutionProfile;
use std::fs::File;
use std::io::Write;

/// Generate DOT graph for taskflow visualization
pub fn generate_dot_graph(tasks: &[(usize, String)], dependencies: &[(usize, usize)]) -> String {
    let mut dot = String::from("digraph Taskflow {\n");
    dot.push_str("  rankdir=LR;\n");
    dot.push_str("  node [shape=box, style=rounded];\n\n");

    // Add nodes
    for (id, name) in tasks {
        dot.push_str(&format!("  {} [label=\"{}\"];\n", id, name));
    }

    dot.push_str("\n");

    // Add edges
    for (from, to) in dependencies {
        dot.push_str(&format!("  {} -> {};\n", from, to));
    }

    dot.push_str("}\n");
    dot
}

/// Save DOT graph to file
pub fn save_dot_graph(dot: &str, filename: &str) -> std::io::Result<()> {
    let mut file = File::create(filename)?;
    file.write_all(dot.as_bytes())?;
    Ok(())
}

/// Generate execution timeline visualization in SVG format
pub fn generate_timeline_svg(profile: &ExecutionProfile) -> String {
    let width = 1200;
    let height = 100 + profile.num_workers * 60;
    let margin_left = 150;
    let margin_top = 50;
    let timeline_width = width - margin_left - 50;

    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg width=\"{}\" height=\"{}\" xmlns=\"http://www.w3.org/2000/svg\">\n",
        width, height
    ));

    // Title
    svg.push_str(&format!(
        "  <text x=\"{}\" y=\"30\" font-size=\"20\" font-weight=\"bold\">Execution Timeline</text>\n",
        width / 2 - 80
    ));

    // Get timeline
    let timeline = profile.worker_timeline();
    let max_time = profile.total_duration.as_secs_f64();

    // Draw worker lanes
    for worker_id in 0..profile.num_workers {
        let y = margin_top + worker_id * 60;

        // Worker label
        svg.push_str(&format!(
            "  <text x=\"10\" y=\"{}\" font-size=\"14\">Worker {}</text>\n",
            y + 25,
            worker_id
        ));

        // Lane background
        svg.push_str(&format!(
            "  <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"40\" fill=\"#f0f0f0\" stroke=\"#999\"/>\n",
            margin_left, y, timeline_width
        ));

        // Tasks for this worker
        if let Some(tasks) = timeline.get(&worker_id) {
            for task in tasks {
                let start_offset = (task.start_time - profile.start_time).as_secs_f64();
                let duration = task.duration.as_secs_f64();

                let x = margin_left + (start_offset / max_time * timeline_width as f64) as i32;
                let task_width = ((duration / max_time * timeline_width as f64) as i32).max(2);

                // Task color based on duration
                let color = if duration > max_time * 0.3 {
                    "#e74c3c" // Red for long tasks
                } else if duration > max_time * 0.1 {
                    "#f39c12" // Orange for medium tasks
                } else {
                    "#2ecc71" // Green for short tasks
                };

                svg.push_str(&format!(
                    "  <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"35\" fill=\"{}\" stroke=\"#333\"/>\n",
                    x, y + 2, task_width, color
                ));

                // Task label (if wide enough)
                if task_width > 30 {
                    let label_text = match &task.name {
                        Some(name) => name.clone(),
                        None => format!("T{}", task.task_id),
                    };

                    svg.push_str(&format!(
                        "  <text x=\"{}\" y=\"{}\" font-size=\"10\" fill=\"white\">{}</text>\n",
                        x + 5,
                        y + 22,
                        label_text
                    ));
                }
            }
        }
    }

    // Time axis
    let axis_y = margin_top + profile.num_workers * 60 + 20;
    svg.push_str(&format!(
        "  <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#333\" stroke-width=\"2\"/>\n",
        margin_left,
        axis_y,
        margin_left + timeline_width,
        axis_y
    ));

    // Time markers
    for i in 0..=10 {
        let x = margin_left + (i * timeline_width / 10);
        let time = (i as f64 / 10.0) * max_time;

        svg.push_str(&format!(
            "  <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#333\"/>\n",
            x,
            axis_y - 5,
            x,
            axis_y + 5
        ));

        svg.push_str(&format!(
            "  <text x=\"{}\" y=\"{}\" font-size=\"10\" text-anchor=\"middle\">{:.2}s</text>\n",
            x,
            axis_y + 20,
            time
        ));
    }

    svg.push_str("</svg>\n");
    svg
}

/// Save timeline SVG to file
pub fn save_timeline_svg(svg: &str, filename: &str) -> std::io::Result<()> {
    let mut file = File::create(filename)?;
    file.write_all(svg.as_bytes())?;
    Ok(())
}

/// Generate HTML report with statistics and visualizations
pub fn generate_html_report(profile: &ExecutionProfile) -> String {
    let mut html = String::new();

    html.push_str("<!DOCTYPE html>\n<html>\n<head>\n");
    html.push_str("  <meta charset=\"UTF-8\">\n");
    html.push_str("  <title>TaskFlow Execution Report</title>\n");
    html.push_str("  <style>\n");
    html.push_str(
        "    body { font-family: Arial, sans-serif; margin: 20px; background: #f5f5f5; }\n",
    );
    html.push_str("    .container { max-width: 1200px; margin: 0 auto; background: white; padding: 20px; box-shadow: 0 2px 4px rgba(0,0,0,0.1); }\n");
    html.push_str(
        "    h1 { color: #333; border-bottom: 2px solid #2ecc71; padding-bottom: 10px; }\n",
    );
    html.push_str("    .stats { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 20px; margin: 20px 0; }\n");
    html.push_str("    .stat-box { background: #ecf0f1; padding: 15px; border-radius: 5px; }\n");
    html.push_str("    .stat-label { font-size: 12px; color: #7f8c8d; }\n");
    html.push_str("    .stat-value { font-size: 24px; font-weight: bold; color: #2c3e50; }\n");
    html.push_str("    .timeline { margin: 20px 0; overflow-x: auto; }\n");
    html.push_str("    table { width: 100%; border-collapse: collapse; margin: 20px 0; }\n");
    html.push_str(
        "    th, td { padding: 10px; text-align: left; border-bottom: 1px solid #ddd; }\n",
    );
    html.push_str("    th { background: #34495e; color: white; }\n");
    html.push_str("  </style>\n");
    html.push_str("</head>\n<body>\n");
    html.push_str("  <div class=\"container\">\n");

    // Header
    html.push_str("    <h1>TaskFlow Execution Report</h1>\n");

    // Statistics
    html.push_str("    <div class=\"stats\">\n");
    html.push_str(&format!(
        "      <div class=\"stat-box\"><div class=\"stat-label\">Total Duration</div><div class=\"stat-value\">{:.2}s</div></div>\n",
        profile.total_duration.as_secs_f64()
    ));
    html.push_str(&format!(
        "      <div class=\"stat-box\"><div class=\"stat-label\">Tasks Executed</div><div class=\"stat-value\">{}</div></div>\n",
        profile.task_stats.len()
    ));
    html.push_str(&format!(
        "      <div class=\"stat-box\"><div class=\"stat-label\">Workers Used</div><div class=\"stat-value\">{}</div></div>\n",
        profile.num_workers
    ));
    html.push_str(&format!(
        "      <div class=\"stat-box\"><div class=\"stat-label\">Parallelism</div><div class=\"stat-value\">{:.1}%</div></div>\n",
        profile.parallelism_efficiency()
    ));
    html.push_str("    </div>\n");

    // Timeline
    html.push_str("    <h2>Execution Timeline</h2>\n");
    html.push_str("    <div class=\"timeline\">\n");
    html.push_str(&generate_timeline_svg(profile));
    html.push_str("    </div>\n");

    // Task details table
    html.push_str("    <h2>Task Details</h2>\n");
    html.push_str("    <table>\n");
    html.push_str("      <tr><th>Task ID</th><th>Name</th><th>Duration</th><th>Worker</th></tr>\n");

    let mut sorted_tasks = profile.task_stats.clone();
    sorted_tasks.sort_by_key(|s| s.start_time);

    for task in &sorted_tasks {
        let name = task.name.as_ref().map(|s| s.as_str()).unwrap_or("-");
        html.push_str(&format!(
            "      <tr><td>{}</td><td>{}</td><td>{:.2}ms</td><td>{}</td></tr>\n",
            task.task_id,
            name,
            task.duration.as_secs_f64() * 1000.0,
            task.worker_id
        ));
    }

    html.push_str("    </table>\n");
    html.push_str("  </div>\n");
    html.push_str("</body>\n</html>\n");

    html
}

/// Save HTML report to file
pub fn save_html_report(html: &str, filename: &str) -> std::io::Result<()> {
    let mut file = File::create(filename)?;
    file.write_all(html.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiler::{ExecutionProfile, TaskStats};
    use std::time::{Duration, Instant};

    #[test]
    fn test_dot_graph_generation() {
        let tasks = vec![
            (1, "Task A".to_string()),
            (2, "Task B".to_string()),
            (3, "Task C".to_string()),
        ];
        let deps = vec![(1, 2), (1, 3)];

        let dot = generate_dot_graph(&tasks, &deps);
        assert!(dot.contains("digraph Taskflow"));
        assert!(dot.contains("Task A"));
        assert!(dot.contains("1 -> 2"));
    }

    #[test]
    fn test_timeline_svg_generation() {
        let start = Instant::now();
        let profile = ExecutionProfile {
            start_time: start,
            total_duration: Duration::from_secs(1),
            task_stats: vec![TaskStats {
                task_id: 1,
                name: Some("task1".to_string()),
                start_time: start,
                duration: Duration::from_millis(100),
                worker_id: 0,
                num_dependencies: 0,
            }],
            num_workers: 2,
        };

        let svg = generate_timeline_svg(&profile);
        assert!(svg.contains("<svg"));
        assert!(svg.contains("Execution Timeline"));
    }
}
