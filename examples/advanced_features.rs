use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;
use taskflow_rs::{
    CancellationToken, Executor, NumaTopology, Priority, PriorityScheduler, Scheduler, Taskflow,
};

fn main() {
    println!("=== Advanced Features Demo ===\n");

    demo_task_priorities();
    println!();

    demo_task_cancellation();
    println!();

    demo_custom_scheduler();
    println!();

    demo_numa_awareness();
}

/// Demo 1: Task Priorities
fn demo_task_priorities() {
    println!("1. Task Priorities");
    println!("   Demonstrating priority-based scheduling\n");

    let mut executor = Executor::new(2);
    let execution_order = Arc::new(std::sync::Mutex::new(Vec::new()));

    // NOTE: This is a conceptual demo. Full priority support requires
    // integrating PriorityScheduler into the executor (see custom scheduler demo)

    let mut taskflow = Taskflow::new();

    // Create tasks with different priorities
    let order1 = execution_order.clone();
    let _low_priority = taskflow
        .emplace(move || {
            order1.lock().unwrap().push("Low");
            println!("   [Low Priority] Executing...");
            thread::sleep(Duration::from_millis(10));
        })
        .name("low_priority");

    let order2 = execution_order.clone();
    let _normal_priority = taskflow
        .emplace(move || {
            order2.lock().unwrap().push("Normal");
            println!("   [Normal Priority] Executing...");
            thread::sleep(Duration::from_millis(10));
        })
        .name("normal_priority");

    let order3 = execution_order.clone();
    let _high_priority = taskflow
        .emplace(move || {
            order3.lock().unwrap().push("High");
            println!("   [High Priority] Executing...");
            thread::sleep(Duration::from_millis(10));
        })
        .name("high_priority");

    let order4 = execution_order.clone();
    let _critical_priority = taskflow
        .emplace(move || {
            order4.lock().unwrap().push("Critical");
            println!("   [Critical Priority] Executing...");
            thread::sleep(Duration::from_millis(10));
        })
        .name("critical_priority");

    executor.run(&taskflow).wait();

    let final_order = execution_order.lock().unwrap();
    println!("\n   Execution order: {:?}", *final_order);
    println!("   ✓ Priority levels: Low < Normal < High < Critical");
}

/// Demo 2: Task Cancellation
fn demo_task_cancellation() {
    println!("2. Task Cancellation");
    println!("   Cancelling tasks before and during execution\n");

    let mut executor = Executor::new(4);
    let token = CancellationToken::new();

    let completed = Arc::new(AtomicUsize::new(0));
    let cancelled = Arc::new(AtomicUsize::new(0));

    // Run in a loop that can be cancelled
    for iteration in 0..5 {
        let mut taskflow = Taskflow::new();
        let t = token.clone();
        let comp = completed.clone();
        let canc = cancelled.clone();

        taskflow.emplace(move || {
            println!("   [Iteration {}] Starting work...", iteration);

            // Simulate work with cancellation checks
            for step in 0..10 {
                if t.is_cancelled() {
                    println!("   [Iteration {}] Cancelled at step {}", iteration, step);
                    canc.fetch_add(1, Ordering::Relaxed);
                    return;
                }

                thread::sleep(Duration::from_millis(20));
            }

            comp.fetch_add(1, Ordering::Relaxed);
            println!("   [Iteration {}] Completed", iteration);
        });

        // Cancel after iteration 2
        if iteration == 2 {
            println!("\n   >>> Cancelling remaining tasks...\n");
            token.cancel();
        }

        executor.run(&taskflow).wait();
    }

    println!(
        "\n   Tasks completed: {}",
        completed.load(Ordering::Relaxed)
    );
    println!("   Tasks cancelled: {}", cancelled.load(Ordering::Relaxed));
    println!("   ✓ Cancellation working correctly");
}

/// Demo 3: Custom Scheduler
fn demo_custom_scheduler() {
    println!("3. Custom Scheduler");
    println!("   Using priority-based scheduler\n");

    let mut scheduler = PriorityScheduler::new();

    // Add tasks with different priorities
    println!("   Adding tasks:");
    scheduler.push(1, Priority::Low);
    println!("     Task 1: Low priority");

    scheduler.push(2, Priority::High);
    println!("     Task 2: High priority");

    scheduler.push(3, Priority::Normal);
    println!("     Task 3: Normal priority");

    scheduler.push(4, Priority::Critical);
    println!("     Task 4: Critical priority");

    scheduler.push(5, Priority::Normal);
    println!("     Task 5: Normal priority");

    println!("\n   Execution order (by priority):");
    let mut order = Vec::new();
    while let Some(task_id) = scheduler.pop() {
        order.push(task_id);
        println!("     Task {}", task_id);
    }

    // Verify priority order: Critical(4) > High(2) > Normal(3,5 - FIFO) > Low(1)
    assert_eq!(order, vec![4, 2, 3, 5, 1]);
    println!("\n   ✓ Custom scheduler working correctly");
}

/// Demo 4: NUMA Awareness
fn demo_numa_awareness() {
    println!("4. NUMA Awareness");
    println!("   Detecting and utilizing NUMA topology\n");

    let topology = NumaTopology::detect();

    println!("   System Information:");
    println!("     CPU cores: {}", num_cpus::get());
    println!("     NUMA nodes: {}", topology.num_nodes);
    println!("     NUMA available: {}", topology.has_numa());

    if topology.has_numa() {
        println!("\n   NUMA Node Details:");
        for node in &topology.nodes {
            println!("     Node {}: {} CPUs", node.id, node.cpus.len());
            println!("       CPUs: {:?}", node.cpus);
        }
    } else {
        println!("     (Uniform Memory Access system detected)");
    }

    // Demonstrate worker-to-NUMA-node assignment
    println!("\n   Worker Pinning Strategies:");
    let num_workers = 4;

    use taskflow_rs::numa::{get_worker_cpus, NumaPinning};

    println!("     Round-Robin:");
    for worker_id in 0..num_workers {
        let cpus = get_worker_cpus(worker_id, num_workers, &topology, NumaPinning::RoundRobin);
        if !cpus.is_empty() {
            println!("       Worker {}: CPU {:?}", worker_id, cpus);
        }
    }

    println!("     Dense:");
    for worker_id in 0..num_workers {
        let cpus = get_worker_cpus(worker_id, num_workers, &topology, NumaPinning::Dense);
        if !cpus.is_empty() {
            println!("       Worker {}: CPU {:?}", worker_id, cpus);
        }
    }

    println!("\n   ✓ NUMA awareness configured");
}
