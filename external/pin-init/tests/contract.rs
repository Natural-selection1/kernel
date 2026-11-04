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

//! Contract checks against the compiled pin-init library (alloc, without std).
#![feature(allocator_api)]

use allocation_test_support::{observe, Counts, TestAllocator};
use pin_init::{
    init, pin_data, pin_init, pinned_drop, stack_pin_init, InPlaceInit, PinInit, PinnedDrop,
    Wrapper,
};
use std::{
    cell::{Cell, UnsafeCell},
    marker::PhantomPinned,
    pin::Pin,
    ptr::NonNull,
};

#[global_allocator]
static ALLOCATOR: TestAllocator = TestAllocator;

#[pin_data(PinnedDrop)]
#[must_use]
struct MacroValue<'a> {
    value: u32,
    drops: &'a Cell<usize>,
    #[pin]
    _pin: PhantomPinned,
}
#[pinned_drop]
impl PinnedDrop for MacroValue<'_> {
    fn drop(self: Pin<&mut Self>) {
        self.drops.set(self.drops.get() + 1);
    }
}

#[test]
fn macros_initialize_and_run_pinned_drop() {
    let counter = Cell::new(0);
    let drops = &counter;
    let value = Box::pin_init(pin_init!(MacroValue {
        value: 42,
        drops,
        _pin: PhantomPinned
    }))
    .unwrap();
    let moved = std::hint::black_box(value);
    assert_eq!(moved.value, 42);
    drop(moved);
    assert_eq!(drops.get(), 1);
    struct Plain {
        value: u32,
    }
    let plain = Box::init(init!(Plain { value: 7 })).unwrap();
    assert_eq!(plain.value, 7);
}

struct Registry {
    address: Cell<Option<NonNull<u32>>>,
}
#[must_use]
struct Registered<'a> {
    registry: &'a Registry,
    value: u32,
    _pin: PhantomPinned,
}
impl Drop for Registered<'_> {
    fn drop(&mut self) {
        self.registry.address.set(None);
    }
}
impl Registry {
    fn new() -> Self {
        Self {
            address: Cell::new(None),
        }
    }
    fn initializer(&self) -> impl PinInit<Registered<'_>> + '_ {
        // SAFETY: All fields are written before registration. PinInit's caller
        // keeps the value pinned until Drop deregisters it before deallocation.
        // The lifetime in Registered keeps this registry alive throughout.
        unsafe {
            pin_init::pin_init_from_closure(move |slot: *mut Registered<'_>| {
                slot.write(Registered {
                    registry: self,
                    value: 42,
                    _pin: PhantomPinned,
                });
                self.address
                    .set(Some(NonNull::new_unchecked(&raw mut (*slot).value)));
                Ok(())
            })
        }
    }
    fn read(&self) -> Option<u32> {
        // SAFETY: Registered::drop clears this initialized, immutable field's
        // address before its storage is invalidated.
        self.address.get().map(|p| unsafe { p.as_ptr().read() })
    }
}

#[test]
fn ordinary_box_deregisters_before_freeing() {
    let registry = Registry::new();
    let value = Box::pin_init(registry.initializer()).unwrap();
    assert_eq!(registry.read(), Some(42));
    drop(value);
    assert_eq!(registry.read(), None);
}

#[test]
fn unsafe_cell_wrapper_preserves_inner_destruction() {
    let registry = Registry::new();
    let value = Box::pin_init(UnsafeCell::pin_init(registry.initializer())).unwrap();
    assert_eq!(registry.read(), Some(42));
    drop(value);
    assert_eq!(registry.read(), None);
}

#[test]
fn stack_initialization_needs_no_allocator_and_drops_in_place() {
    let registry = Registry::new();
    let (value, counts) = observe(true, || {
        stack_pin_init!(let value = registry.initializer());
        value.value
    });
    assert_eq!(value, 42);
    assert_eq!(counts, Counts::default());
    assert_eq!(registry.read(), None);
}
