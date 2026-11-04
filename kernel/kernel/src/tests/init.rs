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
