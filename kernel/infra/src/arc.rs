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

//! Heap-only strong owners with an intrinsically pinned shared state.
//!
//! `UniqueArc<T>` may move/replace its value until converted into
//! `Pin<UniqueArc<T>>` or `Arc<T>`. Those transitions keep the allocation and
//! guarantee that the value stays at its address until its destructor runs.
//! There is deliberately no shared-to-unique or owning raw-pointer API.
//!
//! A live owner keeps the count nonzero while cloning. Relaxed increments only
//! extend this lifetime; publishing an owner requires separate synchronization.
//! Every release is an atomic RMW with Release ordering. The last releaser's
//! Acquire fence observes the release sequence before destroying the payload.
//! CAS updates never wrap: usize::MAX is absorbing, including for releases.
//! Saturation therefore leaks the allocation instead of freeing live data.

extern crate alloc;
use alloc::{alloc::AllocError, boxed::Box};
use core::{
    marker::PhantomData,
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
    pin::Pin,
    ptr::NonNull,
    sync::atomic::{fence, AtomicUsize, Ordering},
};

struct Refcount(AtomicUsize);
impl Refcount {
    fn new() -> Self {
        Self(AtomicUsize::new(1))
    }

    // The caller owns a live reference throughout the operation.
    fn acquire(&self) {
        let mut count = self.0.load(Ordering::Relaxed);
        loop {
            debug_assert_ne!(count, 0);
            if count == usize::MAX {
                return;
            }
            match self.0.compare_exchange_weak(
                count,
                count + 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return,
                Err(actual) => count = actual,
            }
        }
    }

    // Consumes one live reference; true grants exclusive destruction rights.
    fn release(&self) -> bool {
        let mut count = self.0.load(Ordering::Relaxed);
        loop {
            debug_assert_ne!(count, 0);
            if count == usize::MAX {
                return false;
            }
            match self.0.compare_exchange_weak(
                count,
                count - 1,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    if count == 1 {
                        fence(Ordering::Acquire);
                        return true;
                    }
                    return false;
                }
                Err(actual) => count = actual,
            }
        }
    }
}

#[repr(C)]
struct ArcInner<T> {
    count: Refcount,
    data: T,
}

/// A shared owner that pins its payload, even without a `Pin<Arc<T>>` wrapper.
///
/// Cloning never allocates. Reference-count saturation permanently leaks the
/// allocation. Raw pointers borrowed with `as_ptr` do not extend its lifetime.
#[must_use]
pub struct Arc<T> {
    ptr: NonNull<ArcInner<T>>,
    _owns: PhantomData<T>,
}

/// An exclusive owner. Its payload may move until this owner is pinned/shared.
#[must_use]
pub struct UniqueArc<T> {
    ptr: NonNull<ArcInner<T>>,
    _owns: PhantomData<T>,
}

// Moving a handle never moves the allocation. Pin<UniqueArc<T>> gates mutable
// access and extraction on the *target* T's Unpin, not the handle's Unpin.
impl<T> Unpin for Arc<T> {}
impl<T> Unpin for UniqueArc<T> {}

// SAFETY: Other owners can access T and the final owner may destroy it on any
// thread. Hence both shared access and transfer of T must be permitted.
unsafe impl<T: Send + Sync> Send for Arc<T> {}
// SAFETY: Shared access may clone an owner and send/drop it on another thread.
unsafe impl<T: Send + Sync> Sync for Arc<T> {}
// SAFETY: There are no other owners. Transfer gives exclusive access to T.
unsafe impl<T: Send> Send for UniqueArc<T> {}
// SAFETY: Borrowing an exclusive owner exposes only &T; it cannot be cloned or
// destroyed through &UniqueArc<T>. Mutation requires an exclusive borrow.
unsafe impl<T: Sync> Sync for UniqueArc<T> {}

impl<T> UniqueArc<T> {
    /// Allocate one control block. On allocation failure, `value` is dropped.
    pub fn try_new(value: T) -> Result<Self, AllocError> {
        let inner = Box::try_new(ArcInner {
            count: Refcount::new(),
            data: value,
        })?;
        Ok(Self {
            // SAFETY: Box::into_raw returns a non-null pointer to this allocation.
            ptr: unsafe { NonNull::new_unchecked(Box::into_raw(inner)) },
            _owns: PhantomData,
        })
    }
}

impl<T> Arc<T> {
    /// Allocate and share a value. Even zero-sized T needs a control block.
    pub fn try_new(value: T) -> Result<Self, AllocError> {
        UniqueArc::try_new(value).map(Self::from)
    }

    /// Borrow the payload's address without transferring ownership or granting
    /// permission to mutate it. The pointer is valid only while an owner lives.
    pub fn as_ptr(this: &Self) -> *const T {
        // SAFETY: A live owner keeps this initialized allocation alive. Project
        // through a raw pointer, without retagging the allocation via &T.
        unsafe { &raw const (*this.ptr.as_ptr()).data }
    }

    /// Whether two owners refer to the same allocation.
    pub fn ptr_eq(this: &Self, other: &Self) -> bool {
        this.ptr == other.ptr
    }

    /// Borrow the fixed-address value. No API can later move it out of this Arc.
    pub fn as_pin_ref(&self) -> Pin<&T> {
        // SAFETY: The shared state never exposes mutable/moving access, and the
        // final owner runs T's destructor before freeing its allocation.
        unsafe { Pin::new_unchecked(self.deref()) }
    }
}

impl<T> From<UniqueArc<T>> for Arc<T> {
    fn from(value: UniqueArc<T>) -> Self {
        let value = ManuallyDrop::new(value);
        Self {
            ptr: value.ptr,
            _owns: PhantomData,
        }
    }
}
impl<T> From<UniqueArc<T>> for Pin<UniqueArc<T>> {
    fn from(value: UniqueArc<T>) -> Self {
        // SAFETY: Pin gates DerefMut/extraction for !Unpin T. Drop destroys T
        // in place, and conversion into Arc preserves the pinning guarantee.
        unsafe { Pin::new_unchecked(value) }
    }
}
impl<T> From<Pin<UniqueArc<T>>> for Arc<T> {
    fn from(value: Pin<UniqueArc<T>>) -> Self {
        // SAFETY: The extracted handle is immediately consumed into a shared
        // owner. Neither this conversion nor any Arc API can move its T.
        Self::from(unsafe { Pin::into_inner_unchecked(value) })
    }
}
impl<T> Clone for Arc<T> {
    fn clone(&self) -> Self {
        // SAFETY: This owner keeps the control block alive during the increment.
        unsafe { &(*self.ptr.as_ptr()).count }.acquire();
        Self {
            ptr: self.ptr,
            _owns: PhantomData,
        }
    }
}
impl<T> Deref for Arc<T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: Live shared owner; initialized T is never moved or mutated
        // except through T's own interior mutability.
        unsafe { &(*self.ptr.as_ptr()).data }
    }
}
impl<T> Deref for UniqueArc<T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: The sole owner keeps T alive and initialized.
        unsafe { &(*self.ptr.as_ptr()).data }
    }
}
impl<T> DerefMut for UniqueArc<T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: This is the only owner, and &mut self excludes other borrows.
        // Project only the payload, not the entire allocation/refcount.
        unsafe { &mut (*self.ptr.as_ptr()).data }
    }
}

// SAFETY requirement: ptr represents exactly one live ownership reference,
// which the caller relinquishes permanently. T must be initialized.
unsafe fn release<T>(ptr: NonNull<ArcInner<T>>) {
    // SAFETY: The caller's reference keeps the count alive during release.
    if unsafe { &(*ptr.as_ptr()).count }.release() {
        // SAFETY: The last reference has acquired all preceding releases.
        // This pointer came from a Box with exactly this layout and type.
        // Reconstructing Box also frees storage if T::drop unwinds.
        drop(unsafe { Box::from_raw(ptr.as_ptr()) });
    }
}
impl<T> Drop for Arc<T> {
    fn drop(&mut self) {
        // SAFETY: Drop consumes this owner's one live reference.
        unsafe { release(self.ptr) };
    }
}
impl<T> Drop for UniqueArc<T> {
    fn drop(&mut self) {
        // SAFETY: Drop consumes the sole live reference.
        unsafe { release(self.ptr) };
    }
}

#[cfg(test)]
mod refcount_tests {
    use super::Refcount;
    use core::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn reference_count_last_release() {
        let count = Refcount::new();
        count.acquire();
        assert!(!count.release());
        assert!(count.release());
        assert_eq!(count.0.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn reference_count_saturation_is_absorbing() {
        let count = Refcount(AtomicUsize::new(usize::MAX - 2));
        count.acquire();
        assert_eq!(count.0.load(Ordering::Relaxed), usize::MAX - 1);
        assert!(!count.release());
        assert_eq!(count.0.load(Ordering::Relaxed), usize::MAX - 2);
        count.acquire();
        count.acquire();
        for _ in 0..4 {
            count.acquire();
            assert!(!count.release());
            assert_eq!(count.0.load(Ordering::Relaxed), usize::MAX);
        }
    }

    #[test]
    fn reference_count_concurrent_saturation() {
        let count = Refcount(AtomicUsize::new(usize::MAX - 2));
        std::thread::scope(|scope| {
            for _ in 0..3 {
                scope.spawn(|| {
                    count.acquire();
                });
            }
            scope.spawn(|| assert!(!count.release()));
        });
        // Three acquires and one release reach saturation in every ordering;
        // once reached, even a racing release must leave it absorbing.
        assert_eq!(count.0.load(Ordering::Relaxed), usize::MAX);
        assert!(!count.release());
    }
}
