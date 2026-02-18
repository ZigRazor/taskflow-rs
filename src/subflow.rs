use std::sync::{Arc, Mutex};
use crate::task::{TaskHandle, TaskNode, TaskWork, TaskId};

/// Subflow - A nested task graph created within a task
pub struct Subflow {
    graph: Arc<Mutex<Vec<TaskNode>>>,
    next_id: Arc<Mutex<TaskId>>,
}

impl Subflow {
    pub(crate) fn new(graph: Arc<Mutex<Vec<TaskNode>>>, next_id: Arc<Mutex<TaskId>>) -> Self {
        Self { graph, next_id }
    }

    /// Create a static task in the subflow
    pub fn emplace<F>(&mut self, work: F) -> TaskHandle
    where
        F: FnOnce() + Send + 'static,
    {
        let id = {
            let mut next_id = self.next_id.lock().unwrap();
            let id = *next_id;
            *next_id += 1;
            id
        };

        let node = TaskNode::new(id, TaskWork::Static(Box::new(work)));
        self.graph.lock().unwrap().push(node);

        TaskHandle::new(id, Arc::clone(&self.graph))
    }

    /// Create a nested subflow task
    pub fn emplace_subflow<F>(&mut self, work: F) -> TaskHandle
    where
        F: FnOnce(&mut Subflow) + Send + 'static,
    {
        let id = {
            let mut next_id = self.next_id.lock().unwrap();
            let id = *next_id;
            *next_id += 1;
            id
        };

        let node = TaskNode::new(id, TaskWork::Subflow(Box::new(work)));
        self.graph.lock().unwrap().push(node);

        TaskHandle::new(id, Arc::clone(&self.graph))
    }
}
