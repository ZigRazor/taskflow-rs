use crate::task::TaskId;
use std::collections::{HashMap, HashSet, VecDeque};

/// Cycle detection result
#[derive(Debug, Clone)]
pub enum CycleDetectionResult {
    /// No cycles detected - graph is a DAG
    NoCycle,
    /// Cycle detected with the path of task IDs forming the cycle
    CycleDetected(Vec<TaskId>),
}

impl CycleDetectionResult {
    /// Check if a cycle was detected
    pub fn has_cycle(&self) -> bool {
        matches!(self, CycleDetectionResult::CycleDetected(_))
    }
    
    /// Get the cycle path if one exists
    pub fn cycle_path(&self) -> Option<&[TaskId]> {
        match self {
            CycleDetectionResult::CycleDetected(path) => Some(path),
            CycleDetectionResult::NoCycle => None,
        }
    }
}

/// Cycle detector for task graphs using DFS-based detection
pub struct CycleDetector {
    /// Adjacency list: task_id -> list of successor task_ids
    adjacency: HashMap<TaskId, Vec<TaskId>>,
    /// All task IDs in the graph
    tasks: HashSet<TaskId>,
}

impl CycleDetector {
    /// Create a new cycle detector
    pub fn new() -> Self {
        Self {
            adjacency: HashMap::new(),
            tasks: HashSet::new(),
        }
    }
    
    /// Add a task to the graph
    pub fn add_task(&mut self, task_id: TaskId) {
        self.tasks.insert(task_id);
        self.adjacency.entry(task_id).or_insert_with(Vec::new);
    }
    
    /// Add a dependency: from -> to
    pub fn add_dependency(&mut self, from: TaskId, to: TaskId) {
        self.add_task(from);
        self.add_task(to);
        self.adjacency.entry(from).or_insert_with(Vec::new).push(to);
    }
    
    /// Detect cycles in the task graph using DFS
    pub fn detect_cycle(&self) -> CycleDetectionResult {
        // Track visited nodes and nodes in current DFS path
        let mut visited = HashSet::new();
        let mut in_stack = HashSet::new();
        let mut parent = HashMap::new();
        
        // Try DFS from each unvisited node
        for &task_id in &self.tasks {
            if !visited.contains(&task_id) {
                if let Some(cycle) = self.dfs_cycle_detection(
                    task_id,
                    &mut visited,
                    &mut in_stack,
                    &mut parent,
                ) {
                    return CycleDetectionResult::CycleDetected(cycle);
                }
            }
        }
        
        CycleDetectionResult::NoCycle
    }
    
    /// DFS-based cycle detection
    fn dfs_cycle_detection(
        &self,
        node: TaskId,
        visited: &mut HashSet<TaskId>,
        in_stack: &mut HashSet<TaskId>,
        parent: &mut HashMap<TaskId, TaskId>,
    ) -> Option<Vec<TaskId>> {
        visited.insert(node);
        in_stack.insert(node);
        
        if let Some(successors) = self.adjacency.get(&node) {
            for &successor in successors {
                if !visited.contains(&successor) {
                    parent.insert(successor, node);
                    if let Some(cycle) = self.dfs_cycle_detection(
                        successor,
                        visited,
                        in_stack,
                        parent,
                    ) {
                        return Some(cycle);
                    }
                } else if in_stack.contains(&successor) {
                    // Found a back edge - cycle detected
                    return Some(self.reconstruct_cycle(node, successor, parent));
                }
            }
        }
        
        in_stack.remove(&node);
        None
    }
    
    /// Reconstruct the cycle path from parent pointers
    fn reconstruct_cycle(
        &self,
        start: TaskId,
        end: TaskId,
        parent: &HashMap<TaskId, TaskId>,
    ) -> Vec<TaskId> {
        let mut cycle = vec![end];
        let mut current = start;
        
        while current != end {
            cycle.push(current);
            if let Some(&p) = parent.get(&current) {
                current = p;
            } else {
                break;
            }
        }
        
        cycle.reverse();
        cycle
    }
    
    /// Get topological sort if graph is a DAG (no cycles)
    /// Returns None if cycles are detected
    pub fn topological_sort(&self) -> Option<Vec<TaskId>> {
        // First check for cycles
        if self.detect_cycle().has_cycle() {
            return None;
        }
        
        // Kahn's algorithm for topological sort
        let mut in_degree: HashMap<TaskId, usize> = HashMap::new();
        
        // Initialize in-degrees
        for &task_id in &self.tasks {
            in_degree.entry(task_id).or_insert(0);
        }
        
        for successors in self.adjacency.values() {
            for &successor in successors {
                *in_degree.entry(successor).or_insert(0) += 1;
            }
        }
        
        // Queue of nodes with in-degree 0
        let mut queue: VecDeque<TaskId> = in_degree
            .iter()
            .filter(|(_, &degree)| degree == 0)
            .map(|(&id, _)| id)
            .collect();
        
        let mut sorted = Vec::new();
        
        while let Some(node) = queue.pop_front() {
            sorted.push(node);
            
            if let Some(successors) = self.adjacency.get(&node) {
                for &successor in successors {
                    if let Some(degree) = in_degree.get_mut(&successor) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push_back(successor);
                        }
                    }
                }
            }
        }
        
        // If we didn't visit all nodes, there's a cycle
        if sorted.len() == self.tasks.len() {
            Some(sorted)
        } else {
            None
        }
    }
    
    /// Get strongly connected components (for advanced cycle analysis)
    pub fn strongly_connected_components(&self) -> Vec<Vec<TaskId>> {
        let mut index_counter = 0;
        let mut stack = Vec::new();
        let mut indices = HashMap::new();
        let mut lowlinks = HashMap::new();
        let mut on_stack = HashSet::new();
        let mut components = Vec::new();
        
        for &task_id in &self.tasks {
            if !indices.contains_key(&task_id) {
                self.tarjan_scc(
                    task_id,
                    &mut index_counter,
                    &mut stack,
                    &mut indices,
                    &mut lowlinks,
                    &mut on_stack,
                    &mut components,
                );
            }
        }
        
        components
    }
    
    /// Tarjan's algorithm for strongly connected components
    fn tarjan_scc(
        &self,
        node: TaskId,
        index_counter: &mut usize,
        stack: &mut Vec<TaskId>,
        indices: &mut HashMap<TaskId, usize>,
        lowlinks: &mut HashMap<TaskId, usize>,
        on_stack: &mut HashSet<TaskId>,
        components: &mut Vec<Vec<TaskId>>,
    ) {
        indices.insert(node, *index_counter);
        lowlinks.insert(node, *index_counter);
        *index_counter += 1;
        stack.push(node);
        on_stack.insert(node);
        
        if let Some(successors) = self.adjacency.get(&node) {
            for &successor in successors {
                if !indices.contains_key(&successor) {
                    self.tarjan_scc(
                        successor,
                        index_counter,
                        stack,
                        indices,
                        lowlinks,
                        on_stack,
                        components,
                    );
                    let successor_lowlink = *lowlinks.get(&successor).unwrap();
                    let node_lowlink = *lowlinks.get(&node).unwrap();
                    lowlinks.insert(node, node_lowlink.min(successor_lowlink));
                } else if on_stack.contains(&successor) {
                    let successor_index = *indices.get(&successor).unwrap();
                    let node_lowlink = *lowlinks.get(&node).unwrap();
                    lowlinks.insert(node, node_lowlink.min(successor_index));
                }
            }
        }
        
        if lowlinks.get(&node) == indices.get(&node) {
            let mut component = Vec::new();
            loop {
                let w = stack.pop().unwrap();
                on_stack.remove(&w);
                component.push(w);
                if w == node {
                    break;
                }
            }
            if component.len() > 1 || self.has_self_loop(node) {
                components.push(component);
            }
        }
    }
    
    /// Check if a node has a self-loop
    fn has_self_loop(&self, node: TaskId) -> bool {
        if let Some(successors) = self.adjacency.get(&node) {
            successors.contains(&node)
        } else {
            false
        }
    }
}

impl Default for CycleDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_no_cycle() {
        let mut detector = CycleDetector::new();
        
        // Linear chain: 1 -> 2 -> 3
        detector.add_dependency(1, 2);
        detector.add_dependency(2, 3);
        
        let result = detector.detect_cycle();
        assert!(!result.has_cycle());
    }
    
    #[test]
    fn test_simple_cycle() {
        let mut detector = CycleDetector::new();
        
        // Cycle: 1 -> 2 -> 3 -> 1
        detector.add_dependency(1, 2);
        detector.add_dependency(2, 3);
        detector.add_dependency(3, 1);
        
        let result = detector.detect_cycle();
        assert!(result.has_cycle());
        
        let cycle = result.cycle_path().unwrap();
        assert!(!cycle.is_empty());
    }
    
    #[test]
    fn test_self_loop() {
        let mut detector = CycleDetector::new();
        
        // Self loop: 1 -> 1
        detector.add_dependency(1, 1);
        
        let result = detector.detect_cycle();
        assert!(result.has_cycle());
    }
    
    #[test]
    fn test_complex_graph_no_cycle() {
        let mut detector = CycleDetector::new();
        
        // Diamond: 1 -> 2 -> 4, 1 -> 3 -> 4
        detector.add_dependency(1, 2);
        detector.add_dependency(1, 3);
        detector.add_dependency(2, 4);
        detector.add_dependency(3, 4);
        
        let result = detector.detect_cycle();
        assert!(!result.has_cycle());
    }
    
    #[test]
    fn test_topological_sort() {
        let mut detector = CycleDetector::new();
        
        detector.add_dependency(1, 2);
        detector.add_dependency(2, 3);
        detector.add_dependency(1, 3);
        
        let sorted = detector.topological_sort();
        assert!(sorted.is_some());
        
        let sorted = sorted.unwrap();
        assert_eq!(sorted.len(), 3);
        
        // 1 should come before 2 and 3
        let pos_1 = sorted.iter().position(|&x| x == 1).unwrap();
        let pos_2 = sorted.iter().position(|&x| x == 2).unwrap();
        let pos_3 = sorted.iter().position(|&x| x == 3).unwrap();
        
        assert!(pos_1 < pos_2);
        assert!(pos_1 < pos_3);
        assert!(pos_2 < pos_3);
    }
    
    #[test]
    fn test_topological_sort_with_cycle() {
        let mut detector = CycleDetector::new();
        
        detector.add_dependency(1, 2);
        detector.add_dependency(2, 3);
        detector.add_dependency(3, 1);
        
        let sorted = detector.topological_sort();
        assert!(sorted.is_none());
    }
}
