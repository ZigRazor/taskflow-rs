use std::sync::Arc;
use std::collections::HashMap;
use crate::taskflow::Taskflow;
use crate::task::{TaskHandle, TaskId};

/// Cloneable work wrapper for composition
#[derive(Clone)]
pub struct CloneableWork {
    work: Arc<dyn Fn() + Send + Sync>,
}

impl CloneableWork {
    /// Create new cloneable work
    pub fn new<F>(f: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        Self {
            work: Arc::new(f),
        }
    }
    
    /// Execute the work
    pub fn execute(&self) {
        (self.work)()
    }
}

/// Parameters for parameterized compositions
#[derive(Clone)]
pub struct CompositionParams {
    params: HashMap<String, ParamValue>,
}

impl CompositionParams {
    /// Create new empty parameters
    pub fn new() -> Self {
        Self {
            params: HashMap::new(),
        }
    }
    
    /// Set an integer parameter
    pub fn set_int(&mut self, key: &str, value: i64) -> &mut Self {
        self.params.insert(key.to_string(), ParamValue::Int(value));
        self
    }
    
    /// Set a string parameter
    pub fn set_string(&mut self, key: &str, value: String) -> &mut Self {
        self.params.insert(key.to_string(), ParamValue::String(value));
        self
    }
    
    /// Set a float parameter
    pub fn set_float(&mut self, key: &str, value: f64) -> &mut Self {
        self.params.insert(key.to_string(), ParamValue::Float(value));
        self
    }
    
    /// Set a boolean parameter
    pub fn set_bool(&mut self, key: &str, value: bool) -> &mut Self {
        self.params.insert(key.to_string(), ParamValue::Bool(value));
        self
    }
    
    /// Get an integer parameter
    pub fn get_int(&self, key: &str) -> Option<i64> {
        match self.params.get(key) {
            Some(ParamValue::Int(v)) => Some(*v),
            _ => None,
        }
    }
    
    /// Get a string parameter
    pub fn get_string(&self, key: &str) -> Option<&str> {
        match self.params.get(key) {
            Some(ParamValue::String(v)) => Some(v.as_str()),
            _ => None,
        }
    }
    
    /// Get a float parameter
    pub fn get_float(&self, key: &str) -> Option<f64> {
        match self.params.get(key) {
            Some(ParamValue::Float(v)) => Some(*v),
            _ => None,
        }
    }
    
    /// Get a boolean parameter
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        match self.params.get(key) {
            Some(ParamValue::Bool(v)) => Some(*v),
            _ => None,
        }
    }
}

impl Default for CompositionParams {
    fn default() -> Self {
        Self::new()
    }
}

/// Parameter value types
#[derive(Clone, Debug)]
pub enum ParamValue {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
}

/// Parameterized composition builder
pub type CompositionFactory = Arc<dyn Fn(&CompositionParams) -> Composition + Send + Sync>;

/// Builder for parameterized compositions
pub struct ParameterizedComposition {
    factory: CompositionFactory,
    default_params: CompositionParams,
}

impl ParameterizedComposition {
    /// Create a new parameterized composition
    pub fn new<F>(factory: F) -> Self
    where
        F: Fn(&CompositionParams) -> Composition + Send + Sync + 'static,
    {
        Self {
            factory: Arc::new(factory),
            default_params: CompositionParams::new(),
        }
    }
    
    /// Set default parameters
    pub fn with_defaults(mut self, params: CompositionParams) -> Self {
        self.default_params = params;
        self
    }
    
    /// Instantiate with parameters
    pub fn instantiate(&self, params: &CompositionParams) -> Composition {
        (self.factory)(params)
    }
    
    /// Instantiate with default parameters
    pub fn instantiate_default(&self) -> Composition {
        (self.factory)(&self.default_params)
    }
    
    /// Compose into a taskflow with parameters
    pub fn compose_into(&self, target: &mut Taskflow, params: &CompositionParams) -> ComposedInstance {
        let composition = self.instantiate(params);
        composition.compose_into(target)
    }
}

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
    cloneable_works: HashMap<TaskId, CloneableWork>,
}

impl CompositionBuilder {
    /// Create a new composition builder
    pub fn new() -> Self {
        Self {
            taskflow: Taskflow::new(),
            entry_tasks: Vec::new(),
            exit_tasks: Vec::new(),
            cloneable_works: HashMap::new(),
        }
    }
    
    /// Get a mutable reference to the internal taskflow
    pub fn taskflow_mut(&mut self) -> &mut Taskflow {
        &mut self.taskflow
    }
    
    /// Add a cloneable task
    pub fn emplace_cloneable<F>(&mut self, work: F) -> TaskHandle
    where
        F: Fn() + Send + Sync + 'static,
    {
        let cloneable = CloneableWork::new(work);
        let cloneable_clone = cloneable.clone();
        
        let task = self.taskflow.emplace(move || {
            cloneable_clone.execute();
        });
        
        self.cloneable_works.insert(task.id, cloneable);
        task
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
            cloneable_works: self.cloneable_works,
        }
    }
}

impl Default for CompositionBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// A reusable task graph composition
pub struct Composition {
    taskflow: Taskflow,
    entry_tasks: Vec<TaskHandle>,
    exit_tasks: Vec<TaskHandle>,
    cloneable_works: HashMap<TaskId, CloneableWork>,
}

impl Composition {
    /// Create a new composition from a taskflow
    pub fn new(taskflow: Taskflow, entries: Vec<TaskHandle>, exits: Vec<TaskHandle>) -> Self {
        Self {
            taskflow,
            entry_tasks: entries,
            exit_tasks: exits,
            cloneable_works: HashMap::new(),
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
        // Copy all tasks from this composition into the target
        let id_mapping = self.clone_graph_into(target);
        
        // Map entry and exit points
        let new_entries = self.entry_tasks.iter()
            .filter_map(|h| id_mapping.get(&h.id).copied())
            .map(|id| TaskHandle::new(id, target.get_graph()))
            .collect::<Vec<_>>();
            
        let new_exits = self.exit_tasks.iter()
            .filter_map(|h| id_mapping.get(&h.id).copied())
            .map(|id| TaskHandle::new(id, target.get_graph()))
            .collect::<Vec<_>>();
        
        ComposedInstance {
            entries: new_entries,
            exits: new_exits,
        }
    }
    
    /// Clone the internal graph into a target taskflow
    fn clone_graph_into(&self, target: &mut Taskflow) -> HashMap<TaskId, TaskId> {
        let source_graph = self.taskflow.get_graph();
        let source_guard = source_graph.lock().unwrap();
        
        let mut id_mapping = HashMap::new();
        
        // First pass: create all tasks with their work
        for node in source_guard.iter() {
            let new_task = if let Some(cloneable) = self.cloneable_works.get(&node.id) {
                // Clone the work
                let work = cloneable.clone();
                target.emplace(move || {
                    work.execute();
                })
            } else {
                // Placeholder for non-cloneable work
                let node_name = node.name.clone();
                target.emplace(move || {
                    println!("     [Composed: {}]", node_name);
                })
            };
            
            let new_id = new_task.id;
            id_mapping.insert(node.id, new_id);
            
            // Copy name
            let target_graph_arc = target.get_graph();
            let mut target_graph = target_graph_arc.lock().unwrap();
            if let Some(new_node) = target_graph.iter_mut().find(|n| n.id == new_id) {
                new_node.name = node.name.clone();
            }
        }
        
        // Second pass: recreate dependencies
        for node in source_guard.iter() {
            let new_id = id_mapping[&node.id];
            let new_handle = TaskHandle::new(new_id, target.get_graph());
            
            for succ_id in &node.successors {
                if let Some(&new_succ_id) = id_mapping.get(succ_id) {
                    let succ_handle = TaskHandle::new(new_succ_id, target.get_graph());
                    new_handle.precede(&succ_handle);
                }
            }
        }
        
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
    
    #[test]
    fn test_cloneable_work() {
        let counter = Arc::new(std::sync::Mutex::new(0));
        let c = counter.clone();
        
        let work = CloneableWork::new(move || {
            *c.lock().unwrap() += 1;
        });
        
        work.execute();
        work.execute();
        
        assert_eq!(*counter.lock().unwrap(), 2);
    }
    
    #[test]
    fn test_parameterized_composition() {
        let param_comp = ParameterizedComposition::new(|params| {
            let mut builder = CompositionBuilder::new();
            
            let count = params.get_int("count").unwrap_or(3);
            
            let mut tasks = Vec::new();
            for i in 0..count {
                let task = builder.emplace_cloneable(move || {
                    println!("Task {}", i);
                });
                tasks.push(task);
            }
            
            if !tasks.is_empty() {
                builder.mark_entries(&[tasks[0].clone()]);
                builder.mark_exits(&[tasks.last().unwrap().clone()]);
            }
            
            builder.build()
        });
        
        let mut params = CompositionParams::new();
        params.set_int("count", 5);
        
        let composition = param_comp.instantiate(&params);
        assert!(!composition.entries().is_empty());
    }
}
