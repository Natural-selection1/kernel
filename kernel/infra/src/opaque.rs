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

//! Storage for a value whose validity and lifetime are managed externally.
//!
//! This primitive is useful beyond C interoperability, but is not an ordinary
//! Rust owner. Prefer a normal field (or `UnsafeCell<T>` for interior mutability)
//! when a valid `T` should always be present and destroyed automatically.

use core::{cell::UnsafeCell, marker::PhantomPinned, mem::MaybeUninit};

/// A possibly uninitialized value behind an interior-mutability boundary.
///
/// This type neither allocates nor tracks initialization, and **never drops
/// the inner `T`**, even when constructed with [`Self::new`]. The surrounding
/// owner must arrange any required initialization, synchronization and cleanup.
/// Taking a raw pointer does not establish validity, permission to access, or
/// a lifetime for the pointee. Moving an unpinned wrapper can move its storage.
///
/// `Opaque<T>` is `!Unpin` and `!Sync`; `Send` follows the fields and requires
/// `T: Send`. Pinning this wrapper does not expose a pinned reference to `T`.
/// There is deliberately no generic `Wrapper<T>` implementation: suppressing
/// `T::drop` would violate the destruction guarantee of arbitrary `PinInit<T>`.
/// An outer type can use `PinInit` and its own destructor to establish a stricter
/// lifecycle, without exposing the payload as a safe reference.
///
/// Transparent representation preserves the direct size and alignment of `T`,
/// not its validity requirements or layout optimizations in enclosing types
/// such as `Option<Opaque<T>>`.
#[repr(transparent)]
pub struct Opaque<T> {
    value: UnsafeCell<MaybeUninit<T>>,
    _pin: PhantomPinned,
}

impl<T> Opaque<T> {
    /// Store an initialized value, transferring responsibility for its cleanup
    /// to the caller. Dropping the wrapper will not run `T::drop`.
    pub const fn new(value: T) -> Self {
        Self {
            value: UnsafeCell::new(MaybeUninit::new(value)),
            _pin: PhantomPinned,
        }
    }

    /// Create an uninitialized storage shell. The shell itself is a valid value.
    pub const fn uninit() -> Self {
        Self {
            value: UnsafeCell::new(MaybeUninit::uninit()),
            _pin: PhantomPinned,
        }
    }

    /// Obtain the payload address without asserting that a valid `T` is present.
    /// Dereferencing it requires the caller to establish initialization, bounds,
    /// lifetime and the access rules of any live references or concurrent users.
    pub const fn get(&self) -> *mut T {
        self.value.get().cast()
    }

    /// Convert a wrapper pointer to a payload pointer without dereferencing it.
    /// This preserves its address and provenance, including for null pointers;
    /// it grants no permission to read or write through the returned pointer.
    pub const fn cast_into(this: *const Self) -> *mut T {
        this.cast_mut().cast()
    }
}
