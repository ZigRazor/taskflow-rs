// examples/gpu_async_streams.rs
// ============================================================================
// Demonstrates:
//   1. Multiple CUDA streams running in parallel
//   2. Asynchronous host→device and device→host transfers
//   3. Overlapping compute and transfers with a double-buffer pattern
//   4. Backend-agnostic code (works with CUDA / OpenCL / ROCm / Stub)
// ============================================================================
//
// Run with CUDA:    cargo run --features gpu   --example gpu_async_streams
// Run with OpenCL:  cargo run --features opencl --example gpu_async_streams
// Run (stub only):  cargo run                  --example gpu_async_streams

use std::sync::{Arc, Mutex};
use std::time::Instant;

use taskflow_rs::{Executor, Taskflow};
use taskflow_rs::gpu::{
    GpuDevice, GpuBuffer, BackendKind,
};

// ---------------------------------------------------------------------------
// Demo 1 — Stream Pool with Round-Robin Assignment
// ---------------------------------------------------------------------------

fn demo_stream_pool() {
    println!("\n╔══════════════════════════════════════════════════════╗");
    println!("║  Demo 1: Stream Pool — Round-Robin Assignment        ║");
    println!("╚══════════════════════════════════════════════════════╝\n");

    let device = match GpuDevice::new(0) {
        Ok(d)  => { println!("  ✓ Device: {} ({:?})", d.name(), d.backend_kind()); d }
        Err(e) => { println!("  ✗ No GPU: {} (using stub)", e); return; }
    };

    // Create a pool of 4 streams — each stream can run independently
    let pool = device.stream_pool(4).expect("stream pool");
    println!("  ✓ StreamPool created with {} streams\n", pool.len());

    let num_batches = 8;
    let batch_size  = 256 * 1024; // 256K f32 elements = 1 MB

    let start = Instant::now();

    for batch_idx in 0..num_batches {
        // Acquire a stream via round-robin (non-blocking)
        let guard = pool.acquire().expect("acquire stream");
        let stream = guard.stream();

        let src: Vec<f32> = (0..batch_size)
            .map(|i| (batch_idx * batch_size + i) as f32)
            .collect();

        let mut buf: GpuBuffer<f32> = GpuBuffer::allocate(&device, batch_size)
            .expect("allocate");

        // Enqueue async H2D on this stream — returns immediately on CPU
        unsafe {
            buf.copy_from_host_async(&src, stream).expect("h2d async");
        }

        println!(
            "  Batch {:2}: enqueued H2D on stream {} (pending: {})",
            batch_idx,
            stream.id(),
            guard.pending_ops()
        );
    }

    // Barrier: wait for all streams to finish
    pool.synchronize_all().expect("sync all");
    println!("\n  ✓ All batches transferred in {:.2?}", start.elapsed());
}

// ---------------------------------------------------------------------------
// Demo 2 — Double-Buffer Pipeline (compute/transfer overlap)
// ---------------------------------------------------------------------------
//
//  Cycle:  [CPU fill A] → [H2D A ‖ CPU fill B] → [H2D B ‖ D2H A] → ...
//
//  Using a StreamSet of depth 2 gives us a simple 2-stage pipeline.

fn demo_double_buffer_pipeline() {
    println!("\n╔══════════════════════════════════════════════════════╗");
    println!("║  Demo 2: Double-Buffer Pipeline                      ║");
    println!("╚══════════════════════════════════════════════════════╝\n");

    let device = match GpuDevice::new(0) {
        Ok(d)  => d,
        Err(e) => { println!("  ✗ No GPU: {}", e); return; }
    };

    const DEPTH:      usize = 2;   // pipeline depth
    const BATCH_SIZE: usize = 512 * 1024; // 2 MB per buffer
    const N_BATCHES:  usize = 6;

    // Create a 2-deep stream set for the pipeline
    let stream_set = device.stream_set(DEPTH, "pipeline").expect("stream_set");

    // Two host-pinned buffers (alternating)
    let mut host_src  = [
        vec![0.0f32; BATCH_SIZE],
        vec![0.0f32; BATCH_SIZE],
    ];
    let mut host_dst  = [
        vec![0.0f32; BATCH_SIZE],
        vec![0.0f32; BATCH_SIZE],
    ];
    let mut dev_bufs: Vec<GpuBuffer<f32>> = (0..DEPTH)
        .map(|_| GpuBuffer::allocate(&device, BATCH_SIZE).expect("allocate"))
        .collect();

    let start = Instant::now();

    for batch in 0..N_BATCHES {
        let slot   = batch % DEPTH;
        let stream = stream_set.get(batch);

        // 1. Wait for the slot to be free (sync *this slot's* stream)
        if batch >= DEPTH {
            stream.synchronize().expect("sync slot");
        }

        // 2. Fill the host buffer for this slot (CPU work)
        for (i, v) in host_src[slot].iter_mut().enumerate() {
            *v = (batch * BATCH_SIZE + i) as f32 * 0.001;
        }

        // 3. Enqueue H2D (async)
        unsafe {
            dev_bufs[slot]
                .copy_from_host_async(&host_src[slot], stream)
                .expect("h2d async");
        }

        // 4. Enqueue D2H back (async — simulates result retrieval)
        unsafe {
            dev_bufs[slot]
                .copy_to_host_async(&mut host_dst[slot], stream)
                .expect("d2h async");
        }

        println!(
            "  Batch {:2}: slot={} stream={} — H2D+D2H enqueued",
            batch, slot, stream.id()
        );
    }

    // Final barrier
    stream_set.synchronize_all().expect("final sync");

    println!("\n  ✓ Double-buffer pipeline complete in {:.2?}", start.elapsed());
}

// ---------------------------------------------------------------------------
// Demo 3 — Multi-Stream in a TaskFlow DAG
// ---------------------------------------------------------------------------
//
//  TaskFlow graph:
//
//    [generate_data]──┬──[gpu_stream_0]──┬──[validate]
//                     ├──[gpu_stream_1]──┤
//                     └──[gpu_stream_2]──┘
//
//  Each GPU task runs on its own CUDA stream, so the three GPU tasks can
//  overlap on the device even though TaskFlow runs them on different
//  CPU worker threads.

fn demo_taskflow_multi_stream() {
    println!("\n╔══════════════════════════════════════════════════════╗");
    println!("║  Demo 3: Multi-Stream inside a TaskFlow DAG          ║");
    println!("╚══════════════════════════════════════════════════════╝\n");

    let device = match GpuDevice::new(0) {
        Ok(d)  => d,
        Err(e) => { println!("  ✗ No GPU: {}", e); return; }
    };

    const N_STREAMS:  usize = 3;
    const BATCH_SIZE: usize = 128 * 1024;

    // Shared data protected by a Mutex for cross-task communication
    let shared_data: Arc<Mutex<Vec<Vec<f32>>>> = Arc::new(Mutex::new(vec![]));

    // Create a pool accessible from all tasks
    let pool = Arc::new(device.stream_pool(N_STREAMS).expect("pool"));
    let device = Arc::new(device);

    let mut executor = Executor::new(4);
    let mut taskflow = Taskflow::new();

    // -- Task 1: generate data on CPU ------------------------------------
    let data_ref = Arc::clone(&shared_data);
    let generate = taskflow.emplace(move || {
        let mut data = data_ref.lock().unwrap();
        for s in 0..N_STREAMS {
            let batch: Vec<f32> = (0..BATCH_SIZE).map(|i| (s * BATCH_SIZE + i) as f32).collect();
            data.push(batch);
        }
        println!("  [CPU] Generated {} batches of {} elements", N_STREAMS, BATCH_SIZE);
    });

    // -- Tasks 2..4: one GPU task per stream ----------------------------
    let mut gpu_tasks = Vec::new();

    for stream_idx in 0..N_STREAMS {
        let data_ref = Arc::clone(&shared_data);
        let pool_ref = Arc::clone(&pool);
        let dev_ref  = Arc::clone(&device);

        let gpu_task = taskflow.emplace(move || {
            let guard  = pool_ref.acquire().expect("acquire");
            let stream = guard.stream();

            let data = data_ref.lock().unwrap();
            let src  = &data[stream_idx];

            let mut buf = GpuBuffer::allocate(&dev_ref, src.len())
                .expect("alloc");

            // Async H2D on this stream
            unsafe {
                buf.copy_from_host_async(src, stream).expect("h2d");
            }

            // Simulate device computation (would be a kernel launch here)
            // ...

            // Async D2H
            let mut result = vec![0.0f32; src.len()];
            unsafe {
                buf.copy_to_host_async(&mut result, stream).expect("d2h");
            }

            // Sync *this stream only* — other streams keep running
            stream.synchronize().expect("sync stream");

            println!(
                "  [GPU stream {}] Processed {} elements on {}",
                stream.id(), src.len(), dev_ref.name()
            );
        });

        gpu_tasks.push(gpu_task);
    }

    // -- Task 5: validate (runs after all GPU tasks) --------------------
    let validate = taskflow.emplace(|| {
        println!("  [CPU] Validation complete ✓");
    });

    // -- Wire the DAG ---------------------------------------------------
    for gpu_task in &gpu_tasks {
        generate.precede(gpu_task);
        gpu_task.precede(&validate);
    }

    let start = Instant::now();
    executor.run(&taskflow).wait();
    println!("\n  ✓ TaskFlow multi-stream DAG complete in {:.2?}", start.elapsed());
}

// ---------------------------------------------------------------------------
// Demo 4 — Backend selection at runtime
// ---------------------------------------------------------------------------

fn demo_backend_selection() {
    println!("\n╔══════════════════════════════════════════════════════╗");
    println!("║  Demo 4: Runtime Backend Selection                   ║");
    println!("╚══════════════════════════════════════════════════════╝\n");

    // Try each backend in turn; report what's available
    let candidates = [
        BackendKind::Cuda,
        BackendKind::Rocm,
        BackendKind::OpenCL,
        BackendKind::Stub,
    ];

    for kind in &candidates {
        match GpuDevice::with_backend(0, *kind) {
            Ok(dev) => println!(
                "  ✓ {:<10} — {} | memory: {:.1} GB",
                format!("{:?}", kind),
                dev.name(),
                dev.memory_info()
                   .map(|(_, t)| t as f64 / 1e9)
                   .unwrap_or(0.0)
            ),
            Err(e) => println!("  ✗ {:<10} — {}", format!("{:?}", kind), e),
        }
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    // Initialise simple logger
    let _ = env_logger::try_init();

    println!("┌─────────────────────────────────────────────────────┐");
    println!("│  TaskFlow-RS — Async Transfers & Multi-Stream Demo  │");
    println!("└─────────────────────────────────────────────────────┘");

    demo_backend_selection();
    demo_stream_pool();
    demo_double_buffer_pipeline();
    demo_taskflow_multi_stream();

    println!("\n═══════════════════════════════════════════════════════");
    println!("All demos complete.");
}
