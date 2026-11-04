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

//! Tests for the storage shell and a separately owned pinned lifecycle.
use super::*;
use blueos_infra::opaque::Opaque;
use std::mem::{align_of, size_of};

#[test]
fn uninitialized_shell_is_valid_and_never_drops_payload() {
    // A reference's bit-pattern need not be initialized for its shell to exist.
    let shell = Opaque::<&u32>::uninit();
    drop(std::hint::black_box(shell));
    let drops = Cell::new(0);
    drop(Opaque::new(Tracked(&drops)));
    assert_eq!(drops.get(), 0);
}

#[test]
fn raw_access_controls_initialization_mutation_and_destruction() {
    let shell = Opaque::<u64>::uninit();
    // SAFETY: Valid, aligned storage; initialize before reading, with no payload
    // references or concurrent access. UnsafeCell permits these shared-shell writes.
    unsafe {
        shell.get().write(7);
    }
    let shared = &shell;
    unsafe {
        assert_eq!(shared.get().read(), 7);
        shared.get().write(42);
        assert_eq!(shell.get().read(), 42);
    }
    let drops = Cell::new(0);
    let owned = Opaque::new(Tracked(&drops));
    // SAFETY: Initialized by new, not previously destroyed, no live references.
    unsafe {
        ptr::drop_in_place(owned.get());
    }
    assert_eq!(drops.get(), 1);
    drop(owned);
    assert_eq!(drops.get(), 1);
}

#[test]
fn direct_layout_and_pointer_conversion_match_payload() {
    #[repr(align(128))]
    struct Aligned([u8; 3]);
    assert_eq!(size_of::<Opaque<Aligned>>(), size_of::<Aligned>());
    assert_eq!(align_of::<Opaque<Aligned>>(), align_of::<Aligned>());
    assert_eq!(size_of::<Opaque<()>>(), 0);
    let shell = Opaque::<Aligned>::uninit();
    assert_eq!(Opaque::cast_into(&raw const shell), shell.get());
    assert!(shell.get().is_aligned());
    const NULL: *mut u32 = Opaque::<u32>::cast_into(ptr::null());
    assert!(NULL.is_null());
    const INITIALIZED: Opaque<u32> = Opaque::new(17);
    let initialized = INITIALIZED;
    // SAFETY: Initialized by the constant constructor and not modified.
    assert_eq!(unsafe { initialized.get().read() }, 17);
    fn assert_send<T: Send>() {}
    assert_send::<Opaque<u32>>();
}

#[must_use]
struct Payload<'a> {
    address: *const u32,
    value: u32,
    drops: &'a Cell<usize>,
}
impl Drop for Payload<'_> {
    fn drop(&mut self) {
        assert_eq!(self.address, &raw const self.value);
        self.drops.set(self.drops.get() + 1);
    }
}

// Invariant: raw contains exactly one initialized Payload for this owner's
// entire lifetime. Its address is stable and only the owner's Drop destroys it.
#[pin_data(PinnedDrop)]
#[must_use]
struct Owner<'a> {
    #[pin]
    raw: Opaque<Payload<'a>>,
}
#[pinned_drop]
impl PinnedDrop for Owner<'_> {
    fn drop(self: Pin<&mut Self>) {
        // SAFETY: The owner invariant guarantees a live payload. PinnedDrop runs
        // once, before releasing its stable storage; Opaque won't drop it again.
        unsafe {
            ptr::drop_in_place(self.raw.get());
        }
    }
}
impl<'a> Owner<'a> {
    fn initializer(drops: &'a Cell<usize>) -> impl PinInit<Self> + 'a {
        // SAFETY: Establish every field and the payload invariant before success.
        // These raw writes cannot error or unwind. PinInit keeps the whole owner
        // pinned until PinnedDrop destroys its payload. No pointer is published.
        unsafe {
            pin_init::pin_init_from_closure(move |slot: *mut Self| {
                slot.write(Self {
                    raw: Opaque::uninit(),
                });
                let payload = Opaque::cast_into(&raw const (*slot).raw);
                payload.write(Payload {
                    address: ptr::null(),
                    value: 42,
                    drops,
                });
                (&raw mut (*payload).address).write(&raw const (*payload).value);
                Ok(())
            })
        }
    }
    fn read(self: Pin<&Self>) -> u32 {
        // SAFETY: Initialized by the owner invariant, still pinned and live;
        // no mutation or references into the payload overlap this raw read.
        unsafe { (*self.raw.get()).address.read() }
    }
}

#[test]
fn outer_pinned_owner_initializes_and_drops_payload_once() {
    let drops = Cell::new(0);
    let value = Box::pin_init(Owner::initializer(&drops)).unwrap();
    let moved = std::hint::black_box(value);
    assert_eq!(moved.as_ref().read(), 42);
    let (_, counts) = observe(false, || drop(moved));
    assert_eq!(drops.get(), 1);
    assert_eq!(counts.deallocations, 1);
}
