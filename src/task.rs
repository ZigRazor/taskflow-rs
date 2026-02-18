use std::sync::{Arc, Mutex};
use std::collections::HashSet;

pub type TaskId = usize;

/// Task work - either a static closure or a subflow
pub enum TaskWork {
    Static(Box<dyn FnOnce() + Send + 'static>),
    Subflow(Box<dyn FnOnce(&mut crate::Subflow) + Send + 'static>),
    Condition(Box<dyn FnOnce() -> usize + Send + 'static>),
}

/// Internal task node structure
pub struct TaskNode {
    pub id: TaskId,
    pub name: String,
    pub work: Option<TaskWork>,
    pub successors: HashSet<TaskId>,
    pub dependents: HashSet<TaskId>,
    pub num_dependents: usize,
}

impl TaskNode {
    pub fn new(id: TaskId, work: TaskWork) -> Self {
        Self {
            id,
            name: format!("task_{}", id),
            work: Some(work),
            successors: HashSet::new(),
            dependents: HashSet::new(),
            num_dependents: 0,
        }
    }
}

/// Task handle for building task graphs
#[derive(Clone)]
pub struct TaskHandle {
    id: TaskId,
    graph: Arc<Mutex<Vec<TaskNode>>>,
}

impl TaskHandle {
    pub(crate) fn new(id: TaskId, graph: Arc<Mutex<Vec<TaskNode>>>) -> Self {
        Self { id, graph }
    }

    /// Set the task name
    pub fn name(self, name: &str) -> Self {
        {
            let mut graph = self.graph.lock().unwrap();
            if let Some(node) = graph.iter_mut().find(|n| n.id == self.id) {
                node.name = name.to_string();
            }
        } // Lock is dropped here
        self
    }

    /// Make this task precede another task (this -> other)
    pub fn precede(&self, other: &TaskHandle) {
        let mut graph = self.graph.lock().unwrap();
        
        // Add edge from self to other
        if let Some(node) = graph.iter_mut().find(|n| n.id == self.id) {
            node.successors.insert(other.id);
        }
        
        // Update other's dependents
        if let Some(node) = graph.iter_mut().find(|n| n.id == other.id) {
            node.dependents.insert(self.id);
        }
    }

    /// Make this task succeed another task (other -> this)
    pub fn succeed(&self, other: &TaskHandle) {
        other.precede(self);
    }

    pub(crate) fn get_id(&self) -> TaskId {
        self.id
    }
}

/// Task builder trait for creating tasks with different types of work
pub trait Task {
    fn id(&self) -> TaskId;
}

impl Task for TaskHandle {
    fn id(&self) -> TaskId {
        self.id
    }
}
