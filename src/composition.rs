use crate::taskflow::Taskflow;
use crate::task::{TaskHandle, TaskId};

/// A module task - a reusable task graph component with defined entry/exit points
pub struct ModuleTask {
    name: String,
    entry_points: Vec<TaskHandle>,
    exit_points: Vec<TaskHandle>,
}

impl ModuleTask {
    /// Create a new module task
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            entry_points: Vec::new(),
            exit_points: Vec::new(),
        }
    }
    
    /// Get the entry points (tasks that can receive dependencies from outside)
    pub fn entries(&self) -> &[TaskHandle] {
        &self.entry_points
    }
    
    /// Get the exit points (tasks that can provide dependencies to outside)
    pub fn exits(&self) -> &[TaskHandle] {
        &self.exit_points
    }
    
    /// Get the name of this module
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Composition builder for creating reusable task graph templates
pub struct CompositionBuilder {
    taskflow: Taskflow,
    entry_tasks: Vec<TaskHandle>,
    exit_tasks: Vec<TaskHandle>,
}

impl CompositionBuilder {
    /// Create a new composition builder
    pub fn new() -> Self {
        Self {
            taskflow: Taskflow::new(),
            entry_tasks: Vec::new(),
            exit_tasks: Vec::new(),
        }
    }
    
    /// Get a mutable reference to the internal taskflow
    pub fn taskflow_mut(&mut self) -> &mut Taskflow {
        &mut self.taskflow
    }
    
    /// Mark tasks as entry points
    pub fn mark_entries(&mut self, tasks: &[TaskHandle]) {
        self.entry_tasks.extend_from_slice(tasks);
    }
    
    /// Mark tasks as exit points
    pub fn mark_exits(&mut self, tasks: &[TaskHandle]) {
        self.exit_tasks.extend_from_slice(tasks);
    }
    
    /// Build the composition
    pub fn build(self) -> Composition {
        Composition {
            taskflow: self.taskflow,
            entry_tasks: self.entry_tasks,
            exit_tasks: self.exit_tasks,
        }
    }
}

/// A reusable task graph composition
pub struct Composition {
    taskflow: Taskflow,
    entry_tasks: Vec<TaskHandle>,
    exit_tasks: Vec<TaskHandle>,
}

impl Composition {
    /// Create a new composition from a taskflow
    pub fn new(taskflow: Taskflow, entries: Vec<TaskHandle>, exits: Vec<TaskHandle>) -> Self {
        Self {
            taskflow,
            entry_tasks: entries,
            exit_tasks: exits,
        }
    }
    
    /// Get entry points
    pub fn entries(&self) -> &[TaskHandle] {
        &self.entry_tasks
    }
    
    /// Get exit points
    pub fn exits(&self) -> &[TaskHandle] {
        &self.exit_tasks
    }
    
    /// Compose this into another taskflow
    pub fn compose_into(&self, target: &mut Taskflow) -> ComposedInstance {
        eprintln!("\nDEBUG compose_into:");
        eprintln!("  Entry task IDs in composition: {:?}", 
                 self.entry_tasks.iter().map(|h| h.id).collect::<Vec<_>>());
        eprintln!("  Exit task IDs in composition: {:?}", 
                 self.exit_tasks.iter().map(|h| h.id).collect::<Vec<_>>());
        
        // Copy all tasks from this composition into the target
        let id_mapping = self.clone_graph_into(target);
        
        eprintln!("  ID mapping: {:?}", id_mapping);
        
        // Map entry and exit points
        let new_entries = self.entry_tasks.iter()
            .filter_map(|h| {
                let mapped = id_mapping.get(&h.id).copied();
                eprintln!("  Mapping entry {} -> {:?}", h.id, mapped);
                mapped
            })
            .map(|id| {
                eprintln!("    Creating handle for entry task {}", id);
                TaskHandle::new(id, target.get_graph())
            })
            .collect::<Vec<_>>();
            
        let new_exits = self.exit_tasks.iter()
            .filter_map(|h| {
                let mapped = id_mapping.get(&h.id).copied();
                eprintln!("  Mapping exit {} -> {:?}", h.id, mapped);
                mapped
            })
            .map(|id| {
                eprintln!("    Creating handle for exit task {}", id);
                TaskHandle::new(id, target.get_graph())
            })
            .collect::<Vec<_>>();
        
        eprintln!("  Result: {} entries, {} exits\n", new_entries.len(), new_exits.len());
        
        // Debug: warn if entries/exits are empty
        if new_entries.is_empty() {
            eprintln!("WARNING: Composed instance has no entries! Original had {} entry IDs: {:?}",
                     self.entry_tasks.len(),
                     self.entry_tasks.iter().map(|h| h.id).collect::<Vec<_>>());
            eprintln!("  id_mapping keys: {:?}", id_mapping.keys().collect::<Vec<_>>());
        }
        if new_exits.is_empty() {
            eprintln!("WARNING: Composed instance has no exits! Original had {} exit IDs: {:?}",
                     self.exit_tasks.len(),
                     self.exit_tasks.iter().map(|h| h.id).collect::<Vec<_>>());
        }
        
        ComposedInstance {
            entries: new_entries,
            exits: new_exits,
        }
    }
    
    /// Clone the internal graph into a target taskflow
    fn clone_graph_into(&self, target: &mut Taskflow) -> std::collections::HashMap<TaskId, TaskId> {
        use std::collections::HashMap;
        
        let source_graph = self.taskflow.get_graph();
        let source_guard = source_graph.lock().unwrap();
        
        eprintln!("DEBUG clone_graph_into: source has {} tasks", source_guard.len());
        
        let mut id_mapping = HashMap::new();
        
        // First pass: create all tasks with their work
        for node in source_guard.iter() {
            eprintln!("  Cloning task {} (name: '{}')", node.id, node.name);
            
            // Create a placeholder task with visible work
            let node_name = node.name.clone();
            let new_task = target.emplace(move || {
                // Placeholder - actual work copying is complex due to FnOnce
                // For now, just indicate the task executed
                println!("     [Composed: {}]", node_name);
            });
            
            // Get the actual ID from the task handle
            let new_id = new_task.id;
            eprintln!("    Mapped {} -> {}", node.id, new_id);
            id_mapping.insert(node.id, new_id);
            
            // Copy name
            let target_graph_arc = target.get_graph();
            let mut target_graph = target_graph_arc.lock().unwrap();
            if let Some(new_node) = target_graph.iter_mut().find(|n| n.id == new_id) {
                new_node.name = node.name.clone();
            }
        }
        
        // Second pass: recreate dependencies
        eprintln!("  Recreating dependencies...");
        for node in source_guard.iter() {
            let new_id = id_mapping[&node.id];
            let new_handle = TaskHandle::new(new_id, target.get_graph());
            
            eprintln!("    Task {} (now {}) has {} successors", node.id, new_id, node.successors.len());
            for succ_id in &node.successors {
                if let Some(&new_succ_id) = id_mapping.get(succ_id) {
                    eprintln!("      {} -> {}", new_id, new_succ_id);
                    let succ_handle = TaskHandle::new(new_succ_id, target.get_graph());
                    new_handle.precede(&succ_handle);
                }
            }
        }
        
        eprintln!("  Clone complete: {} tasks cloned", id_mapping.len());
        id_mapping
    }
    
    /// Get the internal taskflow (for direct execution)
    pub fn taskflow(&self) -> &Taskflow {
        &self.taskflow
    }
}

/// A composed instance within a target taskflow
pub struct ComposedInstance {
    entries: Vec<TaskHandle>,
    exits: Vec<TaskHandle>,
}

impl ComposedInstance {
    /// Get entry points of this composed instance
    pub fn entries(&self) -> &[TaskHandle] {
        &self.entries
    }
    
    /// Get exit points of this composed instance
    pub fn exits(&self) -> &[TaskHandle] {
        &self.exits
    }
    
    /// Get a specific entry point by index
    pub fn entry(&self, index: usize) -> Option<&TaskHandle> {
        self.entries.get(index)
    }
    
    /// Get a specific exit point by index
    pub fn exit(&self, index: usize) -> Option<&TaskHandle> {
        self.exits.get(index)
    }
}

/// Helper trait for taskflow composition
pub trait TaskflowComposable {
    /// Compose another taskflow into this one
    fn compose(&mut self, other: &Composition) -> ComposedInstance;
    
    /// Compose with predecessor dependencies
    fn compose_after(&mut self, predecessors: &[TaskHandle], other: &Composition) -> ComposedInstance;
    
    /// Compose with successor dependencies
    fn compose_before(&mut self, other: &Composition, successors: &[TaskHandle]) -> ComposedInstance;
}

impl TaskflowComposable for Taskflow {
    fn compose(&mut self, other: &Composition) -> ComposedInstance {
        other.compose_into(self)
    }
    
    fn compose_after(&mut self, predecessors: &[TaskHandle], other: &Composition) -> ComposedInstance {
        let instance = other.compose_into(self);
        
        // Connect predecessors to entries
        for pred in predecessors {
            for entry in &instance.entries {
                pred.precede(entry);
            }
        }
        
        instance
    }
    
    fn compose_before(&mut self, other: &Composition, successors: &[TaskHandle]) -> ComposedInstance {
        let instance = other.compose_into(self);
        
        // Connect exits to successors
        for exit in &instance.exits {
            for succ in successors {
                exit.precede(succ);
            }
        }
        
        instance
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_composition_builder() {
        let mut builder = CompositionBuilder::new();
        let task = builder.taskflow_mut().emplace(|| {});
        builder.mark_entries(&[task.clone()]);
        builder.mark_exits(&[task]);
        
        let composition = builder.build();
        assert_eq!(composition.entries().len(), 1);
        assert_eq!(composition.exits().len(), 1);
    }
}
