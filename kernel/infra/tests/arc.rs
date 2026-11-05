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

//! Public contracts, linked against the production infra library.
#![feature(allocator_api)]
use allocation_test_support::{observe, Counts, TestAllocator};
use blueos_infra::arc::{Arc, UniqueArc};
use std::{
    cell::Cell,
    hint::black_box,
    marker::PhantomPinned,
    pin::Pin,
    sync::atomic::{AtomicUsize, Ordering},
};
#[global_allocator]
static ALLOCATOR: TestAllocator = TestAllocator;

#[must_use]
struct Tracked<'a>(&'a Cell<usize>);
impl Drop for Tracked<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
fn released() -> Counts {
    Counts {
        attempts: 1,
        allocations: 1,
        deallocations: 1,
    }
}

#[test]
fn failed_constructors_drop_input_without_deallocation() {
    let drops = Cell::new(0);
    let (result, counts) = observe(true, || Arc::try_new(Tracked(&drops)));
    assert!(result.is_err());
    assert_eq!(drops.get(), 1);
    assert_eq!(
        counts,
        Counts {
            attempts: 1,
            allocations: 0,
            deallocations: 0
        }
    );
    let (result, counts) = observe(true, || UniqueArc::try_new(Tracked(&drops)));
    assert!(result.is_err());
    assert_eq!(drops.get(), 2);
    assert_eq!(
        counts,
        Counts {
            attempts: 1,
            allocations: 0,
            deallocations: 0
        }
    );
}
#[test]
fn unique_mutation_and_move_release_once() {
    let drops = Cell::new(0);
    let (_, counts) = observe(false, || {
        let mut value = UniqueArc::try_new((Tracked(&drops), 3)).unwrap();
        value.1 = 7;
        let moved = black_box(value);
        assert_eq!(moved.1, 7);
        drop(moved);
    });
    assert_eq!(drops.get(), 1);
    assert_eq!(counts, released());
}
#[test]
fn clone_keeps_allocation_until_last_owner() {
    let drops = Cell::new(0);
    let (_, counts) = observe(false, || {
        let value = black_box(Arc::try_new(Tracked(&drops)).unwrap());
        let second = value.clone();
        assert!(Arc::ptr_eq(&value, &second));
        assert_eq!(Arc::as_ptr(&value), Arc::as_ptr(&second));
        drop(value);
        assert_eq!(drops.get(), 0);
        drop(black_box(second));
    });
    assert_eq!(drops.get(), 1);
    assert_eq!(counts, released());
}
#[test]
fn sharing_and_pinning_keep_addresses_without_allocating() {
    let unique = UniqueArc::try_new(PhantomPinned).unwrap();
    let address = &*unique as *const _;
    let (shared, counts) = observe(true, || Arc::from(unique));
    assert_eq!(counts, Counts::default());
    assert_eq!(address, Arc::as_ptr(&shared));
    let _: Pin<&PhantomPinned> = shared.as_pin_ref();
    let unique = UniqueArc::try_new(PhantomPinned).unwrap();
    let address = &*unique as *const _;
    let (shared, counts) = observe(true, || {
        let pinned: Pin<UniqueArc<_>> = unique.into();
        Arc::from(black_box(pinned))
    });
    assert_eq!(counts, Counts::default());
    assert_eq!(address, Arc::as_ptr(&shared));
}
#[test]
fn alignment_and_zero_sized_payload_still_allocate_control_blocks() {
    #[repr(align(4096))]
    struct Aligned(u32);
    #[repr(align(4096))]
    struct Zero;
    let (_, counts) = observe(false, || {
        let value = black_box(Arc::try_new(Aligned(42)).unwrap());
        assert!(Arc::as_ptr(&value).is_aligned());
        assert_eq!(value.0, 42);
    });
    assert_eq!(counts, released());
    let (_, counts) = observe(false, || {
        let value = black_box(Arc::try_new(Zero).unwrap());
        assert!(Arc::as_ptr(&value).is_aligned());
    });
    assert_eq!(counts, released());
    let (result, counts) = observe(true, || UniqueArc::try_new(()));
    assert!(result.is_err());
    assert_eq!(counts.attempts, 1);
}
#[test]
fn separate_allocations_are_not_equal() {
    let a = Arc::try_new(()).unwrap();
    let b = Arc::try_new(()).unwrap();
    assert!(!Arc::ptr_eq(&a, &b));
}
#[test]
fn unique_send_does_not_require_sync_payload() {
    let value = UniqueArc::try_new(Cell::new(7)).unwrap();
    assert_eq!(
        std::thread::spawn(move || {
            value.set(9);
            value.get()
        })
        .join()
        .unwrap(),
        9
    );
}
#[test]
fn concurrent_clones_drop_payload_once() {
    #[must_use]
    struct Concurrent<'a> {
        drops: &'a AtomicUsize,
        writes: AtomicUsize,
        expected: usize,
    }
    impl Drop for Concurrent<'_> {
        fn drop(&mut self) {
            // The final release must observe each worker's writes before it
            // relinquished ownership, even before the joining thread runs.
            assert_eq!(self.writes.load(Ordering::Relaxed), self.expected);
            self.drops.fetch_add(1, Ordering::Relaxed);
        }
    }
    let drops = AtomicUsize::new(0);
    let iterations = if cfg!(miri) { 4 } else { 128 };
    let owner = Arc::try_new(Concurrent {
        drops: &drops,
        writes: AtomicUsize::new(0),
        expected: 3 * iterations,
    })
    .unwrap();
    std::thread::scope(|scope| {
        for _ in 0..3 {
            let value = owner.clone();
            scope.spawn(move || {
                for _ in 0..iterations {
                    value.writes.fetch_add(1, Ordering::Relaxed);
                    drop(black_box(value.clone()));
                }
            });
        }
        drop(owner);
    });
    assert_eq!(drops.load(Ordering::Relaxed), 1);
}
