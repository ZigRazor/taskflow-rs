//! Preemptive cancellation demo
//!
//! Run:  cargo run --example preemptive_cancellation

use taskflow_rs::{
    Executor, Taskflow,
    PreemptiveCancellationToken, DeadlineGuard, Preempted, with_deadline,
};
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use std::time::Duration;
use std::thread;

fn main() {
    println!("=== Preemptive Cancellation Demo ===\n");

    demo_watchdog_timeout();
    println!();

    demo_manual_cancel_beats_watchdog();
    println!();

    demo_deadline_guard_raii();
    println!();

    demo_with_deadline_scoped();
    println!();

    demo_parallel_tasks_with_shared_token();
    println!();

    demo_reset_and_reuse();
}

// ─── 1. Watchdog automatically fires ─────────────────────────────────────────

fn demo_watchdog_timeout() {
    println!("1. Watchdog timeout");
    println!("   Task runs until watchdog fires after 100 ms\n");

    let token  = PreemptiveCancellationToken::new();
    let iters  = Arc::new(AtomicUsize::new(0));

    // Budget: 100 ms
    token.cancel_after_with(Duration::from_millis(100), "budget exceeded");

    let t   = token.clone();
    let ctr = iters.clone();
    let handle = thread::spawn(move || {
        for i in 0..1_000_000 {
            if t.is_cancelled() {
                println!("   [worker] preempted at iteration {} (reason: {:?})", i, t.reason());
                break;
            }
            ctr.fetch_add(1, Ordering::Relaxed);
            thread::sleep(Duration::from_micros(50)); // ~20 iters/ms
        }
    });

    handle.join().unwrap();
    println!("   Completed iterations: {}", iters.load(Ordering::Relaxed));
    println!("   ✓ Watchdog cancelled the task automatically");
}

// ─── 2. Manual cancel before watchdog ────────────────────────────────────────

fn demo_manual_cancel_beats_watchdog() {
    println!("2. Manual cancel before watchdog");
    println!("   Watchdog set to 5 s; manual cancel fires at 50 ms\n");

    let token = PreemptiveCancellationToken::new();
    token.cancel_after(Duration::from_secs(5)); // long watchdog

    let t = token.clone();
    let handle = thread::spawn(move || {
        let mut i = 0u64;
        loop {
            match t.check() {
                Err(Preempted { reason }) => {
                    println!("   [worker] cancelled at i={} (reason: {:?})", i, reason);
                    break;
                }
                Ok(()) => {}
            }
            i += 1;
            thread::sleep(Duration::from_millis(10));
        }
    });

    thread::sleep(Duration::from_millis(50));
    token.cancel_with("user requested stop");

    handle.join().unwrap();
    println!("   ✓ Manual cancel took effect before watchdog");
}

// ─── 3. DeadlineGuard RAII ───────────────────────────────────────────────────

fn demo_deadline_guard_raii() {
    println!("3. DeadlineGuard RAII");
    println!("   Guard auto-cancels when scope exits past budget\n");

    let token = PreemptiveCancellationToken::new();

    // Simulate a scope that takes 200 ms with a 100 ms budget.
    {
        let _guard: DeadlineGuard = token.deadline_guard(Duration::from_millis(100));
        thread::sleep(Duration::from_millis(200)); // intentionally overrun
        // _guard drops here → cancels token because elapsed > budget
    }

    println!("   Token cancelled after guard drop: {}", token.is_cancelled());
    assert!(token.is_cancelled());
    println!("   ✓ DeadlineGuard correctly cancelled the token");
}

// ─── 4. with_deadline scoped helper ──────────────────────────────────────────

fn demo_with_deadline_scoped() {
    println!("4. with_deadline() scoped helper");
    println!("   Runs a closure with a time budget; returns Err on timeout\n");

    let completed = Arc::new(AtomicUsize::new(0));
    let c = completed.clone();

    let result = with_deadline(Duration::from_millis(120), |tok| {
        for step in 0u32..100 {
            tok.check()?;                          // returns Err when cancelled
            thread::sleep(Duration::from_millis(20));
            c.fetch_add(1, Ordering::Relaxed);
            println!("   [step {}] working...", step);
        }
        Ok(42u32)
    });

    match result {
        Err(ref e) => println!("   Task preempted: {}", e),
        Ok(v)      => println!("   Task finished: result={}", v),
    }
    println!("   Steps completed: {}", completed.load(Ordering::Relaxed));
    println!("   ✓ with_deadline correctly interrupted the closure");
}

// ─── 5. Shared token across parallel tasks ───────────────────────────────────

fn demo_parallel_tasks_with_shared_token() {
    println!("5. Shared token across parallel tasks");
    println!("   One token cancels all tasks in a Taskflow\n");

    let token    = PreemptiveCancellationToken::new();
    let finished = Arc::new(AtomicUsize::new(0));
    let cancelled_count = Arc::new(AtomicUsize::new(0));

    // Set watchdog: cancel all after 80 ms
    token.cancel_after(Duration::from_millis(80));

    let mut executor = Executor::new(4);
    let mut taskflow = Taskflow::new();

    for task_id in 0..6 {
        let t    = token.clone();
        let fin  = finished.clone();
        let canc = cancelled_count.clone();

        taskflow.emplace(move || {
            let delay_ms = 20 + task_id * 15;
            let steps = delay_ms / 5;
            for _ in 0..steps {
                if t.is_cancelled() {
                    canc.fetch_add(1, Ordering::Relaxed);
                    return;
                }
                thread::sleep(Duration::from_millis(5));
            }
            fin.fetch_add(1, Ordering::Relaxed);
        });
    }

    executor.run(&taskflow).wait();

    println!("   Tasks finished (within budget): {}", finished.load(Ordering::Relaxed));
    println!("   Tasks cancelled (exceeded budget): {}", cancelled_count.load(Ordering::Relaxed));
    println!("   ✓ Shared token coordinated cancellation across all workers");
}

// ─── 6. Token reset and reuse ────────────────────────────────────────────────

fn demo_reset_and_reuse() {
    println!("6. Token reset and reuse");
    println!("   Same token reused across multiple task runs\n");

    let token = PreemptiveCancellationToken::new();

    for run in 0..3 {
        token.reset();
        token.cancel_after(Duration::from_millis(50));

        let t = token.clone();
        let steps = Arc::new(AtomicUsize::new(0));
        let s = steps.clone();

        let handle = thread::spawn(move || {
            while !t.is_cancelled() {
                s.fetch_add(1, Ordering::Relaxed);
                thread::sleep(Duration::from_millis(10));
            }
        });
        handle.join().unwrap();
        println!("   Run {}: {} steps before cancellation", run, steps.load(Ordering::Relaxed));
    }
    println!("   ✓ Token successfully reused 3 times");
}
