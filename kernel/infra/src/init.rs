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

//! Fallible in-place initialization using the program's global allocator.
//!
//! Import this module's [`InPlaceInit`], not the upstream trait of the same name.
//! Macros come from `pin_init`; GN users of those macros must depend explicitly
//! on `//external/pin-init:pin_init` because expansion refers to `::pin_init`.
//! Even `init` and `pin_init` can fail to allocate: only their initializers are
//! infallible. `try_*` additionally preserves the initializer's own error.
//!
//! ```
//! # #![feature(allocator_api)]
//! # extern crate alloc;
//! use alloc::boxed::Box;
//! use blueos_infra::init::InPlaceInit;
//! use pin_init::init;
//! struct Value { number: u32 }
//! let value = Box::init(init!(Value { number: 42 }))?;
//! # Ok::<(), alloc::alloc::AllocError>(())
//! ```
//!
//! [`InPlaceWrite`] consumes an existing `Box<MaybeUninit<T>>`: success transfers
//! ownership to `Box<T>` (or `Pin<Box<T>>`), and failure releases the storage.
//! Initializers must clean up completed fields on error or unwind; the macros
//! provide that rollback. The uninitialized complete `T` is never dropped.
//!
//! [`crate::arc::UniqueArc`] also implements this trait and `InPlaceWrite` for
//! its uninitialized form. Its pinned result is `Pin<UniqueArc<T>>`.
//! [`crate::arc::Arc`] delegates initialization to that exclusive owner and
//! returns `Arc<T>` directly: shared ownership intrinsically pins its value.
//!
//! ```
//! # #![feature(allocator_api)]
//! # extern crate alloc;
//! use blueos_infra::{arc::{Arc, UniqueArc}, init::{InPlaceInit, InPlaceWrite}};
//! let storage = UniqueArc::<u32>::try_new_uninit()?;
//! let unique = storage.write_init(42u32).unwrap();
//! let shared: Arc<u32> = unique.into();
//! let directly_shared = Arc::pin_init(7u32)?;
//! # Ok::<(), alloc::alloc::AllocError>(())
//! ```

extern crate alloc;
use alloc::{alloc::AllocError, boxed::Box};
use core::pin::Pin;
pub use pin_init::{InPlaceWrite, Init, PinInit};

/// An owner that can allocate and initialize its value in place.
///
/// `PinnedSelf` allows future owners with an intrinsic pinning guarantee to
/// return themselves. Implementations cover standard `Box<T>`,
/// [`crate::arc::UniqueArc<T>`] and [`crate::arc::Arc<T>`].
pub trait InPlaceInit<T>: Sized {
    /// The owner returned after fixed-address initialization.
    type PinnedSelf;
    /// Allocate and initialize; only allocation can fail.
    fn init(init: impl Init<T>) -> Result<Self, AllocError>;
    /// Allocate and pin-initialize; only allocation can fail.
    fn pin_init(init: impl PinInit<T>) -> Result<Self::PinnedSelf, AllocError>;
    /// Allocate and initialize, converting allocation failure into `E`.
    fn try_init<E: From<AllocError>>(init: impl Init<T, E>) -> Result<Self, E>;
    /// Allocate and pin-initialize, converting allocation failure into `E`.
    fn try_pin_init<E: From<AllocError>>(init: impl PinInit<T, E>) -> Result<Self::PinnedSelf, E>;
}

impl<T> InPlaceInit<T> for Box<T> {
    type PinnedSelf = Pin<Box<T>>;
    #[inline]
    fn init(init: impl Init<T>) -> Result<Self, AllocError> {
        <Self as pin_init::InPlaceInit<T>>::init(init)
    }
    #[inline]
    fn pin_init(init: impl PinInit<T>) -> Result<Self::PinnedSelf, AllocError> {
        <Self as pin_init::InPlaceInit<T>>::pin_init(init)
    }
    #[inline]
    fn try_init<E: From<AllocError>>(init: impl Init<T, E>) -> Result<Self, E> {
        <Self as pin_init::InPlaceInit<T>>::try_init(init)
    }
    #[inline]
    fn try_pin_init<E: From<AllocError>>(init: impl PinInit<T, E>) -> Result<Self::PinnedSelf, E> {
        <Self as pin_init::InPlaceInit<T>>::try_pin_init(init)
    }
}
