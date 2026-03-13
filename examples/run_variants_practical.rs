use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use taskflow_rs::{Executor, Taskflow};

fn main() {
    println!("=== Practical Run Variants Examples ===\n");

    simulation_loop();
    println!();

    data_pipeline_batches();
    println!();

    convergence_algorithm();
    println!();

    parallel_data_sources();
}

/// Example 1: Game/Physics Simulation Loop
fn simulation_loop() {
    println!("1. Simulation Loop");
    println!("   Running physics simulation for 10 frames\n");

    let mut executor = Executor::new(4);

    #[derive(Clone)]
    struct SimState {
        frame: usize,
        positions: Vec<f64>,
    }

    let state = Arc::new(Mutex::new(SimState {
        frame: 0,
        positions: vec![0.0, 0.0, 0.0],
    }));

    let start = Instant::now();

    let state_clone = state.clone();
    executor
        .run_n(10, move || {
            let mut frame_flow = Taskflow::new();

            // Physics update
            let s1 = state_clone.clone();
            let physics = frame_flow.emplace(move || {
                let mut state = s1.lock().unwrap();
                state.frame += 1;

                // Simulate physics (simple gravity)
                for pos in &mut state.positions {
                    *pos -= 9.8 * 0.016; // gravity * dt
                }

                println!(
                    "   [Frame {}] Positions: {:?}",
                    state.frame, state.positions
                );
            });

            // Collision detection
            let s2 = state_clone.clone();
            let collision = frame_flow.emplace(move || {
                let state = s2.lock().unwrap();
                // Check collisions (simplified)
                for (i, pos) in state.positions.iter().enumerate() {
                    if *pos < -10.0 {
                        println!("   [Frame {}] Object {} hit ground!", state.frame, i);
                    }
                }
            });

            // Rendering (placeholder)
            let s3 = state_clone.clone();
            let render = frame_flow.emplace(move || {
                let state = s3.lock().unwrap();
                // Render frame
                thread::sleep(Duration::from_millis(16)); // 60 FPS
            });

            physics.precede(&collision);
            collision.precede(&render);

            frame_flow
        })
        .wait();

    let elapsed = start.elapsed();

    println!("\n   ✓ Simulation complete: 10 frames in {:?}", elapsed);
    println!("   Average FPS: {:.1}", 10.0 / elapsed.as_secs_f64());
}

/// Example 2: Data Pipeline with Batches
fn data_pipeline_batches() {
    println!("2. Data Pipeline with Batches");
    println!("   Processing 5 batches of data\n");

    let mut executor = Executor::new(4);

    let batch_num = Arc::new(AtomicUsize::new(0));
    let total_processed = Arc::new(AtomicUsize::new(0));

    let start = Instant::now();

    let b = batch_num.clone();
    let t = total_processed.clone();
    executor
        .run_n(5, move || {
            let mut pipeline = Taskflow::new();

            // Extract
            let b1 = b.clone();
            let extract = pipeline.emplace(move || {
                let batch = b1.fetch_add(1, Ordering::Relaxed) + 1;
                println!("   [Batch {}] Extracting data...", batch);
                thread::sleep(Duration::from_millis(50));
            });

            // Transform
            let transform = pipeline.emplace(move || {
                println!("   [Transform] Processing...");
                thread::sleep(Duration::from_millis(100));
            });

            // Load
            let t1 = t.clone();
            let load = pipeline.emplace(move || {
                let count = t1.fetch_add(100, Ordering::Relaxed) + 100;
                println!("   [Load] Stored (total: {} records)", count);
                thread::sleep(Duration::from_millis(50));
            });

            extract.precede(&transform);
            transform.precede(&load);

            pipeline
        })
        .wait();

    let elapsed = start.elapsed();

    println!(
        "\n   ✓ Pipeline complete: {} records in {:?}",
        total_processed.load(Ordering::Relaxed),
        elapsed
    );
}

/// Example 3: Iterative Convergence Algorithm
fn convergence_algorithm() {
    println!("3. Convergence Algorithm");
    println!("   Running until error < 0.01\n");

    let mut executor = Executor::new(4);

    let error = Arc::new(Mutex::new(1.0_f64));
    let iteration = Arc::new(AtomicUsize::new(0));

    let e = error.clone();
    let i = iteration.clone();
    let start = Instant::now();

    let e_check = error.clone();
    executor
        .run_until(
            move || {
                let mut algorithm = Taskflow::new();
                let e = e.clone();
                let i = i.clone();

                algorithm.emplace(move || {
                    let iter = i.fetch_add(1, Ordering::Relaxed) + 1;

                    // Simulate convergence (error decreases each iteration)
                    let mut err = e.lock().unwrap();
                    *err *= 0.8;

                    println!("   [Iteration {}] Error: {:.6}", iter, *err);
                    thread::sleep(Duration::from_millis(50));
                });

                algorithm
            },
            move || {
                let err = *e_check.lock().unwrap();
                err < 0.01
            },
        )
        .wait();

    let elapsed = start.elapsed();
    let final_error = *error.lock().unwrap();
    let iterations = iteration.load(Ordering::Relaxed);

    println!(
        "\n   ✓ Converged after {} iterations in {:?}",
        iterations, elapsed
    );
    println!("   Final error: {:.6}", final_error);
}

/// Example 4: Parallel Data Sources
fn parallel_data_sources() {
    println!("4. Parallel Data Sources");
    println!("   Processing 3 data sources concurrently\n");

    let mut executor = Executor::new(6);

    let results = Arc::new(Mutex::new(Vec::new()));

    // Create pipelines for each data source
    let sources = vec!["Database", "API", "Files"];
    let flows: Vec<_> = sources
        .iter()
        .map(|source| create_data_source_flow(source, results.clone()))
        .collect();

    let refs: Vec<_> = flows.iter().collect();

    let start = Instant::now();

    println!("   Starting parallel processing...\n");
    executor.run_many_and_wait(&refs);

    let elapsed = start.elapsed();
    let res = results.lock().unwrap();

    println!("\n   ✓ All sources processed in {:?}", elapsed);
    println!("   Total records: {}", res.len());
}

// Helper function to create a data source workflow
fn create_data_source_flow(source: &str, results: Arc<Mutex<Vec<i32>>>) -> Taskflow {
    let mut flow = Taskflow::new();
    let source = source.to_string();

    // Fetch
    let s1 = source.clone();
    let fetch = flow.emplace(move || {
        println!("   [{}] Fetching...", s1);
        thread::sleep(Duration::from_millis(100));
    });

    // Process
    let s2 = source.clone();
    let process = flow.emplace(move || {
        println!("   [{}] Processing...", s2);
        thread::sleep(Duration::from_millis(150));
    });

    // Store
    let s3 = source.clone();
    let store = flow.emplace(move || {
        println!("   [{}] Storing...", s3);

        // Simulate storing data
        let mut res = results.lock().unwrap();
        res.extend(vec![1, 2, 3, 4, 5]);

        thread::sleep(Duration::from_millis(50));
    });

    fetch.precede(&process);
    process.precede(&store);

    flow
}
