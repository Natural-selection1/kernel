// Copyright (c) 2025 vivo Mobile Communication Co., Ltd.
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

use crate::{
    support::DisableInterruptGuard,
    types::{IRwLock, IntrusiveAdapter, NestedAdapter, RwLock, RwLockReadGuard, RwLockWriteGuard},
};
use blueos_infra::nolock::{INoLock, NoLock, NoLockReadGuard, NoLockWriteGuard};
use core::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
    sync::atomic::{compiler_fence, Ordering},
};

#[derive(Debug)]
pub struct RwSpinLock<T: ?Sized> {
    #[cfg(smp)]
    lock: RwLock<T>,
    #[cfg(not(smp))]
    lock: NoLock<T>,
}

pub type RwSpinLockWriteGuard<'a, T> = RwSpinLockGuard<'a, T>;

// See https://doc.rust-lang.org/reference/destructors.html#r-destructors.operation for dropping orders.
#[derive(Debug)]
#[repr(C)]
#[must_use]
pub struct RwSpinLockGuard<'a, T: ?Sized> {
    #[cfg(smp)]
    lock_guard: RwLockWriteGuard<'a, T>,
    #[cfg(not(smp))]
    lock_guard: NoLockWriteGuard<'a, T>,
    irq_guard: Option<DisableInterruptGuard>,
}

impl<T: ?Sized> RwSpinLockGuard<'_, T> {
    #[inline]
    pub fn take_irq_guard<S>(&mut self, other: &mut RwSpinLockGuard<'_, S>) {
        self.irq_guard = other.irq_guard.take();
    }
}

impl<'a, T: 'a + ?Sized> Deref for RwSpinLockGuard<'a, T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &T {
        self.lock_guard.deref()
    }
}

impl<'a, T: 'a + ?Sized> DerefMut for RwSpinLockGuard<'a, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        self.lock_guard.deref_mut()
    }
}

#[derive(Debug)]
#[repr(C)]
#[must_use]
pub struct RwSpinLockReadGuard<'a, T: ?Sized> {
    #[cfg(smp)]
    lock_guard: RwLockReadGuard<'a, T>,
    #[cfg(not(smp))]
    lock_guard: NoLockReadGuard<'a, T>,
    irq_guard: Option<DisableInterruptGuard>,
}

impl<T: ?Sized> RwSpinLockReadGuard<'_, T> {
    #[inline]
    pub fn take_irq_guard<S>(&mut self, other: &mut RwSpinLockReadGuard<'_, S>) {
        self.irq_guard = other.irq_guard.take();
    }
}

impl<'a, T: 'a + ?Sized> Deref for RwSpinLockReadGuard<'a, T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &T {
        self.lock_guard.deref()
    }
}

impl<T> RwSpinLock<T> {
    pub const fn new(val: T) -> Self {
        Self {
            #[cfg(smp)]
            lock: RwLock::new(val),
            #[cfg(not(smp))]
            lock: NoLock::new(val),
        }
    }
}

impl<T: ?Sized> RwSpinLock<T> {
    pub fn try_irqsave_lock(&self) -> Option<RwSpinLockGuard<'_, T>> {
        let irq_guard = DisableInterruptGuard::new();
        compiler_fence(Ordering::SeqCst);
        let mut guard = self.try_lock()?;
        debug_assert!(guard.irq_guard.is_none());
        guard.irq_guard = Some(irq_guard);
        Some(guard)
    }

    pub fn irqsave_lock(&self) -> RwSpinLockGuard<'_, T> {
        let irq_guard = DisableInterruptGuard::new();
        loop {
            let Some(mut l) = self.try_lock() else {
                core::hint::spin_loop();
                continue;
            };
            debug_assert!(l.irq_guard.is_none());
            l.irq_guard = Some(irq_guard);
            return l;
        }
    }

    pub fn try_lock(&self) -> Option<RwSpinLockGuard<'_, T>> {
        let lock_guard = self.lock.try_write()?;
        Some(RwSpinLockGuard {
            irq_guard: None,
            lock_guard,
        })
    }

    pub fn lock(&self) -> RwSpinLockGuard<'_, T> {
        loop {
            let Some(l) = self.try_lock() else {
                core::hint::spin_loop();
                continue;
            };
            return l;
        }
    }

    #[inline]
    pub fn write(&self) -> RwSpinLockWriteGuard<'_, T> {
        self.lock()
    }

    #[inline]
    pub fn try_write(&self) -> Option<RwSpinLockWriteGuard<'_, T>> {
        self.try_lock()
    }

    pub fn try_irqsave_write(&self) -> Option<RwSpinLockWriteGuard<'_, T>> {
        self.try_irqsave_lock()
    }

    pub fn irqsave_write(&self) -> RwSpinLockWriteGuard<'_, T> {
        self.irqsave_lock()
    }

    #[inline]
    pub fn try_read(&self) -> Option<RwSpinLockReadGuard<'_, T>> {
        let lock_guard = self.lock.try_read()?;
        Some(RwSpinLockReadGuard {
            irq_guard: None,
            lock_guard,
        })
    }

    #[inline]
    pub fn read(&self) -> RwSpinLockReadGuard<'_, T> {
        loop {
            let Some(l) = self.try_read() else {
                core::hint::spin_loop();
                continue;
            };
            return l;
        }
    }

    pub fn try_irqsave_read(&self) -> Option<RwSpinLockReadGuard<'_, T>> {
        let irq_guard = DisableInterruptGuard::new();
        compiler_fence(Ordering::SeqCst);
        let mut guard = self.try_read()?;
        debug_assert!(guard.irq_guard.is_none());
        guard.irq_guard = Some(irq_guard);
        Some(guard)
    }

    pub fn irqsave_read(&self) -> RwSpinLockReadGuard<'_, T> {
        loop {
            let Some(l) = self.try_irqsave_read() else {
                core::hint::spin_loop();
                continue;
            };
            return l;
        }
    }

    pub fn reader_count(&self) -> usize {
        self.lock.reader_count() as usize
    }

    pub fn writer_count(&self) -> usize {
        self.lock.writer_count() as usize
    }
}

impl<'a, T: 'a + ?Sized> RwSpinLockGuard<'a, T> {
    pub fn forget_irq(&mut self) {
        if self.irq_guard.is_none() {
            return;
        }
        core::mem::forget(self.irq_guard.take())
    }
}

unsafe impl<T: ?Sized + Send> Send for RwSpinLock<T> {}
unsafe impl<T: ?Sized + Sync> Sync for RwSpinLock<T> {}

#[derive(Default, Debug)]
struct ISpinLockOffset<T: Sized, A: const IntrusiveAdapter<T>>(PhantomData<T>, PhantomData<A>);

impl<T: Sized, A: const IntrusiveAdapter<T>> const IntrusiveAdapter<ISpinLock<T, A>>
    for ISpinLockOffset<T, A>
{
    fn offset() -> usize {
        core::mem::offset_of!(ISpinLock<T, A>, lock)
    }
}

#[allow(clippy::type_complexity)]
#[derive(Default, Debug)]
pub struct ISpinLock<T: Sized, A: const IntrusiveAdapter<T>> {
    #[cfg(smp)]
    lock: IRwLock<T, NestedAdapter<T, A, ISpinLock<T, A>, ISpinLockOffset<T, A>>>,
    #[cfg(not(smp))]
    lock: INoLock<T, NestedAdapter<T, A, ISpinLock<T, A>, ISpinLockOffset<T, A>>>,
}

impl<T: Sized, A: const IntrusiveAdapter<T>> ISpinLock<T, A> {
    pub const fn new() -> Self {
        Self {
            #[cfg(smp)]
            lock: IRwLock::new(),
            #[cfg(not(smp))]
            lock: INoLock::new(),
        }
    }

    #[inline]
    pub fn irqsave_lock(&self) -> RwSpinLockGuard<'_, T> {
        let irq_guard = DisableInterruptGuard::new();
        compiler_fence(Ordering::SeqCst);
        RwSpinLockGuard {
            lock_guard: self.lock.write(),
            irq_guard: Some(irq_guard),
        }
    }
}

unsafe impl<T: Sized + Send, A: const IntrusiveAdapter<T>> Send for ISpinLock<T, A> {}
unsafe impl<T: Sized + Sync, A: const IntrusiveAdapter<T>> Sync for ISpinLock<T, A> {}
