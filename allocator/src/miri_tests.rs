// Copyright (c) 2025 vivo Mobile Communication Co., Ltd.
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

//! Miri-runnable unit tests for the BlueOS allocators.
//!
//! These tests are exercised by `cargo miri test` (see
//! `kernel/allocator/.cargo/config.toml`). They use only `core` + `alloc` so
//! they also build under plain `cargo test`. All randomness comes from a
//! hard-coded XorShift32 seed so failures are deterministic and reproducible.

extern crate alloc;

use core::alloc::Layout;
use core::ptr::NonNull;

use alloc::vec::Vec;

use crate::llff::Heap as LlffHeap;
use crate::slab::Slab;
use crate::tlsf::Tlsf;

/// Deterministic PRNG (XorShift32) so tests are reproducible without `rand`.
struct XorShift32(u32);

impl XorShift32 {
    const fn new(seed: u32) -> Self {
        Self(seed)
    }

    fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// Uniform value in `[lo, hi]` (inclusive). Requires `lo <= hi`.
    fn range(&mut self, lo: usize, hi: usize) -> usize {
        let span = (hi - lo + 1) as u32;
        lo + (self.next() % span) as usize
    }

    fn chance(&mut self, percent: u32) -> bool {
        self.next() % 100 < percent
    }
}

/// Builds a `Layout`, rounding `size` up so `size % align == 0` (a
/// `Layout` invariant) and keeping it non-zero.
fn make_layout(size: usize, align: usize) -> Layout {
    let align = align.max(1);
    let size = size.max(1).next_multiple_of(align);
    Layout::from_size_align(size, align).unwrap()
}

/// A tracked live allocation: pointer plus the exact `Layout` it was
/// allocated with, so deallocation can pass matching metadata.
struct Tracked {
    ptr: NonNull<u8>,
    layout: Layout,
}

// ---------------------------------------------------------------------------
// TLSF
// ---------------------------------------------------------------------------

/// Basic TLSF smoke test: insert one arena, allocate two blocks, free both,
/// and assert the allocator returns to zero allocated bytes.
#[test]
fn tlsf_smoke() {
    let mut tlsf: Tlsf<'static, u16, u16, 9, 16> = Tlsf::new();

    #[repr(align(64))]
    struct Arena([u8; 4096]);
    let mut arena = Arena([0; 4096]);
    let arena_ptr = NonNull::new(arena.0.as_mut_ptr()).unwrap();
    let block: NonNull<[u8]> = NonNull::slice_from_raw_parts(arena_ptr, 4096);

    let inserted = unsafe { tlsf.insert_free_block_ptr(block) };
    assert!(inserted.is_some(), "insert_free_block_ptr failed");
    assert!(tlsf.total() > 0);
    assert_eq!(tlsf.allocated(), 0);

    let p1 = tlsf.allocate(&make_layout(64, 8)).expect("alloc 64");
    let p2 = tlsf.allocate(&make_layout(128, 8)).expect("alloc 128");
    assert!(tlsf.allocated() > 0);

    unsafe {
        tlsf.deallocate(p1, 8);
        tlsf.deallocate(p2, 8);
    }
    assert_eq!(tlsf.allocated(), 0, "all allocations freed");
}

/// Deterministic randomized alloc/dealloc stress test. 500 steps with a
/// 60/40 alloc/free split, sizes 8..=1024, alignments {1,2,4,8,16,32}. Every
/// live allocation is tracked with its layout; at the end everything is
/// drained and we assert `allocated() == 0` (no leaks, no corruption of the
/// allocator's internal accounting).
#[test]
fn tlsf_random_alloc_dealloc() {
    let mut rng = XorShift32::new(0xC0FF_EE11);
    let mut tlsf: Tlsf<'static, u32, u32, 9, 16> = Tlsf::new();

    #[repr(align(64))]
    struct Arena([u8; 65536]);
    let mut arena = Arena([0; 65536]);
    let arena_ptr = NonNull::new(arena.0.as_mut_ptr()).unwrap();
    let block: NonNull<[u8]> = NonNull::slice_from_raw_parts(arena_ptr, 65536);
    unsafe { tlsf.insert_free_block_ptr(block) }.expect("insert arena");

    const ALIGNS: [usize; 6] = [1, 2, 4, 8, 16, 32];
    let mut live: Vec<Tracked> = Vec::new();

    for _ in 0..500 {
        // 60% allocate (or always allocate when empty), else free.
        if live.is_empty() || rng.chance(60) {
            let size = rng.range(8, 1024);
            let align = ALIGNS[rng.range(0, ALIGNS.len() - 1)];
            let layout = make_layout(size, align);
            if let Some(ptr) = tlsf.allocate(&layout) {
                // The returned pointer must satisfy the requested alignment.
                assert_eq!(ptr.as_ptr() as usize % layout.align(), 0);
                live.push(Tracked { ptr, layout });
            }
            // Allocation failure is acceptable under fragmentation; skip.
        } else {
            let idx = rng.range(0, live.len() - 1);
            let Tracked { ptr, layout } = live.swap_remove(idx);
            unsafe { tlsf.deallocate(ptr, layout.align()) };
        }
    }

    // Drain everything and verify accounting returns to zero.
    for Tracked { ptr, layout } in live.drain(..) {
        unsafe { tlsf.deallocate(ptr, layout.align()) };
    }
    assert_eq!(tlsf.allocated(), 0, "leak detected after drain");
}

/// Repeatedly allocate many small blocks then free them all, checking that
/// freed blocks coalesce (the free pool recovers to near its original size).
#[test]
fn tlsf_split_and_coalesce() {
    let mut tlsf: Tlsf<'static, u32, u32, 9, 16> = Tlsf::new();

    #[repr(align(64))]
    struct Arena([u8; 16384]);
    let mut arena = Arena([0; 16384]);
    let arena_ptr = NonNull::new(arena.0.as_mut_ptr()).unwrap();
    let block: NonNull<[u8]> = NonNull::slice_from_raw_parts(arena_ptr, 16384);
    unsafe { tlsf.insert_free_block_ptr(block) }.expect("insert arena");

    let free_at_start = tlsf.free();
    let layout = make_layout(32, 8);

    // Allocate as many small blocks as the arena can hold.
    let mut ptrs: Vec<NonNull<u8>> = Vec::new();
    while let Some(p) = tlsf.allocate(&layout) {
        ptrs.push(p);
    }
    assert!(!ptrs.is_empty(), "expected at least one allocation");
    assert!(tlsf.free() < free_at_start, "free should shrink after allocs");

    // Free them all; coalescing should recover essentially all free space.
    for p in ptrs.drain(..) {
        unsafe { tlsf.deallocate(p, layout.align()) };
    }
    assert_eq!(tlsf.allocated(), 0);
    assert_eq!(
        tlsf.free(),
        free_at_start,
        "coalescing failed to recover free space"
    );
}

// ---------------------------------------------------------------------------
// LLFF (linked-list first-fit)
// ---------------------------------------------------------------------------

/// LLFF smoke test: build a heap over a raw arena, verify the initial hole
/// list via the test accessor, then allocate/free and check accounting.
#[test]
fn llff_hole_list_smoke() {
    let mut heap = LlffHeap::empty();

    #[repr(align(64))]
    struct Arena([u8; 4096]);
    let mut arena = Arena([0; 4096]);
    let arena_base = arena.0.as_mut_ptr() as usize;
    unsafe { heap.init(arena_base, 4096) };

    // The freshly initialized heap must expose one contiguous hole.
    let first = heap
        .holes_first_hole_for_test()
        .expect("expected an initial hole");
    assert!(first.1 >= crate::block::GRANULARITY);
    assert_eq!(heap.allocated(), 0);
    assert!(heap.total() > 0);

    let l1 = make_layout(64, 8);
    let p1 = heap.allocate_first_fit(&l1).expect("llff alloc 64");
    assert!(heap.allocated() > 0);

    let l2 = make_layout(128, 16);
    let p2 = heap.allocate_first_fit(&l2).expect("llff alloc 128");

    unsafe {
        heap.deallocate(p1, &l1);
        heap.deallocate(p2, &l2);
    }
    assert_eq!(heap.allocated(), 0, "llff leak after free");
}

/// LLFF randomized alloc/dealloc stress test, mirroring the TLSF one but
/// through the `Heap` API (which takes `&Layout` on deallocate).
#[test]
fn llff_random_alloc_dealloc() {
    let mut rng = XorShift32::new(0xDEAD_BEEF);
    let mut heap = LlffHeap::empty();

    #[repr(align(64))]
    struct Arena([u8; 65536]);
    let mut arena = Arena([0; 65536]);
    let arena_base = arena.0.as_mut_ptr() as usize;
    unsafe { heap.init(arena_base, 65536) };

    const ALIGNS: [usize; 5] = [8, 16, 32, 64, 128];
    let mut live: Vec<Tracked> = Vec::new();

    for _ in 0..400 {
        if live.is_empty() || rng.chance(60) {
            let size = rng.range(8, 512);
            let align = ALIGNS[rng.range(0, ALIGNS.len() - 1)];
            let layout = make_layout(size, align);
            if let Some(ptr) = heap.allocate_first_fit(&layout) {
                assert_eq!(ptr.as_ptr() as usize % layout.align(), 0);
                live.push(Tracked { ptr, layout });
            }
        } else {
            let idx = rng.range(0, live.len() - 1);
            let Tracked { ptr, layout } = live.swap_remove(idx);
            unsafe { heap.deallocate(ptr, &layout) };
        }
    }

    for Tracked { ptr, layout } in live.drain(..) {
        unsafe { heap.deallocate(ptr, &layout) };
    }
    assert_eq!(heap.allocated(), 0, "llff leak detected after drain");
}

// ---------------------------------------------------------------------------
// Slab (fixed-size block allocator)
// ---------------------------------------------------------------------------

/// Slab smoke test over a single fixed block size. `Slab` ignores `Layout`
/// on allocate (it always hands out `block_size` blocks), so we exercise the
/// raw block lifecycle: init an arena of N blocks, allocate all N, confirm
/// exhaustion returns `None`, then free everything and confirm double-free
/// detection doesn't fire.
#[test]
fn slab_smoke() {
    const BLOCK_SIZE: usize = 64;
    const COUNT: usize = 16;

    let mut slab = Slab::new();

    // Blocks must be at least 2 * size_of::<usize>() (16 bytes) because the
    // allocator writes a magic word at ptr+size_of::<usize>() for double-free
    // detection. We use 64 and align the arena to it.
    #[repr(align(64))]
    struct Arena([u8; BLOCK_SIZE * COUNT]);
    let mut arena = Arena([0; BLOCK_SIZE * COUNT]);
    let base = arena.0.as_mut_ptr() as usize;

    unsafe { slab.init(base, COUNT, BLOCK_SIZE) };

    let layout = make_layout(BLOCK_SIZE, 8);
    let mut ptrs: Vec<NonNull<u8>> = Vec::new();

    // Allocate all COUNT blocks; the (COUNT+1)-th must fail.
    for _ in 0..COUNT {
        let p = slab.allocate(&layout).expect("slab allocate");
        // Slab writes a magic word at ptr+8 on alloc/free; memory must be writable.
        unsafe { core::ptr::write_bytes(p.as_ptr(), 0xAB, BLOCK_SIZE) };
        ptrs.push(p);
    }
    assert!(
        slab.allocate(&layout).is_none(),
        "slab should be exhausted after COUNT allocations"
    );

    for p in ptrs.drain(..) {
        unsafe { slab.deallocate(p) };
    }

    // After freeing all blocks the slab is reusable again.
    let p = slab.allocate(&layout).expect("slab reusable after frees");
    unsafe { slab.deallocate(p) };
}
