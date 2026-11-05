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

//! Exercise initialization with the actual kernel global allocator.
use alloc::boxed::Box;
use blueos_infra::init::InPlaceInit;
use blueos_test_macro::test;
use core::{cell::Cell, marker::PhantomPinned, pin::Pin};
use pin_init::{pin_data, pin_init, pinned_drop, PinnedDrop};

#[pin_data(PinnedDrop)]
#[must_use]
struct Value<'a> {
    number: u32,
    drops: &'a Cell<usize>,
    #[pin]
    _pin: PhantomPinned,
}
#[pinned_drop]
impl PinnedDrop for Value<'_> {
    fn drop(self: Pin<&mut Self>) {
        self.drops.set(self.drops.get() + 1);
    }
}

#[test]
fn pin_init_kernel_allocator_smoke() {
    let drops = &Cell::new(0);
    let value = Box::try_pin_init(pin_init!(Value {
        number: 42, drops, _pin: PhantomPinned,
    }? crate::error::Error))
    .unwrap();
    let moved = core::hint::black_box(value);
    assert_eq!(moved.number, 42);
    drop(moved);
    assert_eq!(drops.get(), 1);
    let ordinary = Box::init(7u32).unwrap();
    assert_eq!(*ordinary, 7);
}

#[test]
fn arc_kernel_allocator_smoke() {
    use blueos_infra::{
        arc::{Arc, UniqueArc},
        init::InPlaceWrite,
    };
    let drops = &Cell::new(0);
    let value = UniqueArc::try_pin_init(pin_init!(Value {
        number: 42, drops, _pin: PhantomPinned,
    }? crate::error::Error))
    .unwrap();
    let original = &*value as *const Value<'_>;
    let shared: Arc<_> = value.into();
    let clone = shared.clone();
    drop(shared);
    assert_eq!(Arc::as_ptr(&clone), original);
    assert_eq!(clone.as_pin_ref().number, 42);
    assert_eq!(drops.get(), 0);
    drop(clone);
    assert_eq!(drops.get(), 1);
    let direct = Arc::pin_init(pin_init!(Value {
        number: 7,
        drops,
        _pin: PhantomPinned,
    }))
    .unwrap();
    assert_eq!(direct.number, 7);
    drop(direct);
    assert_eq!(drops.get(), 2);
    let storage = UniqueArc::<u32>::try_new_uninit().unwrap();
    let mut unique = storage.write_init(9u32).unwrap();
    *unique = 10;
    let shared: Arc<_> = unique.into();
    assert_eq!(*shared, 10);
}
