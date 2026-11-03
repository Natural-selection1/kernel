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

extern crate alloc;
use alloc::alloc::{handle_alloc_error, AllocError, Global};
use core::{
    alloc::{Allocator, Layout},
    ptr::NonNull,
};

/// Raw storage whose bytes may be uninitialized.
///
/// Owned storage is released exactly once on drop. External storage is never
/// released here; its validity, lifetime and synchronization remain the caller's
/// responsibility. Zero-sized owned storage has a non-null aligned pointer,
/// unlike the empty state returned by `new` and `default`.
///
/// Allocation does not initialize the bytes. Slice access is unsafe because
/// callers must establish initialization, validity and the reference's access
/// permissions, including for externally managed memory.
#[derive(Debug)]
#[must_use = "dropping storage releases its owned allocation"]
pub struct Storage {
    kind: StorageKind,
}

#[derive(Debug)]
enum StorageKind {
    Empty,
    Owned { base: NonNull<u8>, layout: Layout },
    External { base: *mut u8, size: usize },
}

impl Default for Storage {
    fn default() -> Self {
        Self::new()
    }
}

impl Storage {
    /// Allocates uninitialized storage using the program's global allocator.
    ///
    /// Returns [`AllocError`] on allocation failure, without creating an owning
    /// `Storage`. Success provides a non-null pointer aligned to `layout.align()`.
    /// Zero-sized layouts use an aligned dangling pointer and do not allocate or
    /// deallocate any heap memory.
    ///
    /// The bytes are not initialized. Initialize them through the raw pointer
    /// before exposing them as initialized values or byte slices.
    #[inline]
    pub fn try_from_layout(layout: Layout) -> Result<Self, AllocError> {
        let base = Global.allocate(layout)?.cast::<u8>();
        Ok(Self {
            kind: StorageKind::Owned { base, layout },
        })
    }

    /// Allocates uninitialized storage, invoking the allocation error handler
    /// if allocation fails. Use [`Self::try_from_layout`] to recover from failure.
    #[inline]
    pub fn from_layout(layout: Layout) -> Self {
        Self::try_from_layout(layout).unwrap_or_else(|_| handle_alloc_error(layout))
    }

    /// Records externally managed storage without taking responsibility for freeing it.
    ///
    /// # Safety
    /// For a nonempty range, `base` must point to `size` accessible bytes within
    /// one allocation, with `size <= isize::MAX`. The caller must keep that memory
    /// alive for all uses and ensure initialization and synchronization suitable
    /// for each access. This type does not encode the external memory's lifetime.
    /// A zero-length range preserves `base` as supplied, including a null pointer.
    #[inline]
    pub unsafe fn from_raw(base: *mut u8, size: usize) -> Self {
        Self {
            kind: StorageKind::External { base, size },
        }
    }

    #[inline]
    pub const fn new() -> Self {
        Self {
            kind: StorageKind::Empty,
        }
    }

    #[inline]
    pub fn base(&self) -> *mut u8 {
        match &self.kind {
            StorageKind::Empty => core::ptr::null_mut(),
            StorageKind::Owned { base, .. } => base.as_ptr(),
            StorageKind::External { base, .. } => *base,
        }
    }

    #[inline]
    pub fn size(&self) -> usize {
        match &self.kind {
            StorageKind::Empty => 0,
            StorageKind::Owned { layout, .. } => layout.size(),
            StorageKind::External { size, .. } => *size,
        }
    }

    /// Views initialized bytes without taking ownership of external memory.
    ///
    /// # Safety
    /// For a nonempty view, the range must be valid for reads within one live
    /// allocation, contain only initialized bytes and have size <= isize::MAX.
    /// No writes may occur through other pointers for the returned reference's
    /// lifetime. External memory must remain alive for that entire lifetime.
    /// An empty view has no backing-memory requirements.
    #[inline]
    pub unsafe fn as_slice(&self) -> &[u8] {
        if self.size() == 0 {
            return &[];
        }
        unsafe { core::slice::from_raw_parts(self.base(), self.size()) }
    }

    /// Mutably views initialized bytes; use raw writes to initialize storage first.
    ///
    /// # Safety
    /// For a nonempty view, the range must be valid for reads and writes within
    /// one live allocation, contain only initialized bytes and have size <=
    /// isize::MAX. The returned reference must have exclusive access to the
    /// bytes for its lifetime, and external memory must remain alive throughout.
    /// An empty view has no backing-memory requirements.
    #[inline]
    pub unsafe fn as_mut_slice(&mut self) -> &mut [u8] {
        if self.size() == 0 {
            return &mut [];
        }
        unsafe { core::slice::from_raw_parts_mut(self.base(), self.size()) }
    }
}

impl Drop for Storage {
    #[inline]
    fn drop(&mut self) {
        if let StorageKind::Owned { base, layout } = &self.kind {
            // SAFETY: Only Global::allocate constructs Owned, and neither its
            // pointer nor layout can be replaced by callers. Global handles
            // zero-sized dangling pointers without calling the underlying heap.
            unsafe { Global.deallocate(*base, *layout) };
        }
    }
}

// Retained for compatibility. External-memory lifetime and cross-thread access
// contracts, including safe wrappers around from_raw, need a separate audit.
unsafe impl Send for Storage {}
unsafe impl Sync for Storage {}
