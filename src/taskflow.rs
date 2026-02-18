use std::sync::{Arc, Mutex};
use crate::task::{TaskHandle, TaskNode, TaskWork, TaskId};

/// Taskflow - A task dependency graph
pub struct Taskflow {
    graph: Arc<Mutex<Vec<TaskNode>>>,
    next_id: TaskId,
}

impl Taskflow {
    /// Create a new empty taskflow
    pub fn new() -> Self {
        Self {
            graph: Arc::new(Mutex::new(Vec::new())),
            next_id: 0,
        }
    }

    /// Create a static task with a closure
    pub fn emplace<F>(&mut self, work: F) -> TaskHandle
    where
        F: FnOnce() + Send + 'static,
    {
        let id = self.next_id;
        self.next_id += 1;

        let node = TaskNode::new(id, TaskWork::Static(Box::new(work)));
        self.graph.lock().unwrap().push(node);

        TaskHandle::new(id, Arc::clone(&self.graph))
    }

    /// Create a subflow task
    pub fn emplace_subflow<F>(&mut self, work: F) -> TaskHandle
    where
        F: FnOnce(&mut crate::Subflow) + Send + 'static,
    {
        let id = self.next_id;
        self.next_id += 1;

        let node = TaskNode::new(id, TaskWork::Subflow(Box::new(work)));
        self.graph.lock().unwrap().push(node);

        TaskHandle::new(id, Arc::clone(&self.graph))
    }

    /// Create a condition task that returns which successor to execute
    pub fn emplace_condition<F>(&mut self, condition: F) -> TaskHandle
    where
        F: FnOnce() -> usize + Send + 'static,
    {
        let id = self.next_id;
        self.next_id += 1;

        let node = TaskNode::new(id, TaskWork::Condition(Box::new(condition)));
        self.graph.lock().unwrap().push(node);

        TaskHandle::new(id, Arc::clone(&self.graph))
    }

    /// Get the internal graph (for executor)
    pub(crate) fn get_graph(&self) -> Arc<Mutex<Vec<TaskNode>>> {
        Arc::clone(&self.graph)
    }

    /// Dump the taskflow to DOT format
    pub fn dump(&self) -> String {
        let graph = self.graph.lock().unwrap();
        let mut dot = String::from("digraph Taskflow {\n");

        for node in graph.iter() {
            dot.push_str(&format!("  {} [label=\"{}\"];\n", node.id, node.name));
            for succ in &node.successors {
                dot.push_str(&format!("  {} -> {};\n", node.id, succ));
            }
        }

        dot.push_str("}\n");
        dot
    }

    /// Get the number of tasks
    pub fn size(&self) -> usize {
        self.graph.lock().unwrap().len()
    }

    /// Check if the taskflow is empty
    pub fn is_empty(&self) -> bool {
        self.graph.lock().unwrap().is_empty()
    }
}

impl Default for Taskflow {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_taskflow_creation() {
        let mut tf = Taskflow::new();
        let a = tf.emplace(|| println!("A")).name("A");
        let b = tf.emplace(|| println!("B")).name("B");
        
        a.precede(&b);
        
        assert_eq!(tf.size(), 2);
        
        let dot = tf.dump();
        assert!(dot.contains("A"));
        assert!(dot.contains("B"));
        assert!(dot.contains("->"));
    }
}
