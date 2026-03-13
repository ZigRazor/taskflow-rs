// src/dashboard.rs
//
// Real-time performance dashboard served over HTTP with Server-Sent Events (SSE).

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::monitoring::PerformanceMetrics;
use crate::profiler::ExecutionProfile;

// ──────────────────────────────────────────────────────────────────────────────
// Public API
// ──────────────────────────────────────────────────────────────────────────────

/// Configuration for the dashboard server.
#[derive(Debug, Clone)]
pub struct DashboardConfig {
    /// TCP port to bind (default: 9090).
    pub port: u16,
    /// How often (ms) the SSE stream pushes a new snapshot (default: 500 ms).
    pub push_interval_ms: u64,
    /// How many historical data-points to keep in the ring-buffer (default: 120).
    pub history_len: usize,
    /// Page title shown in the browser tab.
    pub title: String,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            port: 9090,
            push_interval_ms: 500,
            history_len: 120,
            title: "TaskFlow Dashboard".to_string(),
        }
    }
}

/// An opaque handle to a running dashboard server.
pub struct DashboardHandle {
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl DashboardHandle {
    /// Signal the server to stop and wait for its thread to exit.
    pub fn stop(mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }

    pub fn is_running(&self) -> bool {
        !self.shutdown.load(Ordering::SeqCst)
    }
}

impl Drop for DashboardHandle {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Internal shared state
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct MetricSnapshot {
    timestamp_ms: u128,
    tasks_completed: usize,
    tasks_stolen: usize,
    steal_rate: f64,
    avg_task_us: u64,
    worker_utilizations: Vec<f64>,
    throughput_per_sec: f64,
}

impl MetricSnapshot {
    fn to_json(&self) -> String {
        let utils = self
            .worker_utilizations
            .iter()
            .map(|u| format!("{:.2}", u))
            .collect::<Vec<_>>()
            .join(",");

        // NOTE: Use r##"..."## so that "#hex" colours inside don't terminate the raw string.
        format!(
            r##"{{"ts":{ts},"completed":{c},"stolen":{s},"steal_rate":{sr:.3},"avg_task_us":{at},"worker_utils":[{u}],"throughput":{tp:.2}}}"##,
            ts = self.timestamp_ms,
            c = self.tasks_completed,
            s = self.tasks_stolen,
            sr = self.steal_rate,
            at = self.avg_task_us,
            u = utils,
            tp = self.throughput_per_sec,
        )
    }
}

struct SharedState {
    metrics: Arc<PerformanceMetrics>,
    #[allow(dead_code)]
    profile: Arc<Mutex<Option<ExecutionProfile>>>,
    history: Arc<Mutex<VecDeque<MetricSnapshot>>>,
    config: DashboardConfig,
    shutdown: Arc<AtomicBool>,
    start: Instant,
    last_completed: Arc<Mutex<usize>>,
    num_workers: usize,
}

// ──────────────────────────────────────────────────────────────────────────────
// DashboardServer
// ──────────────────────────────────────────────────────────────────────────────

pub struct DashboardServer {
    metrics: Arc<PerformanceMetrics>,
    profile: Arc<Mutex<Option<ExecutionProfile>>>,
    config: DashboardConfig,
    /// Cached worker count — passed explicitly because `PerformanceMetrics::num_workers`
    /// is a private field.  Callers always know this value at construction time.
    num_workers: usize,
}

impl DashboardServer {
    /// Create a new dashboard server.
    ///
    /// `num_workers` must match the value passed to `PerformanceMetrics::new`.
    pub fn new(
        metrics: Arc<PerformanceMetrics>,
        num_workers: usize,
        config: DashboardConfig,
    ) -> Self {
        Self {
            metrics,
            profile: Arc::new(Mutex::new(None)),
            config,
            num_workers,
        }
    }

    pub fn set_profile(&self, profile: ExecutionProfile) {
        *self.profile.lock().unwrap() = Some(profile);
    }

    pub fn start(self) -> DashboardHandle {
        let shutdown = Arc::new(AtomicBool::new(false));
        let sd_clone = Arc::clone(&shutdown);

        let num_workers = self.num_workers;

        let state = Arc::new(SharedState {
            metrics: self.metrics,
            profile: self.profile,
            history: Arc::new(Mutex::new(VecDeque::with_capacity(self.config.history_len))),
            config: self.config.clone(),
            shutdown: Arc::clone(&shutdown),
            start: Instant::now(),
            last_completed: Arc::new(Mutex::new(0usize)),
            num_workers,
        });

        let port = self.config.port;
        let thread = thread::Builder::new()
            .name("dashboard-server".to_string())
            .spawn(move || run_server(state, port, sd_clone))
            .expect("failed to spawn dashboard thread");

        DashboardHandle {
            shutdown,
            thread: Some(thread),
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Server loop
// ──────────────────────────────────────────────────────────────────────────────

fn run_server(state: Arc<SharedState>, port: u16, shutdown: Arc<AtomicBool>) {
    let addr = format!("127.0.0.1:{}", port);
    let listener = match TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[dashboard] bind error on {}: {}", addr, e);
            return;
        }
    };
    listener
        .set_nonblocking(true)
        .expect("set_nonblocking failed");
    eprintln!("[dashboard] listening on http://{}", addr);

    {
        let state_col = Arc::clone(&state);
        thread::Builder::new()
            .name("dashboard-collector".to_string())
            .spawn(move || run_collector(state_col))
            .expect("failed to spawn collector thread");
    }

    while !shutdown.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let state = Arc::clone(&state);
                thread::spawn(move || handle_connection(stream, state));
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(e) => {
                if !shutdown.load(Ordering::Relaxed) {
                    eprintln!("[dashboard] accept error: {}", e);
                }
                break;
            }
        }
    }
    eprintln!("[dashboard] server stopped");
}

// ──────────────────────────────────────────────────────────────────────────────
// Metrics collector
// ──────────────────────────────────────────────────────────────────────────────

fn run_collector(state: Arc<SharedState>) {
    let interval = Duration::from_millis(state.config.push_interval_ms);

    while !state.shutdown.load(Ordering::Relaxed) {
        let m = &state.metrics;
        let now_ms = state.start.elapsed().as_millis();

        // tasks_completed() / tasks_stolen() return usize.
        let completed: usize = m.tasks_completed();

        let throughput = {
            let mut last = state.last_completed.lock().unwrap();
            let delta = completed.saturating_sub(*last) as f64;
            *last = completed;
            delta / interval.as_secs_f64()
        };

        // average_task_duration() returns Duration directly (not Option<Duration>).
        let avg_task_us = m.average_task_duration().as_micros() as u64;

        let worker_utils: Vec<f64> = (0..state.num_workers)
            .map(|i| m.worker_utilization(i))
            .collect();

        let snap = MetricSnapshot {
            timestamp_ms: now_ms,
            tasks_completed: completed,
            tasks_stolen: m.tasks_stolen(),
            steal_rate: m.steal_rate(),
            avg_task_us,
            worker_utilizations: worker_utils,
            throughput_per_sec: throughput,
        };

        {
            let mut hist = state.history.lock().unwrap();
            if hist.len() >= state.config.history_len {
                hist.pop_front();
            }
            hist.push_back(snap);
        }

        thread::sleep(interval);
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// HTTP connection handler
// ──────────────────────────────────────────────────────────────────────────────

fn handle_connection(mut stream: TcpStream, state: Arc<SharedState>) {
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();

    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }

    let mut header = String::new();
    while let Ok(n) = reader.read_line(&mut header) {
        if n <= 2 {
            break;
        }
        header.clear();
    }

    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .to_string();

    match path.as_str() {
        "/" | "/index.html" => serve_html(&mut stream, &state),
        "/events" => serve_sse(&mut stream, &state),
        "/snapshot" => serve_snapshot(&mut stream, &state),
        _ => serve_404(&mut stream),
    }
}

fn serve_html(stream: &mut TcpStream, state: &SharedState) {
    let html = build_html(&state.config.title, state.num_workers);
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-cache\r\n\r\n{}",
        html.len(), html
    );
    let _ = stream.write_all(response.as_bytes());
}

fn serve_sse(stream: &mut TcpStream, state: &SharedState) {
    let headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nAccess-Control-Allow-Origin: *\r\nX-Accel-Buffering: no\r\n\r\n";
    if stream.write_all(headers.as_bytes()).is_err() {
        return;
    }

    let interval = Duration::from_millis(state.config.push_interval_ms);

    {
        let hist = state.history.lock().unwrap();
        for snap in hist.iter() {
            let event = format!("data: {}\n\n", snap.to_json());
            if stream.write_all(event.as_bytes()).is_err() {
                return;
            }
        }
    }

    while !state.shutdown.load(Ordering::Relaxed) {
        thread::sleep(interval);
        let snap = { state.history.lock().unwrap().back().cloned() };
        if let Some(snap) = snap {
            let event = format!("data: {}\n\n", snap.to_json());
            if stream.write_all(event.as_bytes()).is_err() {
                break;
            }
        }
    }
}

fn serve_snapshot(stream: &mut TcpStream, state: &SharedState) {
    let json = {
        let hist = state.history.lock().unwrap();
        hist.back()
            .map(|s| s.to_json())
            .unwrap_or_else(|| "{}".to_string())
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        json.len(),
        json
    );
    let _ = stream.write_all(response.as_bytes());
}

fn serve_404(stream: &mut TcpStream) {
    let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
}

// ──────────────────────────────────────────────────────────────────────────────
// Embedded HTML dashboard (self-contained, no CDN)
//
// IMPORTANT: All multi-line string templates use r##"..."## so that CSS/JS hex
// colour literals like "#1a1d27" do not accidentally terminate a r#"..."# raw
// string (which ends at the first occurrence of `"#`).
// ──────────────────────────────────────────────────────────────────────────────

fn build_html(title: &str, num_workers: usize) -> String {
    let worker_rows = (0..num_workers)
        .map(|i| format!(
            "<div class=\"worker-row\">\
             <span class=\"wlabel\">W{i}</span>\
             <div class=\"bar-bg\"><div class=\"bar-fill\" id=\"wfill-{i}\" style=\"width:0%\"></div></div>\
             <span class=\"wpct\" id=\"wpct-{i}\">0%</span>\
             </div>"
        ))
        .collect::<Vec<_>>()
        .join("\n");

    // CSS variables hold the colour palette; JS uses string literals for the
    // dynamic colours that are computed at runtime.
    let css = r##"
  :root{--bg:#0f1117;--surface:#1a1d27;--border:#2d3148;--accent:#5865f2;--green:#3ba55d;--text:#dcddde;--muted:#72767d}
  *{box-sizing:border-box;margin:0;padding:0}
  body{background:var(--bg);color:var(--text);font-family:"SF Mono",Consolas,monospace;font-size:13px;display:flex;flex-direction:column;min-height:100vh}
  header{background:var(--surface);border-bottom:1px solid var(--border);padding:12px 24px;display:flex;align-items:center;gap:12px}
  .dot{width:10px;height:10px;border-radius:50%;background:var(--green);animation:pulse 2s infinite}
  @keyframes pulse{0%,100%{opacity:1}50%{opacity:.4}}
  h1{font-size:15px;font-weight:600;letter-spacing:.5px}
  .status{margin-left:auto;color:var(--muted);font-size:11px}
  main{display:grid;grid-template-columns:repeat(auto-fit,minmax(280px,1fr));gap:16px;padding:20px}
  .card{background:var(--surface);border:1px solid var(--border);border-radius:8px;padding:16px}
  .card h2{font-size:11px;text-transform:uppercase;letter-spacing:1px;color:var(--muted);margin-bottom:12px}
  .stat-grid{display:grid;grid-template-columns:1fr 1fr;gap:10px}
  .stat{background:var(--bg);border-radius:6px;padding:10px 12px}
  .stat .label{font-size:10px;color:var(--muted);margin-bottom:4px}
  .stat .value{font-size:22px;font-weight:700;color:var(--accent)}
  .stat .unit{font-size:11px;color:var(--muted)}
  canvas{width:100%;height:180px;display:block}
  .worker-bars{display:flex;flex-direction:column;gap:8px}
  .worker-row{display:flex;align-items:center;gap:8px}
  .worker-row .wlabel{width:28px;text-align:right;color:var(--muted);font-size:11px}
  .bar-bg{flex:1;height:16px;background:var(--bg);border-radius:4px;overflow:hidden}
  .bar-fill{height:100%;border-radius:4px;transition:width .4s ease}
  .worker-row .wpct{width:38px;text-align:right;font-size:11px}
"##;

    // JavaScript — uses ES5 syntax for maximum browser compatibility.
    // Hex colours are in plain JS string literals, which are fine.
    let js = r##"
(function(){
  "use strict";
  var HISTORY = 120;
  var throughputHistory = [];
  var canvas = document.getElementById("chart-tp");
  var ctx = canvas.getContext("2d");

  function resizeCanvas(){
    canvas.width  = canvas.offsetWidth  * window.devicePixelRatio;
    canvas.height = canvas.offsetHeight * window.devicePixelRatio;
    ctx.scale(window.devicePixelRatio, window.devicePixelRatio);
  }
  resizeCanvas();
  window.addEventListener("resize", function(){ resizeCanvas(); drawChart(); });

  function drawChart(){
    var W = canvas.offsetWidth, H = canvas.offsetHeight;
    ctx.clearRect(0,0,W,H);
    if(throughputHistory.length < 2) return;
    var max = Math.max.apply(null, throughputHistory.concat([1])) * 1.2;
    var padL=48, padR=12, padT=10, padB=24;
    var cw = W-padL-padR, ch = H-padT-padB;
    ctx.strokeStyle = "#2d3148"; ctx.lineWidth = 1;
    for(var i=0; i<=4; i++){
      var y = padT + ch*(1 - i/4);
      ctx.beginPath(); ctx.moveTo(padL,y); ctx.lineTo(padL+cw,y); ctx.stroke();
      ctx.fillStyle = "#72767d"; ctx.font = "10px monospace";
      ctx.fillText((max*i/4).toFixed(0), 2, y+4);
    }
    ctx.beginPath(); ctx.strokeStyle = "#5865f2"; ctx.lineWidth = 2;
    throughputHistory.forEach(function(v,i){
      var x = padL + (i/(HISTORY-1))*cw;
      var y = padT + ch*(1 - v/max);
      i === 0 ? ctx.moveTo(x,y) : ctx.lineTo(x,y);
    });
    ctx.stroke();
    ctx.lineTo(padL+cw, padT+ch); ctx.lineTo(padL, padT+ch); ctx.closePath();
    ctx.fillStyle = "rgba(88,101,242,0.12)"; ctx.fill();
  }

  function updateWorkerBars(utils){
    utils.forEach(function(u,i){
      var fill = document.getElementById("wfill-" + i);
      var pct  = document.getElementById("wpct-"  + i);
      if(!fill) return;
      var color = u > 80 ? "#3ba55d" : u > 40 ? "#faa61a" : "#ed4245";
      fill.style.width      = u.toFixed(1) + "%";
      fill.style.background = color;
      pct.textContent       = u.toFixed(1) + "%";
    });
  }

  function setText(id, val){ var el=document.getElementById(id); if(el) el.textContent=val; }

  var dot    = document.getElementById("dot");
  var status = document.getElementById("status");
  var evtSrc = new EventSource("/events");

  evtSrc.onopen = function(){
    dot.style.background = "#3ba55d";
    status.textContent   = "Live";
  };
  evtSrc.onerror = function(){
    dot.style.background = "#ed4245";
    status.textContent   = "Disconnected - retrying...";
  };
  evtSrc.onmessage = function(e){
    var d; try{ d = JSON.parse(e.data); } catch(ex){ return; }
    setText("s-completed",  d.completed);
    setText("s-throughput", d.throughput.toFixed(1));
    setText("s-avg",        d.avg_task_us);
    setText("s-steal",      (d.steal_rate * 100).toFixed(1));
    throughputHistory.push(d.throughput);
    if(throughputHistory.length > HISTORY) throughputHistory.shift();
    drawChart();
    if(d.worker_utils) updateWorkerBars(d.worker_utils);
  };
})();
"##;

    format!(
        "<!DOCTYPE html>\n\
         <html lang=\"en\">\n\
         <head>\n\
         <meta charset=\"UTF-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n\
         <title>{title}</title>\n\
         <style>{css}</style>\n\
         </head>\n\
         <body>\n\
         <header>\n\
           <div class=\"dot\" id=\"dot\"></div>\n\
           <h1>{title}</h1>\n\
           <span class=\"status\" id=\"status\">Connecting...</span>\n\
         </header>\n\
         <main>\n\
           <div class=\"card\" style=\"grid-column:span 2\">\n\
             <h2>Live Metrics</h2>\n\
             <div class=\"stat-grid\">\n\
               <div class=\"stat\"><div class=\"label\">Tasks Completed</div><div class=\"value\" id=\"s-completed\">-</div></div>\n\
               <div class=\"stat\"><div class=\"label\">Throughput</div><div class=\"value\" id=\"s-throughput\">-</div><div class=\"unit\">tasks / sec</div></div>\n\
               <div class=\"stat\"><div class=\"label\">Avg Task Duration</div><div class=\"value\" id=\"s-avg\">-</div><div class=\"unit\">us</div></div>\n\
               <div class=\"stat\"><div class=\"label\">Steal Rate</div><div class=\"value\" id=\"s-steal\">-</div><div class=\"unit\">%</div></div>\n\
             </div>\n\
           </div>\n\
           <div class=\"card\" style=\"grid-column:span 2\">\n\
             <h2>Throughput over time (tasks/sec)</h2>\n\
             <canvas id=\"chart-tp\"></canvas>\n\
           </div>\n\
           <div class=\"card\" style=\"grid-column:span 2\">\n\
             <h2>Worker Utilisation</h2>\n\
             <div class=\"worker-bars\">{worker_rows}</div>\n\
           </div>\n\
         </main>\n\
         <script>{js}</script>\n\
         </body>\n\
         </html>",
        title = title,
        css = css,
        js = js,
        worker_rows = worker_rows,
    )
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_json_roundtrip() {
        let s = MetricSnapshot {
            timestamp_ms: 1234,
            tasks_completed: 100,
            tasks_stolen: 5,
            steal_rate: 0.05,
            avg_task_us: 250,
            worker_utilizations: vec![90.0, 75.0, 50.0],
            throughput_per_sec: 200.0,
        };
        let json = s.to_json();
        assert!(json.contains("\"ts\":1234"));
        assert!(json.contains("\"completed\":100"));
        assert!(json.contains("90.00"));
    }

    #[test]
    fn html_contains_canvas_and_sse() {
        let html = build_html("Test", 4);
        assert!(html.contains("<canvas"));
        assert!(html.contains("EventSource"));
        assert!(html.contains("wfill-3"));
        // Hex colours should be present inside JS/CSS sections
        assert!(html.contains("#5865f2"));
    }

    #[test]
    fn dashboard_config_default() {
        let cfg = DashboardConfig::default();
        assert_eq!(cfg.port, 9090);
        assert_eq!(cfg.push_interval_ms, 500);
    }
}
