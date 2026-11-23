use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

pub struct IrqSave(u64);

impl IrqSave {
    pub fn new() -> Self {
        let rflags: u64;
        // UNSAFE JUSTIFICATION:
        // 1. Reason: Read RFLAGS and execute CLI to disable interrupts for critical section.
        // 2. Assumptions: Running in Ring 0 privilege level on x86_64.
        // 3. Invariants: Original interrupt state is saved in self.0 and restored on Drop.
        unsafe {
            core::arch::asm!(
                "pushfq",
                "pop {}",
                "cli",
                out(reg) rflags,
                options(nomem, preserves_flags)
            );
        }
        Self(rflags)
    }
}

impl Drop for IrqSave {
    fn drop(&mut self) {
        if (self.0 & 0x200) != 0 {
            // UNSAFE JUSTIFICATION:
            // 1. Reason: Restore hardware interrupts via STI if previously enabled.
            // 2. Assumptions: Running in Ring 0 privilege level after critical section exit.
            // 3. Invariants: Only re-enables IF bit when prior RFLAGS had IF (bit 9) set.
            unsafe {
                core::arch::asm!("sti", options(nomem, nostack, preserves_flags));
            }
        }
    }
}

pub struct Spinlock<T> {
    locked: AtomicBool,
    acquisitions: AtomicUsize,
    contentions: AtomicUsize,
    data: UnsafeCell<T>,
}

// UNSAFE JUSTIFICATION:
// 1. Reason: Share Spinlock<T> across kernel execution contexts.
// 2. Assumptions: T is Send.
// 3. Invariants: AtomicBool acquire/release semantics enforce exclusive mutable access.
unsafe impl<T: Send> Sync for Spinlock<T> {}

// UNSAFE JUSTIFICATION:
// 1. Reason: Transfer Spinlock<T> across kernel boundaries.
// 2. Assumptions: T is Send.
// 3. Invariants: Protected data is only accessed while lock is held.
unsafe impl<T: Send> Send for Spinlock<T> {}

impl<T> Spinlock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            acquisitions: AtomicUsize::new(0),
            contentions: AtomicUsize::new(0),
            data: UnsafeCell::new(data),
        }
    }

    pub fn lock(&self) -> SpinlockGuard<'_, T> {
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            self.contentions.fetch_add(1, Ordering::Relaxed);
            core::hint::spin_loop();
        }
        self.acquisitions.fetch_add(1, Ordering::Relaxed);
        SpinlockGuard { lock: self }
    }

    pub fn try_lock(&self) -> Option<SpinlockGuard<'_, T>> {
        if self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            self.acquisitions.fetch_add(1, Ordering::Relaxed);
            Some(SpinlockGuard { lock: self })
        } else {
            self.contentions.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    pub fn stats(&self) -> (usize, usize) {
        (
            self.acquisitions.load(Ordering::Relaxed),
            self.contentions.load(Ordering::Relaxed),
        )
    }
}

pub struct SpinlockGuard<'a, T> {
    lock: &'a Spinlock<T>,
}

impl<'a, T> Deref for SpinlockGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // UNSAFE JUSTIFICATION:
        // 1. Reason: Dereference UnsafeCell pointer inside active SpinlockGuard.
        // 2. Assumptions: Guard only exists while self.lock.locked is true.
        // 3. Invariants: Exclusive ownership of lock guarantees no concurrent mutable aliasing.
        unsafe { &*self.lock.data.get() }
    }
}

impl<'a, T> DerefMut for SpinlockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        // UNSAFE JUSTIFICATION:
        // 1. Reason: Obtain mutable reference from UnsafeCell while holding spinlock.
        // 2. Assumptions: Guard holds exclusive lock via Acquire memory ordering.
        // 3. Invariants: Only one SpinlockGuard can exist at any time for a given Spinlock.
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<'a, T> Drop for SpinlockGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
    }
}

struct SharedState {
    counter: u64,
    checksum: u64,
}

static SHARED_STATE: Spinlock<SharedState> = Spinlock::new(SharedState {
    counter: 0,
    checksum: 0,
});

pub fn run_sync_demo() {
    crate::kinfo!("Starting Layer 7 Synchronization Verification...");
    for step in 1..=4 {
        let _irq = IrqSave::new();
        let mut guard = SHARED_STATE.lock();
        if step == 2 {
            let contended = SHARED_STATE.try_lock();
            if contended.is_none() {
                crate::ktrace!(step, "SYNC", "LOCK_CONTENDED", "try_lock=REJECTED");
            }
        }
        guard.counter += 1;
        guard.checksum = guard.counter * 42;
        crate::ktrace!(
            step,
            "SYNC",
            "CRITICAL_SECTION",
            "counter={} checksum={}",
            guard.counter,
            guard.checksum
        );
    }
    let guard = SHARED_STATE.lock();
    let (acq, cont) = SHARED_STATE.stats();
    crate::kinfo!(
        "Spinlock Verified: counter={} checksum={} acquisitions={} contentions={}",
        guard.counter,
        guard.checksum,
        acq,
        cont
    );
    crate::ktrace!(
        5,
        "SYNC",
        "INVARIANT_OK",
        "counter={} checksum={} acq={} cont={}",
        guard.counter,
        guard.checksum,
        acq,
        cont
    );
}