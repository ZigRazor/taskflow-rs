// NUMA (Non-Uniform Memory Access) support
// Provides topology detection and NUMA-aware task scheduling

use std::collections::HashMap;

/// NUMA node information
#[derive(Debug, Clone)]
pub struct NumaNode {
    /// Node ID
    pub id: usize,
    /// CPU cores on this node
    pub cpus: Vec<usize>,
    /// Memory capacity in bytes (if available)
    pub memory_bytes: Option<u64>,
}

/// NUMA topology information
#[derive(Debug, Clone)]
pub struct NumaTopology {
    /// Number of NUMA nodes
    pub num_nodes: usize,
    /// Details for each node
    pub nodes: Vec<NumaNode>,
    /// CPU to NUMA node mapping
    pub cpu_to_node: HashMap<usize, usize>,
}

impl NumaTopology {
    /// Detect NUMA topology
    ///
    /// Currently provides a simplified topology.
    /// For production use, integrate with hwloc or libnuma.
    pub fn detect() -> Self {
        // Simplified detection - assumes uniform system
        let num_cpus = num_cpus::get();

        // Check if NUMA is available (simplified check)
        let num_nodes = Self::detect_numa_nodes();

        if num_nodes <= 1 {
            // UMA system or NUMA not detected
            return Self::uniform(num_cpus);
        }

        // Distribute CPUs across NUMA nodes
        let cpus_per_node = (num_cpus + num_nodes - 1) / num_nodes;
        let mut nodes = Vec::new();
        let mut cpu_to_node = HashMap::new();

        for node_id in 0..num_nodes {
            let start_cpu = node_id * cpus_per_node;
            let end_cpu = ((node_id + 1) * cpus_per_node).min(num_cpus);
            let cpus: Vec<usize> = (start_cpu..end_cpu).collect();

            for &cpu in &cpus {
                cpu_to_node.insert(cpu, node_id);
            }

            nodes.push(NumaNode {
                id: node_id,
                cpus,
                memory_bytes: None,
            });
        }

        Self {
            num_nodes,
            nodes,
            cpu_to_node,
        }
    }

    /// Create a uniform topology (no NUMA)
    fn uniform(num_cpus: usize) -> Self {
        let cpus: Vec<usize> = (0..num_cpus).collect();
        let mut cpu_to_node = HashMap::new();

        for cpu in 0..num_cpus {
            cpu_to_node.insert(cpu, 0);
        }

        Self {
            num_nodes: 1,
            nodes: vec![NumaNode {
                id: 0,
                cpus,
                memory_bytes: None,
            }],
            cpu_to_node,
        }
    }

    /// Simplified NUMA node detection
    fn detect_numa_nodes() -> usize {
        // Try to read from /sys/devices/system/node on Linux
        #[cfg(target_os = "linux")]
        {
            if let Ok(entries) = std::fs::read_dir("/sys/devices/system/node") {
                let count = entries
                    .filter_map(|e| e.ok())
                    .filter(|e| {
                        e.file_name()
                            .to_str()
                            .map_or(false, |s| s.starts_with("node"))
                    })
                    .count();

                if count > 0 {
                    return count;
                }
            }
        }

        // Fallback: assume UMA
        1
    }

    /// Get the NUMA node for a CPU
    pub fn node_for_cpu(&self, cpu: usize) -> Option<usize> {
        self.cpu_to_node.get(&cpu).copied()
    }

    /// Check if system has NUMA
    pub fn has_numa(&self) -> bool {
        self.num_nodes > 1
    }
}

/// NUMA-aware worker pinning strategy
#[derive(Debug, Clone, Copy)]
pub enum NumaPinning {
    /// No pinning (let OS decide)
    None,
    /// Pin workers to NUMA nodes in round-robin fashion
    RoundRobin,
    /// Pin workers densely (fill one node before moving to next)
    Dense,
    /// Pin workers sparsely (distribute across nodes first)
    Sparse,
}

impl Default for NumaPinning {
    fn default() -> Self {
        NumaPinning::None
    }
}

/// Pin a thread to specific CPUs (simplified stub)
///
/// For production use, integrate with:
/// - Linux: libc's sched_setaffinity
/// - Windows: SetThreadAffinityMask
/// - Cross-platform: hwloc or core_affinity crate
pub fn pin_thread_to_cpus(_cpus: &[usize]) -> Result<(), String> {
    // Stub implementation
    // In production, use platform-specific APIs
    #[cfg(target_os = "linux")]
    {
        // Would use: libc::sched_setaffinity
        // For now, just succeed
    }

    Ok(())
}

/// Get the CPUs to pin a worker to based on strategy
pub fn get_worker_cpus(
    worker_id: usize,
    num_workers: usize,
    topology: &NumaTopology,
    strategy: NumaPinning,
) -> Vec<usize> {
    match strategy {
        NumaPinning::None => Vec::new(),

        NumaPinning::RoundRobin => {
            let node_id = worker_id % topology.num_nodes;
            if let Some(node) = topology.nodes.get(node_id) {
                let cpu_idx = (worker_id / topology.num_nodes) % node.cpus.len();
                vec![node.cpus[cpu_idx]]
            } else {
                Vec::new()
            }
        }

        NumaPinning::Dense => {
            // Fill nodes one by one
            let mut cpu_count = 0;
            for node in &topology.nodes {
                if worker_id < cpu_count + node.cpus.len() {
                    let idx = worker_id - cpu_count;
                    return vec![node.cpus[idx]];
                }
                cpu_count += node.cpus.len();
            }
            Vec::new()
        }

        NumaPinning::Sparse => {
            // Distribute workers across nodes first
            let workers_per_node = (num_workers + topology.num_nodes - 1) / topology.num_nodes;
            let node_id = worker_id / workers_per_node;
            let local_worker_id = worker_id % workers_per_node;

            if let Some(node) = topology.nodes.get(node_id) {
                if let Some(&cpu) = node.cpus.get(local_worker_id % node.cpus.len()) {
                    return vec![cpu];
                }
            }
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_numa_topology_detect() {
        let topology = NumaTopology::detect();
        assert!(topology.num_nodes >= 1);
        assert!(!topology.nodes.is_empty());
        assert!(!topology.cpu_to_node.is_empty());
    }

    #[test]
    fn test_numa_topology_uniform() {
        let topology = NumaTopology::uniform(8);
        assert_eq!(topology.num_nodes, 1);
        assert_eq!(topology.nodes[0].cpus.len(), 8);
        assert!(!topology.has_numa());
    }

    #[test]
    fn test_worker_cpu_assignment() {
        let topology = NumaTopology::uniform(8);

        let cpus = get_worker_cpus(0, 4, &topology, NumaPinning::Dense);
        assert!(!cpus.is_empty() || matches!(NumaPinning::Dense, NumaPinning::None));
    }
}
