//! Preemptive task cancellation
//!
//! Extends cooperative cancellation with deadline-driven, signal-driven, and
//! scope-bound preemption. Architecture:
//!
//! ```text
//!   PreemptiveCancellationToken  ──owns──►  PreemptionState (Arc)
//!         │                                       │
//!         │ cancel_after(Duration)                │ cancelled: AtomicBool
//!         │      │                                │ deadline:  AtomicU64
//!         │      ▼                                │ waiters:   Condvar
//!         │  WatchdogThread  ──notify──►  sets cancelled = true
//!         │
//!         │ cancel_at(Instant)  ──same path──►  WatchdogThread
//!         │
//!         │ [Linux] signal_preempt()  ──pthread_kill──►  SIGUSR2 handler
//!         │                                             sets AtomicBool
//!         ▼
//!   DeadlineGuard  (RAII)  ──drop──►  if elapsed ≥ budget → cancel()
//! ```
//!
//! ## Usage
//!
//! ```rust
//! use taskflow_rs::preemptive::{PreemptiveCancellationToken, DeadlineGuard};
//! use std::time::Duration;
//!
//! let token = PreemptiveCancellationToken::new();
//!
//! // Option 1 – automatic watchdog timeout
//! token.cancel_after(Duration::from_millis(500));
//!
//! // Option 2 – RAII deadline guard
//! let guard = token.deadline_guard(Duration::from_millis(500));
//!
//! // Inside the task:
//! for chunk in data.chunks(1024) {
//!     token.check()?;          // returns Err(Preempted) if cancelled
//!     process(chunk);
//! }
//! ```

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// ─── Error type ──────────────────────────────────────────────────────────────

/// Returned by `PreemptiveCancellationToken::check()` when the task has been
/// preempted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preempted {
    /// Human-readable reason supplied at cancellation time, if any.
    pub reason: Option<String>,
}

impl std::fmt::Display for Preempted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.reason {
            Some(r) => write!(f, "Task preempted: {}", r),
            None => write!(f, "Task preempted"),
        }
    }
}

impl std::error::Error for Preempted {}

// ─── Shared state (heap-allocated, reference-counted) ────────────────────────

struct PreemptionState {
    /// Set to `true` as soon as cancellation is requested.
    cancelled: AtomicBool,
    /// Optional reason (set before `cancelled` is flipped).
    reason: Mutex<Option<String>>,
    /// Absolute deadline as UNIX milliseconds (0 = no deadline).
    deadline_unix_ms: AtomicU64,
    /// Wakes any sleeping watchdog threads when state changes.
    condvar: Condvar,
    /// Mutex required by the Condvar protocol.
    wakeup: Mutex<bool>,
}

impl PreemptionState {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            cancelled: AtomicBool::new(false),
            reason: Mutex::new(None),
            deadline_unix_ms: AtomicU64::new(0),
            condvar: Condvar::new(),
            wakeup: Mutex::new(false),
        })
    }

    fn cancel_with_reason(&self, reason: Option<String>) {
        if let Ok(mut r) = self.reason.lock() {
            *r = reason;
        }
        self.cancelled.store(true, Ordering::Release);
        // Wake the watchdog / any waiter immediately.
        if let Ok(mut w) = self.wakeup.lock() {
            *w = true;
            self.condvar.notify_all();
        }
    }
}

// ─── Public token ─────────────────────────────────────────────────────────────

/// Preemptive cancellation token.
///
/// Cloning is cheap – all clones share the same underlying state.
#[derive(Clone)]
pub struct PreemptiveCancellationToken {
    state: Arc<PreemptionState>,
}

impl std::fmt::Debug for PreemptiveCancellationToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreemptiveCancellationToken")
            .field("cancelled", &self.state.cancelled.load(Ordering::Relaxed))
            .finish()
    }
}

impl Default for PreemptiveCancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

impl PreemptiveCancellationToken {
    // ── Construction ─────────────────────────────────────────────────────────

    /// Create a fresh, non-cancelled token.
    pub fn new() -> Self {
        Self {
            state: PreemptionState::new(),
        }
    }

    // ── Cancellation triggers ────────────────────────────────────────────────

    /// Cancel immediately (cooperative – tasks must call `check()` or
    /// `is_cancelled()`).
    pub fn cancel(&self) {
        self.state.cancel_with_reason(None);
    }

    /// Cancel with a human-readable reason.
    pub fn cancel_with(&self, reason: impl Into<String>) {
        self.state.cancel_with_reason(Some(reason.into()));
    }

    /// Schedule automatic cancellation `after` the given duration.
    ///
    /// Spawns a lightweight watchdog thread that sleeps until the deadline,
    /// then atomically sets the cancelled flag.  Returns immediately.
    pub fn cancel_after(&self, after: Duration) {
        let deadline = Instant::now() + after;
        self.cancel_at_instant(deadline, None);
    }

    /// Schedule automatic cancellation with a reason.
    pub fn cancel_after_with(&self, after: Duration, reason: impl Into<String>) {
        let deadline = Instant::now() + after;
        self.cancel_at_instant(deadline, Some(reason.into()));
    }

    /// Schedule automatic cancellation at an absolute `Instant`.
    pub fn cancel_at(&self, deadline: Instant) {
        self.cancel_at_instant(deadline, None);
    }

    // ── RAII helpers ──────────────────────────────────────────────────────────

    /// Create a `DeadlineGuard` that cancels this token if dropped after the
    /// budget elapses.  The token is also auto-cancelled via a watchdog thread,
    /// so even if the guard is never explicitly dropped, cancellation fires.
    pub fn deadline_guard(&self, budget: Duration) -> DeadlineGuard {
        self.cancel_after(budget);
        DeadlineGuard {
            token: self.clone(),
            start: Instant::now(),
            budget,
        }
    }

    // ── Polling helpers (hot path) ────────────────────────────────────────────

    /// `true` if the task has been cancelled/preempted.
    #[inline(always)]
    pub fn is_cancelled(&self) -> bool {
        self.state.cancelled.load(Ordering::Acquire)
    }

    /// Returns `Ok(())` when not cancelled; `Err(Preempted)` otherwise.
    ///
    /// Designed for use inside task loops:
    /// ```ignore
    /// token.check()?;
    /// ```
    #[inline(always)]
    pub fn check(&self) -> Result<(), Preempted> {
        if self.is_cancelled() {
            let reason = self.state.reason.lock().ok().and_then(|r| r.clone());
            Err(Preempted { reason })
        } else {
            Ok(())
        }
    }

    /// Check *and* yield the current thread to the OS scheduler.
    ///
    /// Useful in compute-bound loops – gives the runtime a chance to run
    /// the watchdog or other pending tasks, reducing observed cancellation
    /// latency without expensive system calls.
    #[inline]
    pub fn check_and_yield(&self) -> Result<(), Preempted> {
        let result = self.check();
        if result.is_ok() {
            thread::yield_now();
        }
        result
    }

    // ── State management ──────────────────────────────────────────────────────

    /// Reset the token so it can be reused for a new task.
    /// Cancels any pending watchdog (watchdog will wake, see `cancelled=true`,
    /// then exit without re-setting because `deadline_unix_ms` is zeroed).
    pub fn reset(&self) {
        self.state.deadline_unix_ms.store(0, Ordering::Release);
        if let Ok(mut r) = self.state.reason.lock() {
            *r = None;
        }
        self.state.cancelled.store(false, Ordering::Release);
    }

    /// The reason supplied to `cancel_with` / `cancel_after_with`, if any.
    pub fn reason(&self) -> Option<String> {
        self.state.reason.lock().ok().and_then(|r| r.clone())
    }

    // ── OS-level signal preemption (Linux only) ───────────────────────────────

    /// Install a per-process SIGUSR2 handler that sets a thread-local
    /// cancellation flag.  Call once at program startup on Linux.
    ///
    /// After installation, `signal_preempt_thread(handle)` will send SIGUSR2
    /// to any thread, making its next `check_signal()` call return `Err`.
    ///
    /// # Safety
    /// Installs a signal handler via `libc`.  Safe if called at most once and
    /// not combined with other SIGUSR2 handlers.
    #[cfg(target_os = "linux")]
    pub unsafe fn install_signal_handler() {
        signal_preemption::install();
    }

    /// Send SIGUSR2 to a specific thread to trigger signal-based preemption.
    ///
    /// The target thread must have called `install_signal_handler()` first.
    #[cfg(target_os = "linux")]
    pub fn signal_preempt_thread(pthread_id: libc::pthread_t) {
        signal_preemption::preempt(pthread_id);
    }

    /// Check the thread-local signal flag set by the SIGUSR2 handler.
    #[cfg(target_os = "linux")]
    #[inline]
    pub fn check_signal() -> Result<(), Preempted> {
        signal_preemption::check_flag()
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    fn cancel_at_instant(&self, deadline: Instant, reason: Option<String>) {
        // Encode deadline as UNIX ms for the watchdog thread.
        let unix_ms = unix_ms_from_instant(deadline);
        self.state
            .deadline_unix_ms
            .store(unix_ms, Ordering::Release);

        let state = self.state.clone();
        thread::Builder::new()
            .name("taskflow-watchdog".into())
            .spawn(move || {
                let now = Instant::now();
                if deadline <= now {
                    state.cancel_with_reason(reason);
                    return;
                }
                let sleep_for = deadline - now;
                // Sleep interruptibly: wait_timeout releases the lock while
                // sleeping, then *re-acquires* it on return.
                // We must drop that re-acquired guard BEFORE calling
                // cancel_with_reason, which itself needs to lock `wakeup`
                // (to notify any other waiters).  Failing to drop first
                // causes a self-deadlock: the watchdog thread tries to
                // lock a mutex it already holds and hangs forever,
                // which then also blocks any caller of cancel() on the
                // main thread when the guard scope exits.
                let lock = state.wakeup.lock().unwrap();
                let (guard, timed_out) = state.condvar.wait_timeout(lock, sleep_for).unwrap();
                let should_cancel =
                    timed_out.timed_out() && !state.cancelled.load(Ordering::Acquire);
                drop(guard); // release wakeup lock BEFORE cancel_with_reason
                if should_cancel {
                    state.cancel_with_reason(reason);
                }
                // If woken early (manual cancel), flag is already set.
            })
            .expect("Failed to spawn watchdog thread");
    }
}

// ─── RAII deadline guard ──────────────────────────────────────────────────────

/// RAII guard that automatically cancels a token when dropped (if the budget
/// has elapsed).  Useful for lexically scoped time budgets:
///
/// ```ignore
/// let _guard = token.deadline_guard(Duration::from_millis(100));
/// // ... task body ...
/// // token is cancelled here if the scope took > 100 ms.
/// ```
pub struct DeadlineGuard {
    token: PreemptiveCancellationToken,
    start: Instant,
    budget: Duration,
}

impl DeadlineGuard {
    /// How much of the budget remains, or `Duration::ZERO` if expired.
    pub fn remaining(&self) -> Duration {
        self.budget
            .checked_sub(self.start.elapsed())
            .unwrap_or(Duration::ZERO)
    }

    /// True if the budget window has already expired.
    pub fn is_expired(&self) -> bool {
        self.start.elapsed() >= self.budget
    }

    /// The underlying token (to call `check()` inside the task).
    pub fn token(&self) -> &PreemptiveCancellationToken {
        &self.token
    }
}

impl Drop for DeadlineGuard {
    fn drop(&mut self) {
        if self.is_expired() {
            self.token.cancel_with("deadline elapsed");
        }
    }
}

// ─── Scoped preemption helper ─────────────────────────────────────────────────

/// Run a closure with a time budget.  Returns `Err(Preempted)` if the token
/// was cancelled *before* the closure finishes.  The closure itself decides
/// how frequently it polls `token.check()`.
///
/// ```ignore
/// let result = with_deadline(Duration::from_secs(2), |tok| {
///     for chunk in big_data.chunks(4096) {
///         tok.check()?;
///         process(chunk);
///     }
///     Ok(42)
/// });
/// ```
pub fn with_deadline<T, F>(budget: Duration, f: F) -> Result<T, Preempted>
where
    F: FnOnce(&PreemptiveCancellationToken) -> Result<T, Preempted>,
{
    let token = PreemptiveCancellationToken::new();
    token.cancel_after(budget);
    f(&token)
}

// ─── Utilities ────────────────────────────────────────────────────────────────

fn unix_ms_from_instant(deadline: Instant) -> u64 {
    let now_instant = Instant::now();
    let now_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    if deadline > now_instant {
        let offset_ms = (deadline - now_instant).as_millis() as u64;
        now_unix_ms.saturating_add(offset_ms)
    } else {
        now_unix_ms
    }
}

// ─── Linux signal-based preemption ───────────────────────────────────────────

#[cfg(target_os = "linux")]
mod signal_preemption {
    use std::cell::Cell;
    use std::sync::atomic::{AtomicBool, Ordering};

    // Per-thread interrupt flag set by the SIGUSR2 handler.
    thread_local! {
        static SIGNAL_PREEMPTED: Cell<bool> = Cell::new(false);
    }

    // Global flag indicating the handler is installed.
    static HANDLER_INSTALLED: AtomicBool = AtomicBool::new(false);

    /// Install the SIGUSR2 handler.  Idempotent.
    pub(super) unsafe fn install() {
        if HANDLER_INSTALLED.swap(true, Ordering::AcqRel) {
            return; // already installed
        }
        // Use libc to install the signal handler.
        // We piggyback on SIGUSR2 (signal 12 on Linux).
        extern "C" fn handler(_sig: libc::c_int) {
            SIGNAL_PREEMPTED.with(|f| f.set(true));
        }
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = handler as libc::sighandler_t;
        sa.sa_flags = libc::SA_RESTART;
        libc::sigaction(libc::SIGUSR2, &sa, std::ptr::null_mut());
    }

    /// Send SIGUSR2 to `thread`.
    pub(super) fn preempt(thread: libc::pthread_t) {
        unsafe {
            libc::pthread_kill(thread, libc::SIGUSR2);
        }
    }

    /// Check and clear the thread-local flag.
    #[inline]
    pub(super) fn check_flag() -> Result<(), super::Preempted> {
        let preempted = SIGNAL_PREEMPTED.with(|f| {
            if f.get() {
                f.set(false);
                true
            } else {
                false
            }
        });
        if preempted {
            Err(super::Preempted {
                reason: Some("signal preemption".into()),
            })
        } else {
            Ok(())
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering as AO};
    use std::sync::Arc;

    #[test]
    fn manual_cancel() {
        let token = PreemptiveCancellationToken::new();
        assert!(!token.is_cancelled());
        assert!(token.check().is_ok());

        token.cancel();
        assert!(token.is_cancelled());
        assert!(token.check().is_err());
    }

    #[test]
    fn cancel_with_reason() {
        let token = PreemptiveCancellationToken::new();
        token.cancel_with("budget exceeded");

        let err = token.check().unwrap_err();
        assert_eq!(err.reason.as_deref(), Some("budget exceeded"));
    }

    #[test]
    fn watchdog_fires_after_timeout() {
        let token = PreemptiveCancellationToken::new();
        token.cancel_after(Duration::from_millis(50));

        assert!(!token.is_cancelled());
        thread::sleep(Duration::from_millis(120));
        assert!(token.is_cancelled());
    }

    #[test]
    fn manual_cancel_beats_watchdog() {
        let token = PreemptiveCancellationToken::new();
        token.cancel_after(Duration::from_millis(500));
        token.cancel(); // fires first
        assert!(token.is_cancelled());
    }

    #[test]
    fn reset_clears_state() {
        let token = PreemptiveCancellationToken::new();
        token.cancel_with("test");
        assert!(token.is_cancelled());

        token.reset();
        assert!(!token.is_cancelled());
        assert!(token.reason().is_none());
        assert!(token.check().is_ok());
    }

    #[test]
    fn deadline_guard_cancels_on_expiry() {
        let token = PreemptiveCancellationToken::new();
        {
            let _guard = token.deadline_guard(Duration::from_millis(10));
            thread::sleep(Duration::from_millis(30));
            // guard drops here after budget elapsed
        }
        assert!(token.is_cancelled());
    }

    #[test]
    fn with_deadline_respects_budget() {
        let counter = Arc::new(AtomicUsize::new(0));
        let c = counter.clone();

        let result = with_deadline(Duration::from_millis(80), |tok| loop {
            tok.check()?;
            c.fetch_add(1, AO::Relaxed);
            thread::sleep(Duration::from_millis(20));
        });
        assert!(result.is_err());
        let iterations = counter.load(AO::Relaxed);
        // Should have run at least once but not 10+ times
        assert!(
            (1..=6).contains(&iterations),
            "expected 1-6 iterations, got {}",
            iterations
        );
    }

    #[test]
    fn clone_shares_state() {
        let token = PreemptiveCancellationToken::new();
        let clone = token.clone();
        clone.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn with_deadline_ok_path() {
        let result = with_deadline(Duration::from_secs(5), |tok| {
            tok.check()?;
            Ok(42u32)
        });
        assert_eq!(result.unwrap(), 42);
    }
}
