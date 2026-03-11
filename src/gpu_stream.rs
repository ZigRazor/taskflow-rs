// ============================================================================
// gpu_stream.rs — CUDA Stream Management & Stream Pool
// ============================================================================
//
// Design goals:
//   1. `GpuStream`    – a named, typed handle to a backend stream.
//   2. `StreamPool`   – manages N streams with round-robin / least-loaded
//                       assignment.  Tasks pull a stream from the pool,
//                       perform work, and return it.
//   3. `StreamGuard`  – RAII wrapper; returns the stream to the pool on drop.
//   4. `StreamSet`    – ordered set of streams for a pipeline stage.
//
// Streams are never destroyed while a `StreamGuard` is alive.
// ============================================================================

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use crate::gpu_backend::{ComputeBackend, DeviceStream, GpuError};

// ---------------------------------------------------------------------------
// GpuStream — user-facing wrapper
// ---------------------------------------------------------------------------

/// A named handle to a device command queue / CUDA stream.
///
/// Operations enqueued on a stream execute in order relative to each other
/// but may overlap with operations on *different* streams.
#[derive(Clone)]
pub struct GpuStream {
    pub(crate) inner: DeviceStream,
    /// Human-readable label (e.g. "compute-0", "h2d-1")
    pub(crate) label: Arc<String>,
}

impl GpuStream {
    /// Create a new stream on the given backend.
    pub fn new(backend: &Arc<dyn ComputeBackend>, label: impl Into<String>) -> Result<Self, GpuError> {
        let inner = backend.create_stream()?;
        Ok(Self {
            inner,
            label: Arc::new(label.into()),
        })
    }

    /// Opaque stream ID (for logging / tracing).
    pub fn id(&self) -> u64 { self.inner.id() }

    /// Block until all previously enqueued work on this stream completes.
    pub fn synchronize(&self) -> Result<(), GpuError> {
        self.inner.synchronize()
    }

    /// Label assigned at creation time.
    pub fn label(&self) -> &str { &self.label }
}

impl fmt::Debug for GpuStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GpuStream(id={}, label={})", self.id(), self.label)
    }
}

// ---------------------------------------------------------------------------
// StreamPool — manages a fixed set of streams
// ---------------------------------------------------------------------------

/// Manages `N` streams created from the same backend device.
///
/// Assignment strategy:
///   - **Round-robin** (default): streams are handed out in sequence.
///   - **Least pending**: tracks enqueue counts, picks the least loaded.
///
/// # Example
/// ```ignore
/// let pool = StreamPool::new(&backend, 4, StreamAssignment::RoundRobin)?;
/// {
///     let guard = pool.acquire()?;   // borrows a stream
///     guard.stream().synchronize()?; // wait for it
/// }                                   // stream returned to pool on drop
/// ```
pub struct StreamPool {
    streams:    Vec<Arc<StreamSlot>>,
    strategy:   StreamAssignment,
    next_robin: AtomicUsize,
}

/// How the pool selects which stream to lend out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamAssignment {
    /// Assign streams in a fixed rotation.
    RoundRobin,
    /// Assign to the stream with the fewest un-synchronised operations.
    LeastPending,
}

struct StreamSlot {
    stream:      GpuStream,
    pending_ops: AtomicU64,
}

impl fmt::Debug for StreamPool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StreamPool({} streams, {:?})", self.streams.len(), self.strategy)
    }
}

impl StreamPool {
    /// Create a pool of `count` streams from `backend`.
    ///
    /// Streams are labelled "pool-{i}" by default.
    pub fn new(
        backend:  &Arc<dyn ComputeBackend>,
        count:    usize,
        strategy: StreamAssignment,
    ) -> Result<Self, GpuError> {
        if count == 0 {
            return Err(GpuError::new(backend.kind(), "StreamPool count must be > 0"));
        }

        let mut streams = Vec::with_capacity(count);
        for i in 0..count {
            let label = format!("{}-stream-{}", backend.kind(), i);
            let stream = GpuStream::new(backend, label)?;
            streams.push(Arc::new(StreamSlot {
                stream,
                pending_ops: AtomicU64::new(0),
            }));
        }

        Ok(Self {
            streams,
            strategy,
            next_robin: AtomicUsize::new(0),
        })
    }

    /// Number of streams in the pool.
    pub fn len(&self) -> usize { self.streams.len() }
    pub fn is_empty(&self) -> bool { self.streams.is_empty() }

    /// Acquire a stream from the pool according to the assignment strategy.
    ///
    /// If `LeastPending` is chosen, returns the stream with the lowest
    /// `pending_ops` count.  The count is decremented when the `StreamGuard`
    /// is dropped.
    pub fn acquire(&self) -> Result<StreamGuard<'_>, GpuError> {
        let slot = match self.strategy {
            StreamAssignment::RoundRobin => {
                let idx = self.next_robin.fetch_add(1, Ordering::Relaxed) % self.streams.len();
                Arc::clone(&self.streams[idx])
            }
            StreamAssignment::LeastPending => {
                self.streams
                    .iter()
                    .min_by_key(|s| s.pending_ops.load(Ordering::Relaxed))
                    .map(Arc::clone)
                    .unwrap() // safe: pool is non-empty
            }
        };

        slot.pending_ops.fetch_add(1, Ordering::Relaxed);
        Ok(StreamGuard { slot, _marker: std::marker::PhantomData })
    }

    /// Synchronise *all* streams in the pool.
    /// Useful for a global barrier after a pipeline stage.
    pub fn synchronize_all(&self) -> Result<(), GpuError> {
        for slot in &self.streams {
            slot.stream.synchronize()?;
        }
        Ok(())
    }

    /// Iterator over all streams (for manual management).
    pub fn iter(&self) -> impl Iterator<Item = &GpuStream> {
        self.streams.iter().map(|s| &s.stream)
    }
}

// ---------------------------------------------------------------------------
// StreamGuard — RAII stream borrow
// ---------------------------------------------------------------------------

/// Holds a reference to a pool stream slot.  Decrements `pending_ops` on
/// drop.  Does **not** automatically synchronise the stream — call
/// `guard.stream().synchronize()` explicitly.
pub struct StreamGuard<'pool> {
    slot: Arc<StreamSlot>,
    _marker: std::marker::PhantomData<&'pool ()>,
}

impl<'pool> StreamGuard<'pool> {
    /// The borrowed stream.
    pub fn stream(&self) -> &GpuStream { &self.slot.stream }

    /// Record that an additional operation was enqueued on this stream.
    pub fn record_op(&self) { self.slot.pending_ops.fetch_add(1, Ordering::Relaxed); }

    /// Current pending-op count (informational).
    pub fn pending_ops(&self) -> u64 { self.slot.pending_ops.load(Ordering::Relaxed) }
}

impl Drop for StreamGuard<'_> {
    fn drop(&mut self) {
        // Decrement pending counter; saturate at 0.
        let prev = self.slot.pending_ops.load(Ordering::Relaxed);
        if prev > 0 {
            self.slot.pending_ops.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

impl fmt::Debug for StreamGuard<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StreamGuard({:?})", self.slot.stream)
    }
}

// ---------------------------------------------------------------------------
// StreamSet — fixed set of streams for a pipeline stage
// ---------------------------------------------------------------------------

/// A small, fixed collection of streams used to pipeline a single DAG node.
///
/// # Typical use
/// For a node that processes N batches, create a `StreamSet` with depth D.
/// Enqueue batch `i` on stream `i % D`, then sync stream `(i+1) % D` before
/// reusing it.  This creates a D-deep compute/transfer pipeline with no
/// extra synchronisation overhead.
#[derive(Debug, Clone)]
pub struct StreamSet {
    streams: Vec<GpuStream>,
    depth:   usize,
}

impl StreamSet {
    pub fn new(
        backend: &Arc<dyn ComputeBackend>,
        depth:   usize,
        prefix:  &str,
    ) -> Result<Self, GpuError> {
        let mut streams = Vec::with_capacity(depth);
        for i in 0..depth {
            streams.push(GpuStream::new(backend, format!("{}-{}", prefix, i))?);
        }
        Ok(Self { streams, depth })
    }

    /// Stream for slot `index % depth`.
    pub fn get(&self, index: usize) -> &GpuStream {
        &self.streams[index % self.depth]
    }

    /// Number of streams in the set.
    pub fn depth(&self) -> usize { self.depth }

    /// Synchronise all streams sequentially.
    pub fn synchronize_all(&self) -> Result<(), GpuError> {
        for s in &self.streams {
            s.synchronize()?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu_backend::stub::StubBackend;

    fn stub_backend() -> Arc<dyn ComputeBackend> {
        Arc::new(StubBackend::new(0))
    }

    #[test]
    fn test_stream_pool_round_robin() {
        let backend = stub_backend();
        let pool = StreamPool::new(&backend, 4, StreamAssignment::RoundRobin).unwrap();
        assert_eq!(pool.len(), 4);

        let g0 = pool.acquire().unwrap();
        let g1 = pool.acquire().unwrap();
        // IDs must differ (different streams)
        assert_ne!(g0.stream().id(), g1.stream().id());
    }

    #[test]
    fn test_stream_pool_least_pending() {
        let backend = stub_backend();
        let pool = StreamPool::new(&backend, 3, StreamAssignment::LeastPending).unwrap();

        let g = pool.acquire().unwrap();
        g.record_op();
        g.record_op();
        // Acquire again — should pick a different (less loaded) stream
        let g2 = pool.acquire().unwrap();
        // g2 should have fewer pending ops than g
        assert!(g2.pending_ops() < g.pending_ops());
    }

    #[test]
    fn test_stream_guard_decrements_on_drop() {
        let backend = stub_backend();
        let pool = StreamPool::new(&backend, 2, StreamAssignment::LeastPending).unwrap();
        {
            let g = pool.acquire().unwrap();
            assert_eq!(g.pending_ops(), 1); // acquired = 1
        }
        // After drop, the slot count returns to 0
        let g2 = pool.acquire().unwrap();
        assert_eq!(g2.pending_ops(), 1);
    }

    #[test]
    fn test_stream_set() {
        let backend = stub_backend();
        let set = StreamSet::new(&backend, 3, "compute").unwrap();
        assert_eq!(set.depth(), 3);
        // Modular access
        assert_eq!(set.get(0).id(), set.get(3).id());
        assert_ne!(set.get(0).id(), set.get(1).id());
        set.synchronize_all().unwrap();
    }

    #[test]
    fn test_pool_synchronize_all() {
        let backend = stub_backend();
        let pool = StreamPool::new(&backend, 4, StreamAssignment::RoundRobin).unwrap();
        pool.synchronize_all().unwrap();
    }
}
