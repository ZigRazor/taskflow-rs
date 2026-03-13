// src/flamegraph.rs
//
// Flamegraph generation from ExecutionProfile / raw folded-stack data.
//
// Architecture
// ============
//
//   FlamegraphGenerator
//     ├── from_profile(profile)  → builds FrameTree then renders SVG
//     ├── from_folded(text)      → parses "a;b;c N" format, builds FrameTree
//     └── render(tree)           → produces interactive SVG string
//
// IMPORTANT: All multi-line SVG/CSS/JS template strings use r##"..."## so
// that "#rrggbb" colour literals inside do not accidentally terminate a
// r#"..."# raw string (which ends at the first `"#` sequence).

use std::io::Write;

use crate::profiler::{ExecutionProfile, TaskStats};

// ──────────────────────────────────────────────────────────────────────────────
// Internal frame tree
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Frame {
    name: String,
    total: u64,
    self_samples: u64,
    children: Vec<Frame>,
}

impl Frame {
    fn new(name: impl Into<String>, total: u64) -> Self {
        Frame { name: name.into(), total, self_samples: total, children: Vec::new() }
    }

    fn add_child(&mut self, child: Frame) {
        if let Some(existing) = self.children.iter_mut().find(|c| c.name == child.name) {
            existing.total += child.total;
            existing.self_samples += child.self_samples;
            for grandchild in child.children {
                existing.add_child(grandchild);
            }
        } else {
            self.children.push(child);
        }
        let children_total: u64 = self.children.iter().map(|c| c.total).sum();
        self.self_samples = self.total.saturating_sub(children_total);
    }

    fn depth(&self) -> usize {
        if self.children.is_empty() { 1 } else {
            1 + self.children.iter().map(|c| c.depth()).max().unwrap_or(0)
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Public API
// ──────────────────────────────────────────────────────────────────────────────

/// Configuration for flamegraph rendering.
#[derive(Debug, Clone)]
pub struct FlamegraphConfig {
    pub width: u32,
    pub row_height: u32,
    pub font_size: u32,
    pub title: String,
    /// "hot" (red/orange), "cool" (blue/green), or "purple".
    pub palette: String,
    pub min_width: u32,
}

impl Default for FlamegraphConfig {
    fn default() -> Self {
        Self {
            width: 1200,
            row_height: 22,
            font_size: 12,
            title: "Flamegraph".to_string(),
            palette: "hot".to_string(),
            min_width: 2,
        }
    }
}

pub struct FlamegraphGenerator {
    config: FlamegraphConfig,
}

impl FlamegraphGenerator {
    pub fn new(config: FlamegraphConfig) -> Self {
        Self { config }
    }

    /// Build a flamegraph SVG from a finished [`ExecutionProfile`].
    pub fn from_profile(&self, profile: &ExecutionProfile) -> String {
        let root = build_frame_tree_from_profile(profile);
        self.render(&root, profile.total_duration.as_micros() as u64)
    }

    /// Build a flamegraph SVG from folded stacks text (`"a;b;c  N"` per line).
    pub fn from_folded(&self, folded: &str) -> String {
        let root = parse_folded(folded);
        let total = root.total;
        self.render(&root, total)
    }

    pub fn save_from_profile(&self, profile: &ExecutionProfile, path: &str) -> std::io::Result<()> {
        let svg = self.from_profile(profile);
        std::fs::File::create(path)?.write_all(svg.as_bytes())
    }

    pub fn save_from_folded(&self, folded: &str, path: &str) -> std::io::Result<()> {
        let svg = self.from_folded(folded);
        std::fs::File::create(path)?.write_all(svg.as_bytes())
    }

    fn render(&self, root: &Frame, total_samples: u64) -> String {
        if total_samples == 0 {
            return r##"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg"><text y="20">No data</text></svg>"##.to_string();
        }

        let cfg = &self.config;
        let depth = root.depth();
        let height = (depth as u32 + 3) * cfg.row_height + 40;

        let mut rects: Vec<RenderRect> = Vec::new();
        let y_base = height - cfg.row_height - 10;

        collect_rects(root, 0, total_samples, y_base, cfg, &mut rects, total_samples);
        build_svg(&rects, cfg.width, height, cfg, total_samples)
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// FrameTree from ExecutionProfile
// ──────────────────────────────────────────────────────────────────────────────

fn build_frame_tree_from_profile(profile: &ExecutionProfile) -> Frame {
    use std::collections::HashMap;
    let total_us = profile.total_duration.as_micros() as u64;

    let mut by_worker: HashMap<usize, Vec<&TaskStats>> = HashMap::new();
    for stat in &profile.task_stats {
        by_worker.entry(stat.worker_id).or_default().push(stat);
    }

    let mut root = Frame::new("all", total_us);

    for worker_id in 0..profile.num_workers {
        let tasks = match by_worker.get(&worker_id) {
            Some(t) => t,
            None => continue,
        };

        let worker_us: u64 = tasks.iter().map(|t| t.duration.as_micros() as u64).sum();
        if worker_us == 0 { continue; }

        let mut worker_frame = Frame::new(format!("Worker {}", worker_id), worker_us);

        let mut sorted = tasks.to_vec();
        sorted.sort_by_key(|t| t.start_time);

        for task in sorted {
            let dur_us = task.duration.as_micros() as u64;
            let name = task.name.as_deref().unwrap_or("task");
            let task_frame = Frame::new(
                format!("{} [id={}]", name, task.task_id),
                dur_us,
            );
            worker_frame.add_child(task_frame);
        }

        root.add_child(worker_frame);
    }

    root
}

// ──────────────────────────────────────────────────────────────────────────────
// Folded-stacks parser
// ──────────────────────────────────────────────────────────────────────────────

fn parse_folded(text: &str) -> Frame {
    let mut root = Frame::new("all", 0);

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }

        let (stack_part, count_str) = match line.rsplit_once(|c: char| c.is_whitespace()) {
            Some(pair) => pair,
            None => continue,
        };

        let count: u64 = match count_str.trim().parse() {
            Ok(n) => n,
            Err(_) => continue,
        };

        let frames: Vec<&str> = stack_part.split(';').map(str::trim).collect();
        insert_path(&mut root, &frames, count);
    }

    root.total = root.children.iter().map(|c| c.total).sum();
    root
}

fn insert_path(node: &mut Frame, path: &[&str], count: u64) {
    if path.is_empty() { return; }

    let head = path[0];
    let tail = &path[1..];

    if let Some(child) = node.children.iter_mut().find(|c| c.name == head) {
        child.total += count;
        insert_path(child, tail, count);
        let children_total: u64 = child.children.iter().map(|c| c.total).sum();
        child.self_samples = child.total.saturating_sub(children_total);
    } else {
        let mut new_frame = Frame::new(head, count);
        insert_path(&mut new_frame, tail, count);
        let children_total: u64 = new_frame.children.iter().map(|c| c.total).sum();
        new_frame.self_samples = new_frame.total.saturating_sub(children_total);
        node.children.push(new_frame);
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Rect collection
// ──────────────────────────────────────────────────────────────────────────────

struct RenderRect {
    x: f64,
    y: u32,
    w: f64,
    h: u32,
    name: String,
    total: u64,
    self_s: u64,
    color: String,
}

fn collect_rects(
    frame: &Frame,
    sample_offset: u64,
    _parent_samples: u64,
    y: u32,
    cfg: &FlamegraphConfig,
    rects: &mut Vec<RenderRect>,
    grand_total: u64,
) {
    if grand_total == 0 { return; }

    let x = (sample_offset as f64 / grand_total as f64) * cfg.width as f64;
    let w = (frame.total as f64 / grand_total as f64) * cfg.width as f64;

    if w < cfg.min_width as f64 { return; }

    rects.push(RenderRect {
        x,
        y,
        w,
        h: cfg.row_height,
        name: frame.name.clone(),
        total: frame.total,
        self_s: frame.self_samples,
        color: frame_color(&frame.name, &cfg.palette),
    });

    if y < cfg.row_height { return; }
    let child_y = y - cfg.row_height;
    let mut child_offset = sample_offset;
    for child in &frame.children {
        collect_rects(child, child_offset, grand_total, child_y, cfg, rects, grand_total);
        child_offset += child.total;
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Color palette
// ──────────────────────────────────────────────────────────────────────────────

fn frame_color(name: &str, palette: &str) -> String {
    let hash: u32 = name
        .bytes()
        .fold(5381u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));

    let base_h = (hash % 360) as f32;
    let s = 60.0 + (hash % 30) as f32;
    let l = 45.0 + (hash % 20) as f32;

    let h = match palette {
        "cool"   => 180.0 + base_h % 120.0,
        "purple" => 260.0 + base_h % 60.0,
        _        => base_h % 60.0 + 10.0,
    };

    hsl_to_hex(h, s, l)
}

fn hsl_to_hex(h: f32, s: f32, l: f32) -> String {
    let s = s / 100.0;
    let l = l / 100.0;
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r1, g1, b1) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    format!(
        "#{:02x}{:02x}{:02x}",
        ((r1 + m) * 255.0) as u8,
        ((g1 + m) * 255.0) as u8,
        ((b1 + m) * 255.0) as u8,
    )
}

// ──────────────────────────────────────────────────────────────────────────────
// SVG builder
// ──────────────────────────────────────────────────────────────────────────────

fn build_svg(
    rects: &[RenderRect],
    width: u32,
    height: u32,
    cfg: &FlamegraphConfig,
    total_samples: u64,
) -> String {
    let mut svg = String::with_capacity(rects.len() * 512 + 4096);

    // ── SVG header ───────────────────────────────────────────────────────────
    // NOTE: r##"..."## used so internal "#colour" values don't end the raw string.
    svg.push_str(&format!(
        r##"<?xml version="1.0" encoding="UTF-8"?>
<svg version="1.1" width="{w}" height="{h}"
     xmlns="http://www.w3.org/2000/svg"
     xmlns:xlink="http://www.w3.org/1999/xlink"
     onload="flamegraphInit()">
<defs>
  <linearGradient id="bg-grad" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0%"   stop-color="#1a1d27"/>
    <stop offset="100%" stop-color="#0f1117"/>
  </linearGradient>
</defs>
<style>
  text {{ font-family: Consolas,"SF Mono",monospace; font-size: {fs}px; }}
  .frame {{ cursor: pointer; }}
</style>
<rect width="{w}" height="{h}" fill="url(#bg-grad)"/>
"##,
        w = width, h = height, fs = cfg.font_size,
    ));

    // ── Title ────────────────────────────────────────────────────────────────
    svg.push_str(&format!(
        "<text x=\"{}\" y=\"20\" text-anchor=\"middle\" fill=\"#dcddde\" font-size=\"15\" font-weight=\"bold\">{}</text>\n",
        width / 2,
        escape_xml(&cfg.title),
    ));
    svg.push_str(&format!(
        "<text x=\"{}\" y=\"36\" text-anchor=\"middle\" fill=\"#72767d\" font-size=\"11\">Total: {} us -- click to zoom, Ctrl+click to reset</text>\n",
        width / 2,
        total_samples,
    ));

    // ── Search bar (HTML foreignObject) ──────────────────────────────────────
    let search_x = width.saturating_sub(210);
    svg.push_str(&format!(
        r##"<foreignObject x="{x}" y="6" width="200" height="30">
  <body xmlns="http://www.w3.org/1999/xhtml" style="margin:0">
    <input id="fg-search" type="text" placeholder="Search..."
           style="font-size:11px;border:1px solid #5865f2;background:#1a1d27;color:#dcddde;padding:3px 6px;border-radius:4px;width:120px"
           oninput="flamegraphSearch(this.value)"/>
    <button onclick="flamegraphReset()"
            style="margin-left:4px;font-size:11px;background:#5865f2;color:#fff;border:none;border-radius:4px;padding:3px 8px;cursor:pointer">Reset</button>
  </body>
</foreignObject>
"##,
        x = search_x,
    ));

    // ── Frame rectangles ─────────────────────────────────────────────────────
    svg.push_str("<g id=\"fg-frames\">\n");
    for (i, r) in rects.iter().enumerate() {
        let label_x = r.x + 3.0;
        let label_y = r.y as f64 + r.h as f64 - 6.0;
        let pct = r.total as f64 / total_samples as f64 * 100.0;
        let tooltip = format!(
            "{} ({:.2}% total, {} us self)",
            escape_xml(&r.name), pct, r.self_s
        );

        svg.push_str(&format!(
            "<g class=\"frame\" id=\"f{i}\" onclick=\"flamegraphClick(evt,{i})\" data-name=\"{dname}\">\
             <rect x=\"{x:.1}\" y=\"{y}\" width=\"{w:.1}\" height=\"{h}\" fill=\"{color}\" rx=\"2\"/>\
             <title>{tooltip}</title>\
             <clipPath id=\"cp{i}\"><rect x=\"{x:.1}\" y=\"{y}\" width=\"{w:.1}\" height=\"{h}\"/></clipPath>\
             <text x=\"{lx:.1}\" y=\"{ly:.1}\" fill=\"#111\" clip-path=\"url(#cp{i})\">{label}</text>\
             </g>\n",
            i = i,
            x = r.x, y = r.y, w = r.w, h = r.h,
            color = r.color,
            tooltip = tooltip,
            lx = label_x, ly = label_y,
            dname = escape_xml(&r.name),
            label = escape_xml(&r.name),
        ));
    }
    svg.push_str("</g>\n");

    // ── Inline JavaScript for zoom + search ──────────────────────────────────
    // The JS is in a separate r##"..."## block so hex colours are safe.
    svg.push_str(&format!(
        r##"<script type="text/ecmascript"><![CDATA[
var fgWidth = {width};
var fgFrames = [];

function flamegraphInit() {{
  var gs = document.getElementsByClassName("frame");
  for (var i = 0; i < gs.length; i++) {{
    var g = gs[i];
    var r = g.querySelector("rect");
    fgFrames.push({{
      g:     g,
      origX: parseFloat(r.getAttribute("x")),
      origW: parseFloat(r.getAttribute("width")),
      origY: parseInt(r.getAttribute("y")),
      rect:  r,
      txt:   g.querySelector("text"),
      name:  g.getAttribute("data-name"),
    }});
  }}
}}

function flamegraphClick(evt, idx) {{
  if (evt.ctrlKey) {{ flamegraphReset(); return; }}
  var f = fgFrames[idx];
  zoomTo(f.origX, f.origX + f.origW);
}}

function flamegraphReset() {{
  zoomTo(0, fgWidth);
}}

function zoomTo(x0, x1) {{
  var span = x1 - x0;
  if (span <= 0) return;
  var scale = fgWidth / span;
  for (var i = 0; i < fgFrames.length; i++) {{
    var f = fgFrames[i];
    var nx = (f.origX - x0) * scale;
    var nw = f.origW * scale;
    if (nw < 1) {{
      f.g.setAttribute("visibility", "hidden");
    }} else {{
      f.g.setAttribute("visibility", "visible");
      f.rect.setAttribute("x", nx.toFixed(1));
      f.rect.setAttribute("width", nw.toFixed(1));
      var cp = document.getElementById("cp" + i);
      if (cp) {{
        var cpr = cp.querySelector("rect");
        cpr.setAttribute("x", nx.toFixed(1));
        cpr.setAttribute("width", nw.toFixed(1));
      }}
      if (f.txt) f.txt.setAttribute("x", (nx + 3).toFixed(1));
    }}
  }}
}}

function flamegraphSearch(q) {{
  q = q.toLowerCase();
  for (var i = 0; i < fgFrames.length; i++) {{
    var f = fgFrames[i];
    var match = q === "" || f.name.toLowerCase().indexOf(q) >= 0;
    f.rect.setAttribute("opacity", match ? "1" : "0.2");
  }}
}}
]]></script>
"##,
        width = width,
    ));

    svg.push_str("</svg>\n");
    svg
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace('"', "&quot;")
     .replace('\'', "&apos;")
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiler::{ExecutionProfile, TaskStats};
    use std::time::{Duration, Instant};

    fn make_profile() -> ExecutionProfile {
        let start = Instant::now();
        ExecutionProfile {
            start_time: start,
            total_duration: Duration::from_millis(500),
            task_stats: vec![
                TaskStats {
                    task_id: 1,
                    name: Some("Load".to_string()),
                    start_time: start,
                    duration: Duration::from_millis(100),
                    worker_id: 0,
                    num_dependencies: 0,
                },
                TaskStats {
                    task_id: 2,
                    name: Some("Compute".to_string()),
                    start_time: start,
                    duration: Duration::from_millis(300),
                    worker_id: 1,
                    num_dependencies: 1,
                },
            ],
            num_workers: 2,
        }
    }

    #[test]
    fn generates_svg_from_profile() {
        let gen = FlamegraphGenerator::new(FlamegraphConfig::default());
        let svg = gen.from_profile(&make_profile());
        assert!(svg.starts_with("<?xml"));
        assert!(svg.contains("Load"));
        assert!(svg.contains("Compute"));
        assert!(svg.contains("flamegraphInit"));
    }

    #[test]
    fn generates_svg_from_folded() {
        let folded = "Worker0;task_a 300\nWorker1;task_b;subtask 200\n";
        let gen = FlamegraphGenerator::new(FlamegraphConfig::default());
        let svg = gen.from_folded(folded);
        assert!(svg.contains("task_a"));
        assert!(svg.contains("subtask"));
    }

    #[test]
    fn color_is_deterministic() {
        let c1 = frame_color("my_task", "hot");
        let c2 = frame_color("my_task", "hot");
        assert_eq!(c1, c2);
    }

    #[test]
    fn escape_xml_works() {
        assert_eq!(
            escape_xml("<foo>&\"'</foo>"),
            "&lt;foo&gt;&amp;&quot;&apos;&lt;/foo&gt;"
        );
    }

    #[test]
    fn folded_parser_merges_same_name() {
        let folded = "a;b 10\na;b 20\n";
        let root = parse_folded(folded);
        let a = root.children.iter().find(|c| c.name == "a").unwrap();
        let b = a.children.iter().find(|c| c.name == "b").unwrap();
        assert_eq!(b.total, 30);
    }
}
