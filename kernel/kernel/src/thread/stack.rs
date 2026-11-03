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

use crate::arch::Context;
use blueos_infra::storage::Storage;
use core::alloc::Layout;

#[derive(Debug)]
#[must_use = "dropping a stack releases its owned storage"]
pub struct Stack(Storage);

impl Stack {
    #[inline]
    pub fn top(&self) -> *mut u8 {
        unsafe { self.0.base().add(self.size()) }
    }

    #[inline]
    pub fn base(&self) -> *mut u8 {
        self.0.base()
    }

    #[inline]
    pub fn from_size(size: usize) -> Option<Self> {
        if size == 0 {
            return None;
        }
        let layout = Layout::from_size_align(size, core::mem::align_of::<Context>()).ok()?;
        let storage = Storage::try_from_layout(layout).ok()?;
        Some(Self(storage))
    }

    pub const fn new() -> Self {
        Self(Storage::new())
    }

    #[inline]
    pub fn from_raw(base: *mut u8, size: usize) -> Option<Self> {
        const ALIGN: usize = core::mem::align_of::<Context>();
        if size < ALIGN {
            return None;
        }
        if base.is_null() {
            return None;
        }
        if !(base as usize).is_multiple_of(ALIGN) {
            return None;
        }
        Some(Self(unsafe { Storage::from_raw(base, size) }))
    }

    pub fn size(&self) -> usize {
        self.0.size()
    }
}
