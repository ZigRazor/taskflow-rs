use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};

/// Represents a data item flowing through the pipeline
pub struct Token<T> {
    pub data: T,
    pub index: usize,
}

impl<T> Token<T> {
    pub fn new(data: T, index: usize) -> Self {
        Self { data, index }
    }
}

/// Pipeline stage type
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StageType {
    /// Serial stage - processes one token at a time
    Serial,
    /// Parallel stage - processes multiple tokens concurrently
    Parallel(usize), // number of parallel workers
}

/// Concurrent pipeline for stream processing
#[derive(Clone)]
pub struct ConcurrentPipeline<T> {
    queue: Arc<Mutex<VecDeque<Token<T>>>>,
    buffer_size: usize,
    tokens_in_flight: Arc<AtomicUsize>,
    max_tokens: usize,
    stopped: Arc<Mutex<bool>>,
    condvar: Arc<Condvar>,
}

impl<T: Send + Clone + 'static> ConcurrentPipeline<T> {
    pub fn new(buffer_size: usize, max_tokens: usize) -> Self {
        Self {
            queue: Arc::new(Mutex::new(VecDeque::new())),
            buffer_size,
            tokens_in_flight: Arc::new(AtomicUsize::new(0)),
            max_tokens,
            stopped: Arc::new(Mutex::new(false)),
            condvar: Arc::new(Condvar::new()),
        }
    }

    /// Push data into the pipeline
    pub fn push(&self, data: T) -> Result<(), &'static str> {
        // Check if we're at capacity
        if self.tokens_in_flight.load(Ordering::Acquire) >= self.max_tokens {
            return Err("Pipeline at capacity");
        }

        let mut queue = self.queue.lock().unwrap();

        // Backpressure: wait if buffer is full
        while queue.len() >= self.buffer_size {
            queue = self.condvar.wait(queue).unwrap();
        }

        let index = self.tokens_in_flight.fetch_add(1, Ordering::AcqRel);
        queue.push_back(Token::new(data, index));
        self.condvar.notify_all();

        Ok(())
    }

    /// Try to pop processed data from the pipeline
    pub fn try_pop(&self) -> Option<Token<T>> {
        let mut queue = self.queue.lock().unwrap();
        let token = queue.pop_front();

        if token.is_some() {
            self.tokens_in_flight.fetch_sub(1, Ordering::AcqRel);
            self.condvar.notify_all();
        }

        token
    }

    /// Stop the pipeline
    pub fn stop(&self) {
        *self.stopped.lock().unwrap() = true;
        self.condvar.notify_all();
    }

    /// Check if pipeline is stopped
    pub fn is_stopped(&self) -> bool {
        *self.stopped.lock().unwrap()
    }

    /// Get number of tokens currently in flight
    pub fn tokens_in_flight(&self) -> usize {
        self.tokens_in_flight.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_concurrent_pipeline() {
        let pipeline = ConcurrentPipeline::new(10, 100);

        assert_eq!(pipeline.tokens_in_flight(), 0);

        pipeline.push(42).unwrap();
        assert_eq!(pipeline.tokens_in_flight(), 1);

        let token = pipeline.try_pop();
        assert!(token.is_some());
        assert_eq!(token.unwrap().data, 42);
        assert_eq!(pipeline.tokens_in_flight(), 0);
    }

    #[test]
    fn test_backpressure() {
        let pipeline = ConcurrentPipeline::new(2, 10);

        // Should succeed for first 2
        assert!(pipeline.push(1).is_ok());
        assert!(pipeline.push(2).is_ok());

        // Buffer is full, would need to block
        // In real use, push would wait on condvar
    }
}
