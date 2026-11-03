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

//! Thread-local allocation observation for standalone host and Miri tests.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    marker::PhantomData,
    ptr,
    rc::Rc,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub attempts: usize,
    pub allocations: usize,
    pub deallocations: usize,
}

#[derive(Clone, Copy)]
struct Observation {
    counts: Counts,
    fail_next: bool,
}

thread_local! {
    // Const, destructor-free TLS: accessing it must not allocate recursively.
    // Each test observes only its own thread, even with parallel test execution.
    static OBSERVATION: Cell<Option<Observation>> = const { Cell::new(None) };
}

fn update_observation(f: impl FnOnce(&mut Observation)) {
    let _ = OBSERVATION.try_with(|slot| {
        if let Some(mut observation) = slot.get() {
            f(&mut observation);
            slot.set(Some(observation));
        }
    });
}

/// System allocator with thread-local failure injection and allocation counts.
/// Register it explicitly as the global allocator of a standalone test binary.
pub struct TestAllocator;

impl TestAllocator {
    fn should_fail() -> bool {
        let mut fail = false;
        update_observation(|observation| {
            observation.counts.attempts += 1;
            fail = observation.fail_next;
            observation.fail_next = false;
        });
        fail
    }

    fn allocated(ptr: *mut u8) -> *mut u8 {
        if !ptr.is_null() {
            update_observation(|observation| observation.counts.allocations += 1);
        }
        ptr
    }
}

// SAFETY: Successful allocations and all deallocations are delegated to System
// with their original layout. Injected failures return null without allocating.
unsafe impl GlobalAlloc for TestAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if Self::should_fail() {
            ptr::null_mut()
        } else {
            Self::allocated(unsafe { System.alloc(layout) })
        }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if Self::should_fail() {
            ptr::null_mut()
        } else {
            Self::allocated(unsafe { System.alloc_zeroed(layout) })
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        update_observation(|observation| observation.counts.deallocations += 1);
        unsafe { System.dealloc(ptr, layout) };
    }
}

/// Restores the thread's allocator state, including if the observed code panics.
#[must_use]
struct ObservationGuard {
    // The thread-local observation must be ended on the thread that started it.
    _not_send: PhantomData<Rc<()>>,
}

impl ObservationGuard {
    fn start(fail_next: bool) -> Self {
        OBSERVATION.with(|slot| {
            assert!(slot.get().is_none(), "nested allocation observations");
            slot.set(Some(Observation {
                counts: Counts::default(),
                fail_next,
            }));
        });
        Self {
            _not_send: PhantomData,
        }
    }

    fn finish(self) -> Counts {
        OBSERVATION.with(|slot| slot.take().unwrap().counts)
    }
}

impl Drop for ObservationGuard {
    fn drop(&mut self) {
        OBSERVATION.with(|slot| slot.set(None));
    }
}

/// Observes allocations on the calling thread, optionally failing the next one.
///
/// Keep assertions and formatting outside the observation. In particular, an
/// allocation by the test harness must never consume an injected failure.
/// The test binary must register [`TestAllocator`] as its global allocator.
pub fn observe<R>(fail_next: bool, f: impl FnOnce() -> R) -> (R, Counts) {
    let guard = ObservationGuard::start(fail_next);
    let result = f();
    (result, guard.finish())
}
