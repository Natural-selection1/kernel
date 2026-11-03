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

//! Compile the production Stack with only its Context alignment substituted.

use allocation_test_support::{observe, Counts, TestAllocator};

#[global_allocator]
static ALLOCATOR: TestAllocator = TestAllocator;

mod arch {
    #[repr(align(64))]
    pub struct Context;
}

mod stack;
use stack::Stack;

#[test]
fn invalid_sizes_do_not_allocate() {
    for size in [0, usize::MAX] {
        let (stack, counts) = observe(true, || Stack::from_size(size));
        assert!(stack.is_none());
        assert_eq!(counts, Counts::default());
    }
}

#[test]
fn allocation_failure_returns_none_without_freeing() {
    let (stack, counts) = observe(true, || Stack::from_size(2048));
    assert!(stack.is_none());
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
fn successful_stack_has_requested_layout_and_is_freed_once() {
    let (result, counts) = observe(false, || {
        Stack::from_size(1600).map(|stack| {
            let result = (stack.base() as usize, stack.top() as usize, stack.size());
            drop(stack);
            result
        })
    });
    let (base, top, size) = result.unwrap();
    assert_ne!(base, 0);
    assert_eq!(base % core::mem::align_of::<arch::Context>(), 0);
    assert_eq!(size, 1600);
    assert_eq!(top - base, size);
    assert_eq!(
        counts,
        Counts {
            attempts: 1,
            allocations: 1,
            deallocations: 1
        }
    );
}

#[test]
fn empty_stack_does_not_allocate_or_free() {
    let (result, counts) = observe(true, || {
        let stack = Stack::new();
        let result = (stack.base().is_null(), stack.size());
        drop(stack);
        result
    });
    assert_eq!(result, (true, 0));
    assert_eq!(counts, Counts::default());
}
