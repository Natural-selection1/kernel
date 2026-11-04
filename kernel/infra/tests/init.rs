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

//! Allocation, rollback and pinning checks against the real infra library.
#![feature(allocator_api)]
use allocation_test_support::{observe, Counts, TestAllocator};
use blueos_infra::init::{InPlaceInit, InPlaceWrite, Init, PinInit};
use pin_init::{pin_data, pin_init, pinned_drop, PinnedDrop};
use std::{
    alloc::AllocError,
    cell::Cell,
    marker::PhantomPinned,
    panic::{catch_unwind, AssertUnwindSafe},
    pin::Pin,
    ptr,
};

#[global_allocator]
static ALLOCATOR: TestAllocator = TestAllocator;

#[derive(Debug, PartialEq)]
enum Error {
    Allocation,
    Initializer,
}
impl From<AllocError> for Error {
    fn from(_: AllocError) -> Self {
        Self::Allocation
    }
}

fn infallible(called: &Cell<bool>) -> impl Init<u64> + '_ {
    // SAFETY: Writes the entire value once, without publishing its address.
    unsafe {
        pin_init::init_from_closure(move |slot: *mut u64| {
            called.set(true);
            slot.write(42);
            Ok(())
        })
    }
}
fn fallible(called: &Cell<bool>) -> impl Init<u64, Error> + '_ {
    // SAFETY: Always returns an error without writing or publishing anything.
    unsafe {
        pin_init::init_from_closure(move |slot: *mut u64| {
            std::hint::black_box(slot);
            called.set(true);
            Err(Error::Initializer)
        })
    }
}
fn oom_counts() -> Counts {
    Counts {
        attempts: 1,
        allocations: 0,
        deallocations: 0,
    }
}
fn released_counts() -> Counts {
    Counts {
        attempts: 1,
        allocations: 1,
        deallocations: 1,
    }
}

#[test]
fn all_four_methods_skip_initializer_on_oom() {
    let called = Cell::new(false);
    let (result, counts) = observe(true, || Box::init(infallible(&called)));
    assert_eq!(result, Err(AllocError));
    assert_eq!(counts, oom_counts());
    let (result, counts) = observe(true, || Box::pin_init(infallible(&called)));
    assert_eq!(result, Err(AllocError));
    assert_eq!(counts, oom_counts());
    let (result, counts) = observe(true, || Box::try_init(fallible(&called)));
    assert_eq!(result, Err(Error::Allocation));
    assert_eq!(counts, oom_counts());
    let (result, counts) = observe(true, || Box::try_pin_init(fallible(&called)));
    assert_eq!(result, Err(Error::Allocation));
    assert_eq!(counts, oom_counts());
    assert!(!called.get());
}

#[test]
fn initializer_error_is_preserved_and_storage_released() {
    let called = Cell::new(false);
    let (result, counts) = observe(false, || Box::try_init(fallible(&called)));
    assert_eq!(result, Err(Error::Initializer));
    assert!(called.get());
    assert_eq!(counts, released_counts());
    called.set(false);
    let (result, counts) = observe(false, || Box::try_pin_init(fallible(&called)));
    assert_eq!(result, Err(Error::Initializer));
    assert!(called.get());
    assert_eq!(counts, released_counts());
}

#[must_use]
#[derive(Debug)]
struct Tracked<'a>(&'a Cell<usize>);
impl Drop for Tracked<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
fn tracked(drops: &Cell<usize>, fail: bool, panic: bool) -> impl Init<Tracked<'_>, Error> {
    // SAFETY: Writes a complete value on success; error/unwind leaves no value.
    unsafe {
        pin_init::init_from_closure(move |slot: *mut Tracked<'_>| {
            assert!(!panic, "injected initializer panic");
            if fail {
                return Err(Error::Initializer);
            }
            slot.write(Tracked(drops));
            Ok(())
        })
    }
}

#[test]
fn ordinary_init_can_move_and_drops_once() {
    let drops = Cell::new(0);
    let (_, counts) = observe(false, || {
        let boxed = Box::try_init(tracked(&drops, false, false)).unwrap();
        let moved = *std::hint::black_box(boxed);
        drop(std::hint::black_box(moved));
    });
    assert_eq!(drops.get(), 1);
    assert_eq!(counts, released_counts());
}

#[test]
fn high_alignment_and_zero_size_use_standard_box_rules() {
    #[repr(align(4096))]
    struct Aligned(u64);
    let boxed = Box::init(Aligned(42)).unwrap();
    assert_eq!((&*boxed as *const Aligned as usize) % 4096, 0);
    assert_eq!(boxed.0, 42);
    #[repr(align(4096))]
    struct Zero;
    let (aligned, counts) = observe(true, || {
        let value = Box::pin_init(Zero).unwrap();
        let aligned = (&*value as *const Zero).is_aligned();
        drop(value);
        aligned
    });
    assert!(aligned);
    assert_eq!(counts, Counts::default());
    let (_, counts) = observe(true, || drop(Box::init(()).unwrap()));
    assert_eq!(counts, Counts::default());
}

#[pin_data(PinnedDrop)]
#[must_use]
struct Addressed<'a> {
    address: *const u32,
    value: u32,
    drops: &'a Cell<usize>,
    #[pin]
    _pin: PhantomPinned,
}
#[pinned_drop]
impl PinnedDrop for Addressed<'_> {
    fn drop(self: Pin<&mut Self>) {
        assert_eq!(self.address, &raw const self.value);
        self.drops.set(self.drops.get() + 1);
    }
}
fn addressed(drops: &Cell<usize>) -> impl PinInit<Addressed<'_>> {
    // SAFETY: Initialize all fields in their final slot; never move them. The
    // pointer only reads value while the pinned owner is alive.
    unsafe {
        pin_init::pin_init_from_closure(move |slot: *mut Addressed<'_>| {
            slot.write(Addressed {
                address: ptr::null(),
                value: 42,
                drops,
                _pin: PhantomPinned,
            });
            (&raw mut (*slot).address).write(&raw const (*slot).value);
            Ok(())
        })
    }
}
#[test]
fn pinned_value_survives_owner_move() {
    let drops = Cell::new(0);
    let value = Box::pin_init(addressed(&drops)).unwrap();
    let moved = std::hint::black_box(value);
    // SAFETY: Pointer targets the live, initialized, immutable pinned field.
    assert_eq!(unsafe { moved.address.read() }, 42);
    drop(moved);
    assert_eq!(drops.get(), 1);
}

#[pin_data]
#[must_use]
struct Pair<'a> {
    #[pin]
    first: Tracked<'a>,
    #[pin]
    second: Tracked<'a>,
}

#[test]
fn partial_fields_rollback_before_freeing() {
    let drops = &Cell::new(0);
    let (result, counts) = observe(false, || {
        Box::try_pin_init(pin_init!(Pair {
        first <- tracked(drops, false, false),
        second <- tracked(drops, true, false),
    }? Error))
    });
    assert!(matches!(result, Err(Error::Initializer)));
    assert_eq!(drops.get(), 1);
    assert_eq!(counts, released_counts());
}

#[test]
fn partial_array_drops_only_completed_elements() {
    let drops = Cell::new(0);
    let (result, counts) = observe(false, || {
        Box::<[Tracked<'_>; 5]>::try_init(pin_init::init_array_from_fn(|i| {
            tracked(&drops, i == 3, false)
        }))
    });
    assert!(matches!(result, Err(Error::Initializer)));
    assert_eq!(drops.get(), 3);
    assert_eq!(counts, released_counts());
    drops.set(0);
    let (result, counts) = observe(false, || {
        Box::<[Tracked<'_>; 5]>::try_pin_init(pin_init::pin_init_array_from_fn(|i| {
            tracked(&drops, i == 2, false)
        }))
    });
    assert!(matches!(result, Err(Error::Initializer)));
    assert_eq!(drops.get(), 2);
    assert_eq!(counts, released_counts());
}

#[test]
fn existing_uninit_box_transfers_ownership_without_allocating() {
    let drops = Cell::new(0);
    let mut storage = Box::<Tracked<'_>>::new_uninit();
    let original = storage.as_mut_ptr();
    let (value, counts) = observe(true, || storage.write_init(tracked(&drops, false, false)));
    assert_eq!(counts, Counts::default());
    let value = value.unwrap();
    assert!(ptr::eq(&*value, original));
    let (_, counts) = observe(false, || drop(value));
    assert_eq!(drops.get(), 1);
    assert_eq!(counts.deallocations, 1);
}

#[test]
fn existing_uninit_box_can_pin_initialize() {
    let drops = Cell::new(0);
    let storage = Box::<Addressed<'_>>::new_uninit();
    let (value, counts) = observe(true, || storage.write_pin_init(addressed(&drops)));
    assert_eq!(counts, Counts::default());
    let value = std::hint::black_box(value.unwrap());
    assert_eq!(value.address, &raw const value.value);
    drop(value);
    assert_eq!(drops.get(), 1);
}

#[test]
fn existing_uninit_box_failure_frees_without_dropping_complete_value() {
    let drops = Cell::new(0);
    let storage = Box::<Tracked<'_>>::new_uninit();
    let (result, counts) = observe(false, || storage.write_init(tracked(&drops, true, false)));
    assert!(matches!(result, Err(Error::Initializer)));
    assert_eq!(drops.get(), 0);
    assert_eq!(
        counts,
        Counts {
            attempts: 0,
            allocations: 0,
            deallocations: 1
        }
    );
    let storage = Box::<Tracked<'_>>::new_uninit();
    let (result, counts) = observe(false, || {
        storage.write_pin_init(tracked(&drops, true, false))
    });
    assert!(matches!(result, Err(Error::Initializer)));
    assert_eq!(drops.get(), 0);
    assert_eq!(
        counts,
        Counts {
            attempts: 0,
            allocations: 0,
            deallocations: 1
        }
    );
}

#[test]
fn panic_cleans_partial_fields_and_array() {
    let drops = &Cell::new(0);
    let result = catch_unwind(AssertUnwindSafe(|| {
        let _value = Box::try_pin_init(pin_init!(Pair {
            first <- tracked(drops, false, false),
            second <- tracked(drops, false, true),
        }? Error));
    }));
    assert!(result.is_err());
    assert_eq!(drops.get(), 1);
    drops.set(0);
    let result = catch_unwind(AssertUnwindSafe(|| {
        let _value = Box::<[Tracked<'_>; 5]>::try_init(pin_init::init_array_from_fn(|i| {
            tracked(drops, false, i == 3)
        }));
    }));
    assert!(result.is_err());
    assert_eq!(drops.get(), 3);
    // Miri's leak check also verifies the abandoned Box storage is released.
}

#[test]
fn panic_releases_existing_uninitialized_storage() {
    let drops = &Cell::new(0);
    let storage = Box::<Pair<'_>>::new_uninit();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let _value = storage.write_pin_init(pin_init!(Pair {
            first <- tracked(drops, false, false),
            second <- tracked(drops, false, true),
        }? Error));
    }));
    assert!(result.is_err());
    assert_eq!(drops.get(), 1);
}

mod opaque;
