use crate::task::{TaskHandle, TaskId};
use std::collections::HashMap;

/// Result of a condition evaluation - can branch to multiple paths
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BranchId(pub usize);

impl From<usize> for BranchId {
    fn from(id: usize) -> Self {
        BranchId(id)
    }
}

impl From<i32> for BranchId {
    fn from(id: i32) -> Self {
        BranchId(id as usize)
    }
}

/// A conditional task handle that supports multi-way branching
pub struct ConditionalHandle {
    pub(crate) task: TaskHandle,
    pub(crate) branches: HashMap<BranchId, Vec<TaskHandle>>,
}

impl ConditionalHandle {
    /// Create a new conditional handle from a task
    pub fn new(task: TaskHandle) -> Self {
        Self {
            task,
            branches: HashMap::new(),
        }
    }
    
    /// Add a successor for a specific branch
    pub fn branch(&mut self, branch_id: impl Into<BranchId>, successor: &TaskHandle) -> &mut Self {
        let branch_id = branch_id.into();
        self.branches
            .entry(branch_id)
            .or_insert_with(Vec::new)
            .push(successor.clone());
        self
    }
    
    /// Add multiple successors for a specific branch
    pub fn branch_many(&mut self, branch_id: impl Into<BranchId>, successors: &[TaskHandle]) -> &mut Self {
        let branch_id = branch_id.into();
        self.branches
            .entry(branch_id)
            .or_insert_with(Vec::new)
            .extend(successors.iter().cloned());
        self
    }
    
    /// Get the underlying task handle
    pub fn task(&self) -> &TaskHandle {
        &self.task
    }
    
    /// Get the task ID
    pub(crate) fn task_id(&self) -> TaskId {
        self.task.id
    }
}

/// A loop construct in the task graph
pub struct Loop {
    /// The loop condition task
    pub condition: TaskHandle,
    /// The loop body tasks
    pub body: Vec<TaskHandle>,
    /// Maximum iterations (for safety)
    pub max_iterations: Option<usize>,
}

impl Loop {
    /// Create a new loop with a condition and body
    pub fn new(condition: TaskHandle) -> Self {
        Self {
            condition,
            body: Vec::new(),
            max_iterations: None,
        }
    }
    
    /// Add a task to the loop body
    pub fn add_body_task(&mut self, task: TaskHandle) -> &mut Self {
        self.body.push(task);
        self
    }
    
    /// Set maximum iterations for safety
    pub fn max_iterations(&mut self, max: usize) -> &mut Self {
        self.max_iterations = Some(max);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Taskflow;
    
    #[test]
    fn test_branch_creation() {
        let mut taskflow = Taskflow::new();
        
        let condition = taskflow.emplace(|| {}).name("condition");
        let mut cond_handle = ConditionalHandle::new(condition);
        
        let task_a = taskflow.emplace(|| {}).name("a");
        let task_b = taskflow.emplace(|| {}).name("b");
        
        cond_handle.branch(0, &task_a);
        cond_handle.branch(1, &task_b);
        
        assert_eq!(cond_handle.branches.len(), 2);
    }
}
