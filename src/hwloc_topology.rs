//! Hardware topology integration via **hwloc**.
//!
//! Provides rich hardware topology discovery (NUMA nodes, CPU packages,
//! cache hierarchy) and actual CPU/memory binding for worker threads.
//!
//! ## Feature flag
//!
//! Full hwloc integration is enabled with `--features hwloc`.
//! Without the flag the module compiles to a *fallback* implementation that
//! uses `/sys` discovery (Linux) and approximated cache information, keeping
//! the API identical so callers need no conditional compilation.
//!
//! ## Architecture
//!
//! ```text
//!  HwTopology (trait)
//!       │
//!       ├── [cfg hwloc]  HwlocBackend   ──wraps──► hwloc2::Topology
//!       │                                           real NUMA / cache / bind
//!       └── [fallback]   SysfsBackend   ──reads──►  /sys/devices/system/
//!                                                   + approximate cache info
//!
//!  TopologyProvider (enum)  selects backend at runtime
//!
//!  HwlocWorkerAffinity  ──holds──► TopologyProvider
//!                                  pin_worker(id, strategy) → O(1) syscall
//! ```
//!
//! ## Usage
//!
//! ```rust
//! use taskflow_rs::hwloc_topology::{TopologyProvider, AffinityStrategy};
//!
//! let topo = TopologyProvider::detect();
//! println!("NUMA nodes: {}", topo.numa_nodes().len());
//!
//! for pkg in topo.packages() {
//!     println!("Socket {}: {:?}", pkg.id, pkg.cpus);
//! }
//!
//! for cache in topo.cache_info() {
//!     println!("L{} cache: {} KB, shared by {:?}", cache.level, cache.size_kb, cache.shared_cpus);
//! }
//!
//! // Pin the current thread to CPUs [0, 1].
//! topo.bind_thread(&[0, 1]).ok();
//! ```

use crate::numa::{NumaNode, NumaTopology};

// ─── Public data types ────────────────────────────────────────────────────────

/// Information about a CPU cache level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheInfo {
    /// Cache level (1 = L1, 2 = L2, 3 = L3, …).
    pub level: u8,
    /// Total capacity in kilobytes.
    pub size_kb: u64,
    /// Cache line size in bytes.
    pub line_size: u32,
    /// Number of ways (associativity).  `0` = fully associative or unknown.
    pub associativity: u32,
    /// Logical CPU indices that share this cache instance.
    pub shared_cpus: Vec<usize>,
}

impl CacheInfo {
    /// `true` if this is a unified (instruction + data) cache.
    pub fn is_unified(&self) -> bool {
        self.level >= 2 // L1 may be split; L2+ typically unified
    }
}

/// Information about a physical CPU package (socket).
#[derive(Debug, Clone)]
pub struct PackageInfo {
    /// Physical package / socket index.
    pub id: usize,
    /// Logical CPU indices on this package.
    pub cpus: Vec<usize>,
    /// NUMA node indices whose memory is attached to this package.
    pub numa_nodes: Vec<usize>,
}

/// Binding error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindError(pub String);

impl std::fmt::Display for BindError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CPU binding error: {}", self.0)
    }
}

impl std::error::Error for BindError {}

// ─── Trait ───────────────────────────────────────────────────────────────────

/// Unified hardware topology interface.
///
/// Both the hwloc backend and the sysfs fallback implement this trait so
/// higher-level code is fully backend-agnostic.
pub trait HwTopology: Send + Sync {
    /// NUMA nodes detected on this system.
    fn numa_nodes(&self) -> &[NumaNode];

    /// Total logical CPU count.
    fn cpu_count(&self) -> usize;

    /// Physical packages (sockets).
    fn packages(&self) -> Vec<PackageInfo>;

    /// Cache hierarchy entries, ordered by (level, first shared CPU).
    fn cache_info(&self) -> Vec<CacheInfo>;

    /// Pin the **current** thread to the given set of logical CPU indices.
    ///
    /// Returns `Err(BindError)` if the OS call fails or if the backend does
    /// not support binding.
    fn bind_thread(&self, cpus: &[usize]) -> Result<(), BindError>;

    /// Which NUMA node owns `cpu`, or `None` if not determinable.
    fn numa_node_for_cpu(&self, cpu: usize) -> Option<usize>;

    /// True if hwloc data is available (false for sysfs fallback).
    fn is_hwloc_backed(&self) -> bool;

    /// Human-readable backend name for diagnostics.
    fn backend_name(&self) -> &'static str;
}

// ─── hwloc backend ───────────────────────────────────────────────────────────

#[cfg(feature = "hwloc")]
mod hwloc_backend {
    use super::*;
    use std::collections::HashMap;
    // hwloc2 = "2.2.0"  API surface:
    //   Topology::new() → Option<Topology>
    //   Topology::objects_with_type(&ObjectType) → Result<Vec<&TopologyObject>, _>
    //   Topology::objects_at_depth(depth) → Vec<&TopologyObject>
    //   Topology::set_cpubind(CpuSet, CpuBindFlags) → Result<(), CpuBindError>
    //   TopologyObject::logical_index() → u32
    //   TopologyObject::cpuset() → Option<CpuSet>   (CpuSet = Bitmap)
    //   TopologyObject::memory() → TopologyObjectMemory  (.local_memory() → u64)
    //   Bitmap::iter() → impl Iterator<Item = u32>
    //   CpuBindFlags: PROCESS | THREAD | STRICT | NOMEMBIND
    use hwloc2::{CpuBindFlags, ObjectType, Topology};

    pub struct HwlocBackend {
        topology: Topology,
        numa_nodes: Vec<NumaNode>,
        cpu_to_node: HashMap<usize, usize>,
    }

    impl HwlocBackend {
        pub fn new() -> Result<Self, String> {
            let topology = Topology::new()
                .ok_or_else(|| "hwloc: failed to create topology object".to_string())?;
            let (numa_nodes, cpu_to_node) = Self::extract_numa(&topology);
            Ok(Self {
                topology,
                numa_nodes,
                cpu_to_node,
            })
        }

        // ── NUMA ──────────────────────────────────────────────────────────────

        fn extract_numa(topo: &Topology) -> (Vec<NumaNode>, HashMap<usize, usize>) {
            let mut numa_nodes = Vec::new();
            let mut cpu_to_node = HashMap::new();

            if let Ok(objects) = topo.objects_with_type(&ObjectType::NUMANode) {
                for obj in &objects {
                    let id = obj.logical_index() as usize;
                    let cpus: Vec<usize> = obj
                        .cpuset()
                        .iter()
                        .flat_map(|cs| cs.clone().into_iter().map(|c| c as usize))
                        .collect();
                    for &cpu in &cpus {
                        cpu_to_node.insert(cpu, id);
                    }
                    // local_memory() is on TopologyObjectMemory
                    let memory_bytes = Some(obj.total_memory());
                    numa_nodes.push(NumaNode {
                        id,
                        cpus,
                        memory_bytes,
                    });
                }
            }

            if numa_nodes.is_empty() {
                // UMA system: collect all PUs as a single node.
                let cpus = Self::all_pu_cpus(topo);
                for &c in &cpus {
                    cpu_to_node.insert(c, 0);
                }
                numa_nodes.push(NumaNode {
                    id: 0,
                    cpus,
                    memory_bytes: None,
                });
            }

            (numa_nodes, cpu_to_node)
        }

        fn all_pu_cpus(topo: &Topology) -> Vec<usize> {
            topo.objects_with_type(&ObjectType::PU)
                .map(|objs| objs.iter().map(|o| o.logical_index() as usize).collect())
                .unwrap_or_else(|_| (0..num_cpus::get()).collect())
        }

        // ── Packages ──────────────────────────────────────────────────────────

        fn extract_packages(topo: &Topology, numa_nodes: &[NumaNode]) -> Vec<PackageInfo> {
            let objects = match topo.objects_with_type(&ObjectType::Package) {
                Ok(o) if !o.is_empty() => o,
                _ => return Vec::new(),
            };

            objects
                .iter()
                .map(|obj| {
                    let id = obj.logical_index() as usize;
                    let cpus: Vec<usize> = obj
                        .cpuset()
                        .iter()
                        .flat_map(|cs| cs.clone().into_iter().map(|c| c as usize))
                        .collect();
                    let numa_node_ids = numa_nodes
                        .iter()
                        .filter(|n| n.cpus.iter().any(|c| cpus.contains(c)))
                        .map(|n| n.id)
                        .collect();
                    PackageInfo {
                        id,
                        cpus,
                        numa_nodes: numa_node_ids,
                    }
                })
                .collect()
        }

        // ── Cache ─────────────────────────────────────────────────────────────

        fn extract_caches(topo: &Topology) -> Vec<CacheInfo> {
            let mut caches = Vec::new();

            // ObjectType cache levels in hwloc2 2.2.0
            let cache_levels: &[(u8, ObjectType)] = &[
                (1, ObjectType::L1Cache),
                (2, ObjectType::L2Cache),
                (3, ObjectType::L3Cache),
            ];

            for &(level, ref obj_type) in cache_levels {
                let objects = match topo.objects_with_type(obj_type) {
                    Ok(o) => o,
                    Err(_) => continue,
                };

                for obj in &objects {
                    let shared_cpus: Vec<usize> = obj
                        .cpuset()
                        .iter()
                        .flat_map(|cs| cs.clone().into_iter().map(|c| c as usize))
                        .collect();

                    // Cache attributes: hwloc2 2.2.0 exposes these through
                    // TopologyObject::cache_attributes() → Option<TopologyCacheAttributes>
                    // which has .size() → u64, .line_size() → u32, .associativity() → i32.
                    // We use a defensive helper that falls back to zero if unavailable.
                    let (size_kb, line_size, associativity) = extract_cache_attrs(obj, level);

                    caches.push(CacheInfo {
                        level,
                        size_kb,
                        line_size,
                        associativity,
                        shared_cpus,
                    });
                }
            }
            caches
        }
    }

    /// Extract cache size/line/associativity from a TopologyObject.
    ///
    /// hwloc2 2.2.0 exposes `TopologyObject::cache_attributes()` which returns
    /// `Option<&TopologyCacheAttributes>`.  We call it via the known method name;
    /// if it is missing (future API change), the fallback returns safe defaults.
    fn extract_cache_attrs(obj: &hwloc2::TopologyObject, _level: u8) -> (u64, u32, u32) {
        // `cache_attributes()` is the stable method name in hwloc2 2.2.0.
        if let Some(ca) = obj.cache_attributes() {
            let size_kb = ca.size() / 1024;
            let line = ca.linesize as u32;
            let assoc = ca.associativity.unsigned_abs();
            (size_kb, line, assoc)
        } else {
            (0, 64, 0)
        }
    }

    impl HwTopology for HwlocBackend {
        fn numa_nodes(&self) -> &[NumaNode] {
            &self.numa_nodes
        }

        fn cpu_count(&self) -> usize {
            self.topology
                .objects_with_type(&ObjectType::PU)
                .map(|v| v.len())
                .unwrap_or_else(|_| num_cpus::get())
        }

        fn packages(&self) -> Vec<PackageInfo> {
            Self::extract_packages(&self.topology, &self.numa_nodes)
        }

        fn cache_info(&self) -> Vec<CacheInfo> {
            Self::extract_caches(&self.topology)
        }

        fn bind_thread(&self, cpus: &[usize]) -> Result<(), BindError> {
            if cpus.is_empty() {
                return Ok(());
            }

            // Build a CpuSet (= Bitmap) from the requested CPU indices.
            let mut cpuset = hwloc2::CpuSet::new();
            for &cpu in cpus {
                cpuset.set(cpu as u32);
            }

            // CpuBindFlags::THREAD binds the calling OS thread.
            self.topology
                .set_cpubind(cpuset, CpuBindFlags::ASSUME_SINGLE_THREAD)
                .map_err(|e| BindError(format!("hwloc bind_thread failed: {:?}", e)))
        }

        fn numa_node_for_cpu(&self, cpu: usize) -> Option<usize> {
            self.cpu_to_node.get(&cpu).copied()
        }

        fn is_hwloc_backed(&self) -> bool {
            true
        }
        fn backend_name(&self) -> &'static str {
            "hwloc2 2.2.0"
        }
    }
}

// ─── Sysfs fallback backend ───────────────────────────────────────────────────

mod sysfs_backend {
    use super::*;

    /// Fallback topology using `/sys` (Linux) or approximations on other
    /// platforms.
    pub struct SysfsBackend {
        numa: NumaTopology,
        caches: Vec<CacheInfo>,
        packages: Vec<PackageInfo>,
    }

    impl SysfsBackend {
        pub fn detect() -> Self {
            let numa = NumaTopology::detect();
            let packages = Self::build_packages(&numa);
            let caches = Self::probe_caches();
            Self {
                numa,
                caches,
                packages,
            }
        }

        fn build_packages(numa: &NumaTopology) -> Vec<PackageInfo> {
            // Without hwloc we treat each NUMA node as a separate package.
            numa.nodes
                .iter()
                .map(|n| PackageInfo {
                    id: n.id,
                    cpus: n.cpus.clone(),
                    numa_nodes: vec![n.id],
                })
                .collect()
        }

        fn probe_caches() -> Vec<CacheInfo> {
            let mut caches = Vec::new();

            #[cfg(target_os = "linux")]
            {
                let cpu_count = num_cpus::get();
                for cpu in 0..cpu_count {
                    let base = format!("/sys/devices/system/cpu/cpu{}/cache", cpu);
                    if let Ok(entries) = std::fs::read_dir(&base) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            let level = read_u64_sysfs(&path.join("level"))
                                .map(|v| v as u8)
                                .unwrap_or(0);
                            if level == 0 {
                                continue;
                            }
                            let size_kb = read_size_kb(&path.join("size")).unwrap_or(0);
                            let line_size = read_u64_sysfs(&path.join("coherency_line_size"))
                                .map(|v| v as u32)
                                .unwrap_or(64);
                            let shared_str = std::fs::read_to_string(path.join("shared_cpu_list"))
                                .unwrap_or_default();
                            let shared_cpus = parse_cpu_list(&shared_str);
                            // Deduplicate: only record when cpu == min(shared_cpus)
                            if shared_cpus.first() != Some(&cpu) {
                                continue;
                            }

                            caches.push(CacheInfo {
                                level,
                                size_kb,
                                line_size,
                                associativity: 0,
                                shared_cpus,
                            });
                        }
                    }
                }
                caches.sort_by_key(|c| (c.level, c.shared_cpus.first().copied().unwrap_or(0)));
            }

            // Non-Linux or empty: emit a single synthetic L3
            if caches.is_empty() {
                caches.push(CacheInfo {
                    level: 3,
                    size_kb: 8192,
                    line_size: 64,
                    associativity: 0,
                    shared_cpus: (0..num_cpus::get()).collect(),
                });
            }
            caches
        }
    }

    impl HwTopology for SysfsBackend {
        fn numa_nodes(&self) -> &[NumaNode] {
            &self.numa.nodes
        }

        fn cpu_count(&self) -> usize {
            num_cpus::get()
        }

        fn packages(&self) -> Vec<PackageInfo> {
            self.packages.clone()
        }

        fn cache_info(&self) -> Vec<CacheInfo> {
            self.caches.clone()
        }

        fn bind_thread(&self, cpus: &[usize]) -> Result<(), BindError> {
            bind_thread_sysfs(cpus)
        }

        fn numa_node_for_cpu(&self, cpu: usize) -> Option<usize> {
            self.numa.node_for_cpu(cpu)
        }

        fn is_hwloc_backed(&self) -> bool {
            false
        }

        fn backend_name(&self) -> &'static str {
            "sysfs-fallback"
        }
    }

    // ── sysfs helpers ────────────────────────────────────────────────────────

    fn read_u64_sysfs(path: &std::path::Path) -> Option<u64> {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
    }

    fn read_size_kb(path: &std::path::Path) -> Option<u64> {
        let s = std::fs::read_to_string(path).ok()?;
        let s = s.trim();
        if let Some(stripped) = s.strip_suffix('K') {
            stripped.parse().ok()
        } else if let Some(stripped) = s.strip_suffix('M') {
            stripped.parse::<u64>().ok().map(|v| v * 1024)
        } else {
            s.parse::<u64>().ok().map(|v| v / 1024)
        }
    }

    fn parse_cpu_list(s: &str) -> Vec<usize> {
        let mut cpus = Vec::new();
        for part in s.trim().split(',') {
            let part = part.trim();
            if let Some((a, b)) = part.split_once('-') {
                if let (Ok(lo), Ok(hi)) = (a.parse::<usize>(), b.parse::<usize>()) {
                    cpus.extend(lo..=hi);
                }
            } else if let Ok(n) = part.parse::<usize>() {
                cpus.push(n);
            }
        }
        cpus
    }
}

// ─── OS-level CPU binding (sysfs path) ───────────────────────────────────────

fn bind_thread_sysfs(cpus: &[usize]) -> Result<(), BindError> {
    if cpus.is_empty() {
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        use std::mem;
        unsafe {
            // Build a cpu_set_t bitmask.
            let mut cpuset: libc::cpu_set_t = mem::zeroed();
            for &cpu in cpus {
                libc::CPU_SET(cpu, &mut cpuset);
            }
            let tid = libc::pthread_self();
            let rc = libc::pthread_setaffinity_np(tid, mem::size_of::<libc::cpu_set_t>(), &cpuset);
            if rc != 0 {
                return Err(BindError(format!(
                    "pthread_setaffinity_np failed: errno={}",
                    rc
                )));
            }
        }
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        // macOS does not support arbitrary thread affinity; return a soft
        // error so callers can degrade gracefully.
        let _ = cpus;
        return Err(BindError("Thread affinity not supported on macOS".into()));
    }
    #[allow(unreachable_code)]
    Err(BindError(
        "Thread affinity not implemented on this platform".into(),
    ))
}

// ─── Provider enum ────────────────────────────────────────────────────────────

/// Selected topology backend.  Use [`TopologyProvider::detect`] to obtain an
/// instance – it automatically chooses hwloc when available, falling back to
/// sysfs otherwise.
pub struct TopologyProvider {
    inner: Box<dyn HwTopology>,
}

impl TopologyProvider {
    /// Auto-detect the best available backend.
    ///
    /// With `--features hwloc`: tries hwloc first; falls back to sysfs if
    /// hwloc initialization fails.
    /// Without the feature: always uses sysfs.
    pub fn detect() -> Self {
        #[cfg(feature = "hwloc")]
        {
            match hwloc_backend::HwlocBackend::new() {
                Ok(backend) => {
                    log::info!("Hardware topology: using hwloc2 backend");
                    return Self {
                        inner: Box::new(backend),
                    };
                }
                Err(e) => {
                    log::warn!("hwloc2 init failed ({}); falling back to sysfs", e);
                }
            }
        }
        log::info!("Hardware topology: using sysfs fallback backend");
        Self {
            inner: Box::new(sysfs_backend::SysfsBackend::detect()),
        }
    }

    /// Force sysfs backend regardless of feature flags (useful in tests).
    pub fn sysfs() -> Self {
        Self {
            inner: Box::new(sysfs_backend::SysfsBackend::detect()),
        }
    }
}

impl HwTopology for TopologyProvider {
    fn numa_nodes(&self) -> &[NumaNode] {
        self.inner.numa_nodes()
    }
    fn cpu_count(&self) -> usize {
        self.inner.cpu_count()
    }
    fn packages(&self) -> Vec<PackageInfo> {
        self.inner.packages()
    }
    fn cache_info(&self) -> Vec<CacheInfo> {
        self.inner.cache_info()
    }
    fn bind_thread(&self, cpus: &[usize]) -> Result<(), BindError> {
        self.inner.bind_thread(cpus)
    }
    fn numa_node_for_cpu(&self, cpu: usize) -> Option<usize> {
        self.inner.numa_node_for_cpu(cpu)
    }
    fn is_hwloc_backed(&self) -> bool {
        self.inner.is_hwloc_backed()
    }
    fn backend_name(&self) -> &'static str {
        self.inner.backend_name()
    }
}

// ─── Worker affinity helper ───────────────────────────────────────────────────

/// Assigns worker threads to hardware resources based on topology.
pub struct HwlocWorkerAffinity {
    topology: TopologyProvider,
    strategy: AffinityStrategy,
    num_workers: usize,
}

/// Strategy for distributing workers across hardware resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AffinityStrategy {
    /// No binding (OS decides).
    None,
    /// One worker per logical CPU, round-robin across NUMA nodes.
    NUMARoundRobin,
    /// Fill NUMA nodes densely before moving to the next.
    NUMADense,
    /// One worker per physical core (ignores HT siblings).
    PhysicalCores,
    /// Pin workers to L3 cache domains.
    L3CacheDomain,
}

impl HwlocWorkerAffinity {
    pub fn new(topology: TopologyProvider, strategy: AffinityStrategy, num_workers: usize) -> Self {
        Self {
            topology,
            strategy,
            num_workers,
        }
    }

    /// Compute the CPU set for worker `worker_id`.
    pub fn cpus_for_worker(&self, worker_id: usize) -> Vec<usize> {
        match self.strategy {
            AffinityStrategy::None => Vec::new(),

            AffinityStrategy::NUMARoundRobin => {
                let nodes = self.topology.numa_nodes();
                if nodes.is_empty() {
                    return Vec::new();
                }
                let node_idx = worker_id % nodes.len();
                let node = &nodes[node_idx];
                let local_idx = worker_id / nodes.len();
                node.cpus
                    .get(local_idx % node.cpus.len().max(1))
                    .map(|&cpu| vec![cpu])
                    .unwrap_or_default()
            }

            AffinityStrategy::NUMADense => {
                let nodes = self.topology.numa_nodes();
                let mut offset = 0;
                for node in nodes {
                    if worker_id < offset + node.cpus.len() {
                        let local = worker_id - offset;
                        return node.cpus.get(local).map(|&c| vec![c]).unwrap_or_default();
                    }
                    offset += node.cpus.len();
                }
                Vec::new()
            }

            AffinityStrategy::PhysicalCores => {
                // Approximate: take every other CPU (skip HT sibling).
                let all_cpus: Vec<usize> = (0..self.topology.cpu_count()).collect();
                let physical: Vec<usize> = all_cpus
                    .iter()
                    .step_by(2) // heuristic: even indices = physical cores
                    .copied()
                    .collect();
                physical
                    .get(worker_id % physical.len().max(1))
                    .map(|&c| vec![c])
                    .unwrap_or_default()
            }

            AffinityStrategy::L3CacheDomain => {
                let caches: Vec<_> = self
                    .topology
                    .cache_info()
                    .into_iter()
                    .filter(|c| c.level == 3)
                    .collect();
                if caches.is_empty() {
                    return Vec::new();
                }
                let domain = &caches[worker_id % caches.len()];
                domain.shared_cpus.clone()
            }
        }
    }

    /// Pin the current thread for `worker_id`.
    pub fn pin_current_thread(&self, worker_id: usize) -> Result<(), BindError> {
        let cpus = self.cpus_for_worker(worker_id);
        if cpus.is_empty() {
            return Ok(());
        }
        self.topology.bind_thread(&cpus)
    }

    /// Summary string for logging.
    pub fn describe(&self) -> String {
        format!(
            "HwlocWorkerAffinity {{ backend={}, strategy={:?}, workers={}, numa_nodes={}, cpus={} }}",
            self.topology.backend_name(),
            self.strategy,
            self.num_workers,
            self.topology.numa_nodes().len(),
            self.topology.cpu_count(),
        )
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_returns_valid_topology() {
        let topo = TopologyProvider::detect();
        assert!(topo.cpu_count() >= 1);
        assert!(!topo.numa_nodes().is_empty());
        // Cache info: at least one cache level should be present.
        let caches = topo.cache_info();
        assert!(!caches.is_empty(), "expected at least one cache entry");
    }

    #[test]
    fn sysfs_backend_sane() {
        let topo = TopologyProvider::sysfs();
        assert_eq!(topo.backend_name(), "sysfs-fallback");
        assert!(!topo.is_hwloc_backed());
        assert!(topo.cpu_count() >= 1);
    }

    #[test]
    fn packages_non_empty() {
        let topo = TopologyProvider::detect();
        let pkgs = topo.packages();
        assert!(!pkgs.is_empty());
        // Every package must have at least one CPU.
        for pkg in &pkgs {
            assert!(!pkg.cpus.is_empty(), "package {} has no CPUs", pkg.id);
        }
    }

    #[test]
    fn cache_levels_sane() {
        let topo = TopologyProvider::detect();
        for cache in topo.cache_info() {
            assert!(
                cache.level >= 1 && cache.level <= 4,
                "unexpected cache level {}",
                cache.level
            );
            assert!(
                cache.line_size >= 16,
                "suspiciously small line size {}",
                cache.line_size
            );
            assert!(
                !cache.shared_cpus.is_empty(),
                "cache L{} has no associated CPUs",
                cache.level
            );
        }
    }

    #[test]
    fn numa_node_for_cpu_consistent() {
        let topo = TopologyProvider::detect();
        for node in topo.numa_nodes() {
            for &cpu in &node.cpus {
                let result = topo.numa_node_for_cpu(cpu);
                assert_eq!(
                    result,
                    Some(node.id),
                    "cpu {} should map to node {} but got {:?}",
                    cpu,
                    node.id,
                    result
                );
            }
        }
    }

    #[test]
    fn worker_affinity_numa_round_robin() {
        let topo = TopologyProvider::detect();
        let affinity = HwlocWorkerAffinity::new(topo, AffinityStrategy::NUMARoundRobin, 8);
        for w in 0..8 {
            // Just verify it doesn't panic; CPU sets may be empty on UMA.
            let _cpus = affinity.cpus_for_worker(w);
        }
    }

    #[test]
    fn worker_affinity_l3_domain() {
        let topo = TopologyProvider::detect();
        let affinity = HwlocWorkerAffinity::new(topo, AffinityStrategy::L3CacheDomain, 4);
        for w in 0..4 {
            let cpus = affinity.cpus_for_worker(w);
            // L3 domain must contain at least the one CPU.
            if !cpus.is_empty() {
                for &cpu in &cpus {
                    assert!(
                        cpu < affinity.topology.cpu_count(),
                        "CPU index {} out of range",
                        cpu
                    );
                }
            }
        }
    }

    #[test]
    fn bind_thread_none_strategy_is_noop() {
        let topo = TopologyProvider::detect();
        let affinity = HwlocWorkerAffinity::new(topo, AffinityStrategy::None, 4);
        // Should always succeed (no-op).
        assert!(affinity.pin_current_thread(0).is_ok());
    }
}
