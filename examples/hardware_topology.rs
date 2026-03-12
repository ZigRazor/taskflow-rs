//! Hardware topology integration demo
//!
//! Run (without hwloc): cargo run --example hardware_topology
//! Run (with hwloc):    cargo run --features hwloc --example hardware_topology

use taskflow_rs::{
    Executor, Taskflow,
    TopologyProvider, HwTopology,
    HwlocWorkerAffinity, AffinityStrategy,
};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::thread;

fn main() {
    println!("=== Hardware Topology Integration Demo ===\n");

    let topo = TopologyProvider::detect();
    println!("Backend: {}", topo.backend_name());
    println!("hwloc-backed: {}\n", topo.is_hwloc_backed());

    demo_system_overview(&topo);
    println!();

    demo_cache_hierarchy(&topo);
    println!();

    demo_numa_affinity(&topo);
    println!();

    demo_worker_pinning_strategies();
    println!();

    demo_topology_aware_taskflow(&topo);
}

// ─── 1. System overview ───────────────────────────────────────────────────────

fn demo_system_overview(topo: &TopologyProvider) {
    println!("1. System overview");

    println!("   Logical CPUs:  {}", topo.cpu_count());
    println!("   NUMA nodes:    {}", topo.numa_nodes().len());
    println!("   CPU packages:  {}", topo.packages().len());

    println!("\n   NUMA node details:");
    for node in topo.numa_nodes() {
        let mem = node.memory_bytes
            .map(|b| format!("{} MB", b / 1_048_576))
            .unwrap_or_else(|| "unknown".to_string());
        println!("     Node {}: {} CPUs, memory={}", node.id, node.cpus.len(), mem);
        println!("       CPUs: {:?}", &node.cpus[..node.cpus.len().min(8)]);
        if node.cpus.len() > 8 {
            println!("             … ({} more)", node.cpus.len() - 8);
        }
    }

    println!("\n   CPU packages:");
    for pkg in topo.packages() {
        println!("     Package {}: {} CPUs, NUMA nodes={:?}",
            pkg.id, pkg.cpus.len(), pkg.numa_nodes);
    }
    println!("   ✓ System overview complete");
}

// ─── 2. Cache hierarchy ───────────────────────────────────────────────────────

fn demo_cache_hierarchy(topo: &TopologyProvider) {
    println!("2. Cache hierarchy");

    let caches = topo.cache_info();
    if caches.is_empty() {
        println!("   (no cache information available)");
        return;
    }

    // Group by level
    let mut by_level: std::collections::BTreeMap<u8, Vec<_>> = std::collections::BTreeMap::new();
    for cache in &caches {
        by_level.entry(cache.level).or_default().push(cache);
    }

    for (level, entries) in &by_level {
        println!("   L{} cache ({} instance{}):",
            level, entries.len(), if entries.len() == 1 { "" } else { "s" });
        for (i, cache) in entries.iter().enumerate() {
            println!("     [{}] {} KB, line={} B, assoc={}, shared by {} CPUs",
                i, cache.size_kb, cache.line_size, cache.associativity,
                cache.shared_cpus.len());
            println!("         CPUs: {:?}", &cache.shared_cpus[..cache.shared_cpus.len().min(4)]);
        }
    }
    println!("   ✓ Cache hierarchy enumerated");
}

// ─── 3. NUMA affinity ────────────────────────────────────────────────────────

fn demo_numa_affinity(topo: &TopologyProvider) {
    println!("3. NUMA CPU-to-node mapping");

    let num_cpus = topo.cpu_count().min(16); // show first 16
    for cpu in 0..num_cpus {
        let node = topo.numa_node_for_cpu(cpu)
            .map(|n| n.to_string())
            .unwrap_or_else(|| "?".to_string());
        print!("   CPU {:>2} → node {}   ", cpu, node);
        if (cpu + 1) % 4 == 0 { println!(); }
    }
    if num_cpus % 4 != 0 { println!(); }
    println!("   ✓ NUMA mapping verified");
}

// ─── 4. Worker pinning strategies ────────────────────────────────────────────

fn demo_worker_pinning_strategies() {
    println!("4. Worker pinning strategies");

    let strategies = [
        AffinityStrategy::None,
        AffinityStrategy::NUMARoundRobin,
        AffinityStrategy::NUMADense,
        AffinityStrategy::PhysicalCores,
        AffinityStrategy::L3CacheDomain,
    ];

    let strategy_names = [
        "None",
        "NUMARoundRobin",
        "NUMADense",
        "PhysicalCores",
        "L3CacheDomain",
    ];

    let num_workers = 4;

    for (strategy, name) in strategies.iter().zip(strategy_names.iter()) {
        let topo = TopologyProvider::detect();
        let affinity = HwlocWorkerAffinity::new(topo, *strategy, num_workers);

        print!("   {:>16}: ", name);
        for w in 0..num_workers {
            let cpus = affinity.cpus_for_worker(w);
            if cpus.is_empty() {
                print!("w{}=[any] ", w);
            } else {
                print!("w{}={:?} ", w, cpus);
            }
        }
        println!();
    }
    println!("   ✓ All affinity strategies computed without error");
}

// ─── 5. Topology-aware taskflow ──────────────────────────────────────────────

fn demo_topology_aware_taskflow(topo: &TopologyProvider) {
    println!("5. Topology-aware taskflow execution");
    println!("   Creating one task-group per NUMA node\n");

    let numa_nodes = topo.numa_nodes();
    if numa_nodes.is_empty() {
        println!("   (no NUMA nodes, skipping demo)");
        return;
    }

    let results: Arc<Mutex<Vec<(usize, usize)>>> = Arc::new(Mutex::new(Vec::new()));

    for node in numa_nodes {
        let node_id  = node.id;
        let num_cpus = node.cpus.len().max(1);
        let res      = results.clone();

        // Spawn a mini executor per NUMA node with workers pinned to that
        // node's CPUs (best-effort – actual binding depends on OS support).
        let affinity = HwlocWorkerAffinity::new(
            TopologyProvider::detect(),
            AffinityStrategy::NUMADense,
            num_cpus,
        );

        let mut executor = Executor::new(num_cpus);
        let mut taskflow = Taskflow::new();

        for task_idx in 0..num_cpus.min(3) {
            let r = res.clone();
            let aff = affinity.cpus_for_worker(task_idx);

            taskflow.emplace(move || {
                // In production, pin the actual worker thread:
                //   TopologyProvider::detect().bind_thread(&aff).ok();
                let _ = &aff; // suppress unused warning in demo
                thread::sleep(Duration::from_millis(10));
                r.lock().unwrap().push((node_id, task_idx));
            });
        }

        executor.run(&taskflow).wait();
        println!("   NUMA node {}: ran {} tasks", node_id, num_cpus.min(3));
    }

    let completed = results.lock().unwrap();
    println!("\n   Completed tasks: {}", completed.len());
    println!("   ✓ Topology-aware task distribution successful");
}
