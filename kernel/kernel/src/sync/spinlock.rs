// Copyright (c) 2026 vivo Mobile Communication Co., Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//       http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Exclusive spin locks for short, non-sleeping kernel critical sections.
//!
//! Acquiring the same lock twice with `lock` will deadlock, including on a
//! single CPU. Use `try_lock` when a busy lock should return immediately.

use super::local_context::PreemptDisabled;
use crate::support::DisableInterruptGuard;
use core::ops::{Deref, DerefMut};
use spin::{Mutex, MutexGuard};

/// Disables preemption while waiting for or holding the lock. Local kernel IRQs
/// may still run. Use [`SpinLockIrq`] if an IRQ handler also takes the lock.
pub struct SpinLock<T: ?Sized>(Mutex<T>);

/// Masks local kernel IRQs before taking the lock and keeps them masked until
/// the guard is released.
pub struct SpinLockIrq<T: ?Sized>(Mutex<T>);

/// Holds the lock and the CPU-local state established before acquisition.
#[must_use = "dropping the guard releases the spin lock"]
pub struct Guard<'a, T: ?Sized, C> {
    // Fields drop in declaration order: unlock before restoring IRQs or
    // allowing a pending context switch.
    inner: MutexGuard<'a, T>,
    context: C,
}

pub type SpinLockGuard<'a, T> = Guard<'a, T, PreemptDisabled>;
pub type SpinLockIrqGuard<'a, T, C = DisableInterruptGuard> = Guard<'a, T, C>;

impl<'a, T: ?Sized, C> Guard<'a, T, C> {
    fn lock(lock: &'a Mutex<T>, context: C) -> Self {
        Self {
            inner: lock.lock(),
            context,
        }
    }

    fn try_lock(lock: &'a Mutex<T>, context: C) -> Option<Self> {
        Some(Self {
            inner: lock.try_lock()?,
            context,
        })
    }
}

impl<T: ?Sized, C> Deref for Guard<'_, T, C> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.inner
    }
}

impl<T: ?Sized, C> DerefMut for Guard<'_, T, C> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

impl<T> SpinLock<T> {
    pub const fn new(value: T) -> Self {
        Self(Mutex::new(value))
    }
}

impl<T: ?Sized> SpinLock<T> {
    pub fn lock(&self) -> SpinLockGuard<'_, T> {
        let preempt = PreemptDisabled::new_with(&DisableInterruptGuard::new());
        Guard::lock(&self.0, preempt)
    }

    #[must_use]
    pub fn try_lock(&self) -> Option<SpinLockGuard<'_, T>> {
        let preempt = PreemptDisabled::new_with(&DisableInterruptGuard::new());
        Guard::try_lock(&self.0, preempt)
    }
}

impl<T> SpinLockIrq<T> {
    pub const fn new(value: T) -> Self {
        Self(Mutex::new(value))
    }
}

impl<T: ?Sized> SpinLockIrq<T> {
    pub fn lock(&self) -> SpinLockIrqGuard<'_, T> {
        Guard::lock(&self.0, DisableInterruptGuard::new())
    }

    #[must_use]
    pub fn try_lock(&self) -> Option<SpinLockIrqGuard<'_, T>> {
        Guard::try_lock(&self.0, DisableInterruptGuard::new())
    }

    /// Reuses the caller's IRQ mask for the lifetime of the returned guard.
    pub fn lock_with<'a>(
        &'a self,
        irq: &'a DisableInterruptGuard,
    ) -> SpinLockIrqGuard<'a, T, &'a DisableInterruptGuard> {
        Guard::lock(&self.0, irq)
    }

    #[must_use]
    pub fn try_lock_with<'a>(
        &'a self,
        irq: &'a DisableInterruptGuard,
    ) -> Option<SpinLockIrqGuard<'a, T, &'a DisableInterruptGuard>> {
        Guard::try_lock(&self.0, irq)
    }
}

impl<T: ?Sized> SpinLockIrqGuard<'_, T> {
    /// Borrows the IRQ mask owned by this guard.
    pub fn irq_guard(&self) -> &DisableInterruptGuard {
        &self.context
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as blueos;
    use crate::{arch, scheduler, sync::execution_guard_held};
    use blueos_test_macro::test;
    use core::{
        cell::Cell,
        sync::atomic::{AtomicUsize, Ordering},
    };

    #[test]
    fn test_spinlock_preemption() {
        let lock = SpinLock::new(0);
        let mut held = lock.lock();
        assert!(arch::local_irq_enabled());
        assert!(execution_guard_held());
        assert!(lock.try_lock().is_none());
        scheduler::yield_me_now_or_later();
        *held = 17;
        drop(held);
        assert!(!execution_guard_held());
        assert_eq!(*lock.try_lock().unwrap(), 17);
    }

    #[test]
    fn test_spinlock_irq_restore_order() {
        assert!(arch::local_irq_enabled());
        let a = SpinLockIrq::new(1);
        let b = SpinLockIrq::new(2);
        let first = a.lock();
        let second = b.try_lock().unwrap();
        assert!(a.try_lock().is_none());
        drop(first);
        assert!(!arch::local_irq_enabled());
        assert!(b.try_lock_with(second.irq_guard()).is_none());
        *a.lock_with(second.irq_guard()) += *second;
        drop(second);
        assert!(arch::local_irq_enabled());
        assert!(!execution_guard_held());
        assert_eq!(*a.lock(), 3);
    }

    #[test]
    fn test_spinlock_preserves_outer_irq_mask() {
        let outer = DisableInterruptGuard::new();
        let plain = SpinLock::new(());
        let irq = SpinLockIrq::new(());
        let held = plain.lock();
        let nested = irq.try_lock_with(&outer).unwrap();
        assert!(plain.try_lock().is_none());
        assert!(irq.try_lock().is_none());
        drop(held);
        drop(nested);
        assert!(!arch::local_irq_enabled());
        drop(outer);
        assert!(arch::local_irq_enabled());
    }

    #[test(thread = 2)]
    fn test_exclusive_spinlock_updates() {
        // Cell is Send but not Sync. Both locks must serialize every access.
        static PLAIN: SpinLock<Cell<usize>> = SpinLock::new(Cell::new(0));
        static IRQ: SpinLockIrq<Cell<usize>> = SpinLockIrq::new(Cell::new(0));
        static FINISHED: AtomicUsize = AtomicUsize::new(0);
        const UPDATES: usize = 256;
        for _ in 0..UPDATES {
            {
                let value = PLAIN.lock();
                value.set(value.get() + 1);
            }
            let value = IRQ.lock();
            value.set(value.get() + 1);
        }
        if FINISHED.fetch_add(1, Ordering::AcqRel) == 1 {
            assert_eq!(PLAIN.lock().replace(0), 2 * UPDATES);
            assert_eq!(IRQ.lock().replace(0), 2 * UPDATES);
            FINISHED.store(0, Ordering::Relaxed);
        }
    }
}
