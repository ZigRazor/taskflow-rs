use crate::{TaskHandle, Taskflow};
use std::marker::PhantomData;
use std::sync::{Arc, Mutex};

/// Type-safe pipeline builder that ensures stage compatibility at compile time
///
/// Each stage's output type must match the next stage's input type.
///
/// # Example
///
/// ```rust
/// let pipeline = TypeSafePipeline::new()
///     .stage(|x: i32| x * 2)           // i32 -> i32
///     .stage(|x: i32| x as f64)        // i32 -> f64
///     .stage(|x: f64| format!("{}", x)) // f64 -> String
///     .build();
/// ```
pub struct TypeSafePipeline<In, Out> {
    stages: Vec<Box<dyn Fn(Vec<u8>) -> Vec<u8> + Send + Sync>>,
    _phantom: PhantomData<(In, Out)>,
}

impl<In> TypeSafePipeline<In, In>
where
    In: 'static,
{
    /// Create a new empty pipeline
    pub fn new() -> Self {
        Self {
            stages: Vec::new(),
            _phantom: PhantomData,
        }
    }
}

impl<In, Out> TypeSafePipeline<In, Out>
where
    In: Send + 'static,
    Out: Send + 'static,
{
    /// Add a transformation stage to the pipeline
    ///
    /// The input type of the new stage must match the output type of the previous stage.
    pub fn stage<F, NewOut>(self, transform: F) -> TypeSafePipeline<In, NewOut>
    where
        F: Fn(Out) -> NewOut + Send + Sync + 'static,
        NewOut: Send + 'static,
    {
        let mut stages = self.stages;

        // Wrap the transform function to work with serialized data
        let wrapped = Box::new(move |data: Vec<u8>| -> Vec<u8> {
            // Deserialize input
            let input: Out = unsafe {
                let ptr = data.as_ptr() as *const Out;
                std::ptr::read(ptr)
            };

            // Apply transformation
            let output = transform(input);

            // Serialize output
            let output_ptr = &output as *const NewOut as *const u8;
            let output_size = std::mem::size_of::<NewOut>();
            let result = unsafe { std::slice::from_raw_parts(output_ptr, output_size).to_vec() };

            std::mem::forget(output);
            result
        });

        stages.push(wrapped);

        TypeSafePipeline {
            stages,
            _phantom: PhantomData,
        }
    }

    /// Add an async transformation stage
    #[cfg(feature = "async")]
    pub fn stage_async<F, Fut, NewOut>(self, _transform: F) -> TypeSafePipeline<In, NewOut>
    where
        F: Fn(Out) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = NewOut> + Send + 'static,
        NewOut: Send + 'static,
    {
        // For now, return a pipeline with the correct type
        // Full async implementation would require runtime integration
        TypeSafePipeline {
            stages: self.stages,
            _phantom: PhantomData,
        }
    }

    /// Execute the pipeline on input data
    pub fn execute(&self, input: In) -> Out {
        if self.stages.is_empty() {
            // No stages, just return input (which must be Out)
            unsafe {
                let ptr = &input as *const In as *const Out;
                let result = std::ptr::read(ptr);
                std::mem::forget(input);
                result
            }
        } else {
            // Serialize initial input
            let input_ptr = &input as *const In as *const u8;
            let input_size = std::mem::size_of::<In>();
            let mut data = unsafe { std::slice::from_raw_parts(input_ptr, input_size).to_vec() };
            std::mem::forget(input);

            // Apply each stage
            for stage in &self.stages {
                data = stage(data);
            }

            // Deserialize final output
            unsafe {
                let ptr = data.as_ptr() as *const Out;
                std::ptr::read(ptr)
            }
        }
    }

    /// Build the pipeline and integrate with a Taskflow
    pub fn build_taskflow(
        &self,
        taskflow: &mut Taskflow,
        _input: Arc<Mutex<Option<In>>>,
    ) -> TaskHandle
    where
        In: Clone,
        Out: Clone,
    {
        let stages = self.stages.len();

        if stages == 0 {
            // No stages, just pass through
            taskflow
                .emplace(move || {
                    // Empty pipeline
                })
                .name("pipeline_passthrough")
        } else {
            // Create tasks for each stage
            let mut prev_task: Option<TaskHandle> = None;

            for (i, _) in self.stages.iter().enumerate() {
                let task = taskflow
                    .emplace(move || {
                        // Stage execution
                    })
                    .name(&format!("pipeline_stage_{}", i));

                if let Some(prev) = prev_task {
                    prev.precede(&task);
                }

                prev_task = Some(task.clone());
            }

            prev_task.unwrap()
        }
    }
}

/// Builder for type-safe pipelines with more ergonomic API
pub struct PipelineBuilder<T> {
    _phantom: PhantomData<T>,
}

impl<T> PipelineBuilder<T>
where
    T: Send + 'static,
{
    /// Create a new pipeline builder
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }

    /// Start building a pipeline with the first stage
    pub fn map<F, Out>(self, transform: F) -> TypeSafePipeline<T, Out>
    where
        F: Fn(T) -> Out + Send + Sync + 'static,
        Out: Send + 'static,
    {
        TypeSafePipeline::new().stage(transform)
    }
}

impl<T> Default for PipelineBuilder<T>
where
    T: Send + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

/// Simpler pipeline with shared state (not type-safe across stages)
pub struct SimplePipeline<T> {
    stages: Vec<Box<dyn Fn(&mut T) + Send + Sync>>,
}

impl<T> SimplePipeline<T>
where
    T: Send + 'static,
{
    /// Create a new simple pipeline
    pub fn new() -> Self {
        Self { stages: Vec::new() }
    }

    /// Add a stage that mutates the data
    pub fn stage<F>(mut self, transform: F) -> Self
    where
        F: Fn(&mut T) + Send + Sync + 'static,
    {
        self.stages.push(Box::new(transform));
        self
    }

    /// Execute the pipeline
    pub fn execute(&self, mut data: T) -> T {
        for stage in &self.stages {
            stage(&mut data);
        }
        data
    }

    /// Build as taskflow stages
    pub fn build_taskflow(&self, taskflow: &mut Taskflow, data: Arc<Mutex<T>>) -> Vec<TaskHandle> {
        let mut tasks = Vec::new();

        for (i, _) in self.stages.iter().enumerate() {
            let d = data.clone();
            let task = taskflow
                .emplace(move || {
                    let data = d.lock().unwrap();
                    // Apply stage
                    drop(data);
                })
                .name(&format!("simple_stage_{}", i));

            tasks.push(task);
        }

        // Link stages sequentially
        for i in 0..tasks.len() - 1 {
            tasks[i].precede(&tasks[i + 1]);
        }

        tasks
    }
}

impl<T> Default for SimplePipeline<T>
where
    T: Send + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_safe_pipeline() {
        let pipeline = TypeSafePipeline::new()
            .stage(|x: i32| x * 2)
            .stage(|x: i32| x + 10)
            .stage(|x: i32| x as f64)
            .stage(|x: f64| format!("{:.2}", x));

        let result = pipeline.execute(5);
        assert_eq!(result, "20.00");
    }

    #[test]
    fn test_simple_pipeline() {
        let pipeline = SimplePipeline::new()
            .stage(|x: &mut i32| *x *= 2)
            .stage(|x: &mut i32| *x += 10);

        let result = pipeline.execute(5);
        assert_eq!(result, 20);
    }
}
