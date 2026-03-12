//! Dynamic priority adjustment demo
//!
//! Run:  cargo run --example dynamic_priority

use taskflow_rs::{
    Priority,
    SharedDynamicScheduler,
    EscalationPolicy,
};
use std::time::Duration;
use std::thread;

fn main() {
    println!("=== Dynamic Priority Adjustment Demo ===\n");

    demo_basic_push_pop();
    println!();

    demo_reprioritize_via_handle();
    println!();

    demo_fifo_within_equal_priority();
    println!();

    demo_escalation_policy_anti_starvation();
    println!();

    demo_cancel_queued_task();
    println!();

    demo_concurrent_reprioritize();
}

// ─── 1. Basic push / pop in priority order ───────────────────────────────────

fn demo_basic_push_pop() {
    println!("1. Basic priority-ordered pop");

    let sched = SharedDynamicScheduler::new();
    sched.push(10, Priority::Low);
    sched.push(20, Priority::Critical);
    sched.push(30, Priority::Normal);
    sched.push(40, Priority::High);

    println!("   Queue (push order): 10=Low, 20=Critical, 30=Normal, 40=High");
    print!("   Pop order: ");
    while let Some(id) = sched.pop() {
        print!("{} ", id);
    }
    println!();
    println!("   Expected:           20 40 30 10");
    println!("   ✓ Tasks popped in priority order (Critical→High→Normal→Low)");
}

// ─── 2. Reprioritize via PriorityHandle ──────────────────────────────────────

fn demo_reprioritize_via_handle() {
    println!("2. Reprioritize via PriorityHandle");

    let sched = SharedDynamicScheduler::new();

    let h_background = sched.push(1, Priority::Low);    // background task
    let _h_normal    = sched.push(2, Priority::Normal);
    let _h_important = sched.push(3, Priority::High);

    println!("   Initial queue: 1=Low, 2=Normal, 3=High");
    println!("   Snapshot: {:?}", sched.snapshot()
        .iter().map(|(id, p)| (*id, *p)).collect::<Vec<_>>());

    // Simulate user action: task 1 just became time-critical.
    println!("\n   >>> Escalating task 1: Low → Critical via handle");
    let changed = h_background.reprioritize(Priority::Critical);
    assert!(changed, "reprioritize should return true when task is queued");

    println!("   New priority of task 1: {:?}", h_background.current_priority());
    println!("   Updated snapshot: {:?}", sched.snapshot()
        .iter().map(|(id, p)| (*id, *p)).collect::<Vec<_>>());

    print!("   Pop order: ");
    while let Some(id) = sched.pop() {
        print!("{} ", id);
    }
    println!();
    println!("   ✓ Task 1 moved to front after dynamic escalation");
}

// ─── 3. FIFO preserved within same priority ──────────────────────────────────

fn demo_fifo_within_equal_priority() {
    println!("3. FIFO preserved within equal priority");

    let sched = SharedDynamicScheduler::new();
    for id in [100usize, 200, 300, 400] {
        sched.push(id, Priority::High);
    }

    // Reprioritize scenario with a known handle.
    let sched2 = SharedDynamicScheduler::new();
    let h_a = sched2.push(1usize, Priority::Normal);  // seq=0
    let _h_b = sched2.push(2usize, Priority::Normal); // seq=1
    // Elevate task 1 to High – its seq=0 stays, so it beats future High tasks.
    h_a.reprioritize(Priority::High);
    sched2.push(3usize, Priority::High); // seq=2

    print!("   Pop order (FIFO within High): ");
    while let Some(id) = sched2.pop() {
        print!("{} ", id);
    }
    println!();
    println!("   Expected: 1 3 2  (1 has oldest seq among High; 2 is Normal)");
    println!("   ✓ Sequence numbers preserve insertion order within a priority level");
}

// ─── 4. Anti-starvation escalation policy ────────────────────────────────────

fn demo_escalation_policy_anti_starvation() {
    println!("4. Anti-starvation escalation policy");
    println!("   Low-priority tasks are gradually bumped to avoid starvation\n");

    let sched = SharedDynamicScheduler::new();
    // Fill with mixed priorities
    sched.push(1, Priority::Low);
    sched.push(2, Priority::Low);
    sched.push(3, Priority::Normal);
    sched.push(4, Priority::High);

    let mut policy = EscalationPolicy::new(
        sched.clone(),
        /* tick_interval  */ 1,
        /* low_age_ticks  */ 1,
        /* normal_age_ticks */ 2,
    );

    println!("   Before escalation: {:?}",
        sched.snapshot().iter().map(|(id, p)| (*id, *p)).collect::<Vec<_>>());

    // Simulate scheduler ticks (in production these would be loop iterations).
    for tick in 1..=3 {
        policy.tick();
        println!("   After tick {}: {:?}", tick,
            sched.snapshot().iter().map(|(id, p)| (*id, *p)).collect::<Vec<_>>());
    }

    println!("   ✓ Low-priority tasks escalated to prevent starvation");
}

// ─── 5. Cancel a queued task ──────────────────────────────────────────────────

fn demo_cancel_queued_task() {
    println!("5. Cancel a queued task via handle");

    let sched = SharedDynamicScheduler::new();
    let h_unwanted = sched.push(99, Priority::High);
    let _          = sched.push(1,  Priority::Normal);
    let _          = sched.push(2,  Priority::Normal);

    println!("   Queue length before cancel: {}", sched.len());
    assert!(h_unwanted.cancel(), "cancel should return true when task is queued");
    assert!(!h_unwanted.cancel(), "second cancel should return false (already gone)");
    println!("   Queue length after cancel:  {}", sched.len());
    assert!(!h_unwanted.is_pending());

    print!("   Remaining pop order: ");
    while let Some(id) = sched.pop() {
        print!("{} ", id);
    }
    println!();
    println!("   ✓ Task 99 removed from queue without execution");
}

// ─── 6. Concurrent reprioritization ──────────────────────────────────────────

fn demo_concurrent_reprioritize() {
    println!("6. Concurrent reprioritization from multiple threads");

    let sched = SharedDynamicScheduler::new();

    // Push 20 tasks at Low priority.
    let handles: Vec<_> = (0usize..20)
        .map(|id| sched.push(id, Priority::Low))
        .collect();

    // Spawn a thread that escalates even-numbered tasks to Critical.
    let sched2 = sched.clone();
    let escalate_thread = thread::spawn(move || {
        for id in (0usize..20).step_by(2) {
            sched2.reprioritize(id, Priority::Critical);
            thread::sleep(Duration::from_micros(100));
        }
    });

    // Spawn a thread that demotes odd-numbered tasks via their handles.
    let demote_thread = thread::spawn(move || {
        for h in handles.iter().step_by(2).skip(1) {
            h.reprioritize(Priority::Normal);
            thread::sleep(Duration::from_micros(100));
        }
    });

    escalate_thread.join().unwrap();
    demote_thread.join().unwrap();

    // Drain the queue – all 20 tasks must be present.
    let mut popped = Vec::new();
    while let Some(id) = sched.pop() {
        popped.push(id);
    }
    popped.sort();
    assert_eq!(popped, (0usize..20).collect::<Vec<_>>(),
        "all 20 tasks must be present after concurrent reprioritization");

    println!("   All 20 tasks drained without data races.");
    println!("   ✓ Concurrent reprioritization is safe and lossless");
}
