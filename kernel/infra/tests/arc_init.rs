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

//! Initialization contracts of the actual Arc and UniqueArc implementations.
use allocation_test_support::{observe, Counts};
use blueos_infra::{
    arc::{Arc, UniqueArc},
    init::{InPlaceInit, InPlaceWrite, Init, PinInit},
};
use pin_init::{pin_data, pin_init, pinned_drop, PinnedDrop};
use std::{
    alloc::AllocError,
    cell::Cell,
    marker::PhantomPinned,
    panic::{catch_unwind, AssertUnwindSafe},
    pin::Pin,
    ptr,
};

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
fn all_eight_methods_skip_initializer_on_oom() {
    let called = Cell::new(false);
    macro_rules! check_owner {
        ($owner:ident) => {{
            let (result, counts) = observe(true, || $owner::init(infallible(&called)));
            assert!(matches!(result, Err(AllocError)));
            assert_eq!(counts, oom_counts());
            let (result, counts) = observe(true, || $owner::pin_init(infallible(&called)));
            assert!(matches!(result, Err(AllocError)));
            assert_eq!(counts, oom_counts());
            let (result, counts) = observe(true, || $owner::try_init(fallible(&called)));
            assert!(matches!(result, Err(Error::Allocation)));
            assert_eq!(counts, oom_counts());
            let (result, counts) = observe(true, || $owner::try_pin_init(fallible(&called)));
            assert!(matches!(result, Err(Error::Allocation)));
            assert_eq!(counts, oom_counts());
        }};
    }
    check_owner!(UniqueArc);
    check_owner!(Arc);
    assert!(!called.get());
}

#[test]
fn initializer_errors_release_both_owner_paths() {
    let called = Cell::new(false);
    macro_rules! check_owner {
        ($owner:ident) => {{
            let (result, counts) = observe(false, || $owner::try_init(fallible(&called)));
            assert!(matches!(result, Err(Error::Initializer)));
            assert!(called.replace(false));
            assert_eq!(counts, released_counts());
            let (result, counts) = observe(false, || $owner::try_pin_init(fallible(&called)));
            assert!(matches!(result, Err(Error::Initializer)));
            assert!(called.replace(false));
            assert_eq!(counts, released_counts());
        }};
    }
    check_owner!(UniqueArc);
    check_owner!(Arc);
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
fn ordinary_init_allows_replacing_and_moving_payload() {
    let drops = Cell::new(0);
    let (_, counts) = observe(false, || {
        let unique = UniqueArc::try_init(tracked(&drops, false, false)).unwrap();
        let mut unique = std::hint::black_box(unique);
        let moved = std::mem::replace(&mut *unique, Tracked(&drops));
        drop(std::hint::black_box(moved));
    });
    assert_eq!(drops.get(), 2);
    assert_eq!(counts, released_counts());
}

#[test]
fn high_alignment_and_zero_size_initialize_control_blocks() {
    #[repr(align(4096))]
    struct Aligned(u64);
    let value = Arc::init(Aligned(42)).unwrap();
    assert!(Arc::as_ptr(&value).is_aligned());
    assert_eq!(value.0, 42);
    #[repr(align(4096))]
    struct Zero;
    let (_, counts) = observe(false, || {
        let value = std::hint::black_box(UniqueArc::pin_init(Zero).unwrap());
        assert!((&*value as *const Zero).is_aligned());
    });
    assert_eq!(counts, released_counts());
    let (_, counts) = observe(false, || drop(std::hint::black_box(Arc::init(()).unwrap())));
    assert_eq!(counts, released_counts());
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
    let value = UniqueArc::pin_init(addressed(&drops)).unwrap();
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
        Arc::try_pin_init(pin_init!(Pair {
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
        UniqueArc::<[Tracked<'_>; 5]>::try_init(pin_init::init_array_from_fn(|i| {
            tracked(&drops, i == 3, false)
        }))
    });
    assert!(matches!(result, Err(Error::Initializer)));
    assert_eq!(drops.get(), 3);
    assert_eq!(counts, released_counts());
    drops.set(0);
    let (result, counts) = observe(false, || {
        UniqueArc::<[Tracked<'_>; 5]>::try_pin_init(pin_init::pin_init_array_from_fn(|i| {
            tracked(&drops, i == 2, false)
        }))
    });
    assert!(matches!(result, Err(Error::Initializer)));
    assert_eq!(drops.get(), 2);
    assert_eq!(counts, released_counts());
}

#[test]
fn existing_uninit_unique_transfers_ownership_without_allocating() {
    let drops = Cell::new(0);
    let storage = UniqueArc::<Tracked<'_>>::try_new_uninit().unwrap();
    let original = (&*storage as *const std::mem::MaybeUninit<Tracked<'_>>).cast::<Tracked<'_>>();
    let (value, counts) = observe(true, || storage.write_init(tracked(&drops, false, false)));
    assert_eq!(counts, Counts::default());
    let value = value.unwrap();
    assert!(ptr::eq(&*value, original));
    let (_, counts) = observe(false, || drop(value));
    assert_eq!(drops.get(), 1);
    assert_eq!(counts.deallocations, 1);
}

#[test]
fn existing_uninit_unique_can_pin_initialize() {
    let drops = Cell::new(0);
    let storage = UniqueArc::<Addressed<'_>>::try_new_uninit().unwrap();
    let (value, counts) = observe(true, || storage.write_pin_init(addressed(&drops)));
    assert_eq!(counts, Counts::default());
    let value = std::hint::black_box(value.unwrap());
    assert_eq!(value.address, &raw const value.value);
    drop(value);
    assert_eq!(drops.get(), 1);
}

#[test]
fn existing_uninit_unique_failure_frees_without_dropping_complete_value() {
    let drops = Cell::new(0);
    let storage = UniqueArc::<Tracked<'_>>::try_new_uninit().unwrap();
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
    let storage = UniqueArc::<Tracked<'_>>::try_new_uninit().unwrap();
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
        let _value = Arc::try_pin_init(pin_init!(Pair {
            first <- tracked(drops, false, false),
            second <- tracked(drops, false, true),
        }? Error));
    }));
    assert!(result.is_err());
    assert_eq!(drops.get(), 1);
    drops.set(0);
    let result = catch_unwind(AssertUnwindSafe(|| {
        let _value = UniqueArc::<[Tracked<'_>; 5]>::try_init(pin_init::init_array_from_fn(|i| {
            tracked(drops, false, i == 3)
        }));
    }));
    assert!(result.is_err());
    assert_eq!(drops.get(), 3);
    // Miri's leak check also verifies the abandoned UniqueArc storage is released.
}

#[test]
fn panic_releases_existing_uninitialized_storage() {
    let drops = &Cell::new(0);
    let storage = UniqueArc::<Pair<'_>>::try_new_uninit().unwrap();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let _value = storage.write_pin_init(pin_init!(Pair {
            first <- tracked(drops, false, false),
            second <- tracked(drops, false, true),
        }? Error));
    }));
    assert!(result.is_err());
    assert_eq!(drops.get(), 1);
}

#[test]
fn pinned_to_shared_then_cloned_keeps_self_pointer_and_drops_once() {
    let drops = Cell::new(0);
    let unique = UniqueArc::pin_init(addressed(&drops)).unwrap();
    let original = &*unique as *const Addressed<'_>;
    let (shared, counts) = observe(true, || {
        let shared = Arc::from(std::hint::black_box(unique));
        let clone = shared.clone();
        drop(shared);
        clone
    });
    assert_eq!(counts, Counts::default());
    assert_eq!(original, Arc::as_ptr(&shared));
    // SAFETY: The stored pointer refers to this live pinned, immutable field.
    assert_eq!(unsafe { shared.address.read() }, 42);
    let (_, counts) = observe(false, || drop(std::hint::black_box(shared)));
    assert_eq!(counts.deallocations, 1);
    assert_eq!(drops.get(), 1);

    let value = Arc::pin_init(addressed(&drops)).unwrap();
    let clone = value.clone();
    drop(value);
    // SAFETY: Clone retains the initialized field's allocation and pinning.
    assert_eq!(unsafe { clone.address.read() }, 42);
    drop(clone);
    assert_eq!(drops.get(), 2);
}

struct Registry {
    address: Cell<*const u32>,
    drops: Cell<usize>,
}
#[must_use]
struct Registered<'a> {
    registry: &'a Registry,
    number: u32,
    _pin: PhantomPinned,
}
impl Drop for Registered<'_> {
    fn drop(&mut self) {
        self.registry.address.set(ptr::null());
        self.registry.drops.set(self.registry.drops.get() + 1);
    }
}
impl Registry {
    fn initializer(&self) -> impl PinInit<Registered<'_>> {
        // SAFETY: All fields are initialized before registering the address.
        // The owner must preserve this address through Drop, which unregisters
        // it before deallocation. Registered's lifetime keeps Registry alive.
        unsafe {
            pin_init::pin_init_from_closure(move |slot: *mut Registered<'_>| {
                slot.write(Registered {
                    registry: self,
                    number: 42,
                    _pin: PhantomPinned,
                });
                self.address.set(&raw const (*slot).number);
                Ok(())
            })
        }
    }
    fn read(&self) -> Option<u32> {
        let pointer = self.address.get();
        // SAFETY: Only initializer registers, after complete initialization;
        // the value is immutable and Drop unregisters it before invalidation.
        if pointer.is_null() {
            None
        } else {
            Some(unsafe { pointer.read() })
        }
    }
}
#[test]
fn pinned_and_shared_owners_unregister_before_freeing() {
    let registry = Registry {
        address: Cell::new(ptr::null()),
        drops: Cell::new(0),
    };
    let value = UniqueArc::pin_init(registry.initializer()).unwrap();
    assert_eq!(registry.read(), Some(42));
    drop(std::hint::black_box(value));
    assert_eq!(registry.read(), None);
    assert_eq!(registry.drops.get(), 1);
    let value = Arc::pin_init(registry.initializer()).unwrap();
    let clone = value.clone();
    drop(value);
    assert_eq!(registry.read(), Some(42));
    drop(std::hint::black_box(clone));
    assert_eq!(registry.read(), None);
    assert_eq!(registry.drops.get(), 2);
}

#[test]
fn uninitialized_owner_does_not_drop_payload() {
    let (_, counts) = observe(false, || {
        let storage = UniqueArc::<Tracked<'_>>::try_new_uninit().unwrap();
        drop(std::hint::black_box(storage));
    });
    assert_eq!(counts, released_counts());
    // Tracked::drop dereferences its field: interpreting uninitialized storage
    // as a complete Tracked would fail Miri rather than merely leak a counter.
}

#[test]
fn panicking_payload_destructor_still_frees_control_block() {
    #[must_use]
    struct Panics;
    impl Drop for Panics {
        fn drop(&mut self) {
            panic!("payload drop panic");
        }
    }
    let owner = Arc::init(Panics).unwrap();
    let result = catch_unwind(AssertUnwindSafe(|| drop(owner)));
    assert!(result.is_err());
    // Miri's leak check verifies that Box's cleanup released the allocation.
}
