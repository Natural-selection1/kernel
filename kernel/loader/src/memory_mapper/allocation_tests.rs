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

use super::*;
use allocation_test_support::{observe, Counts};

const START: usize = 0x1000;
const SIZE: usize = 64;

fn mapper(size: usize) -> MemoryMapper {
    let mut mapper = MemoryMapper::new(None);
    mapper.update_start(START).update_end(START + size);
    mapper
}

fn allocated_mapper() -> MemoryMapper {
    let mut mapper = mapper(SIZE);
    assert_eq!(mapper.allocate_memory(), Ok(SIZE));
    assert_eq!(mapper.write_slice_at(START, &[0x5a; SIZE]), Ok(SIZE));
    mapper
}

fn assert_freed_once(mapper: MemoryMapper) {
    let (_, counts) = observe(false, || drop(mapper));
    assert_eq!(
        counts,
        Counts {
            attempts: 0,
            allocations: 0,
            deallocations: 1
        }
    );
}

#[test]
fn initial_failure_keeps_mapper_unallocated() {
    let mut mapper = mapper(SIZE);
    let (result, counts) = observe(true, || {
        let result = (
            mapper.allocate_memory(),
            mapper.real_start(),
            mapper.mem.size(),
        );
        drop(mapper);
        result
    });
    assert_eq!(
        result,
        (
            Err("Memory allocation failed"),
            Err("Memory not allocated yet"),
            0
        )
    );
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
fn replacement_failure_preserves_allocation_and_contents() {
    let mut mapper = allocated_mapper();
    let old_base = mapper.mem.base();
    mapper.update_end(START + SIZE * 2);
    let (result, counts) = observe(true, || mapper.allocate_memory());
    assert_eq!(result, Err("Memory allocation failed"));
    assert_eq!(
        counts,
        Counts {
            attempts: 1,
            allocations: 0,
            deallocations: 0
        }
    );
    assert_eq!(mapper.mem.base(), old_base);
    assert_eq!(mapper.mem.size(), SIZE);
    // Range metadata belongs to the caller and is not rolled back.
    assert_eq!(mapper.total_size(), Ok(SIZE * 2));
    for offset in 0..SIZE {
        // SAFETY: All bytes of the still-live old allocation were initialized.
        assert_eq!(unsafe { mapper.mem.base().add(offset).read() }, 0x5a);
    }
    assert_freed_once(mapper);
}

#[test]
fn successful_replacement_frees_old_then_new_storage_once() {
    let mut mapper = allocated_mapper();
    mapper.update_end(START + SIZE * 2);
    let (result, counts) = observe(false, || mapper.allocate_memory());
    assert_eq!(result, Ok(SIZE * 2));
    assert_eq!(mapper.mem.size(), SIZE * 2);
    assert!(!mapper.mem.base().is_null());
    assert_eq!(
        counts,
        Counts {
            attempts: 1,
            allocations: 1,
            deallocations: 1
        }
    );
    // Successful replacement is uninitialized; do not read its bytes.
    assert_eq!(
        mapper.write_slice_at(START, &[0x33; SIZE * 2]),
        Ok(SIZE * 2)
    );
    assert_freed_once(mapper);
}

#[test]
fn initial_success_has_expected_alignment_and_writable_storage() {
    let mut mapper = mapper(SIZE);
    let (result, counts) = observe(false, || mapper.allocate_memory());
    assert_eq!(result, Ok(SIZE));
    assert_eq!(
        counts,
        Counts {
            attempts: 1,
            allocations: 1,
            deallocations: 0
        }
    );
    let align = if cfg!(target_arch = "aarch64") {
        4096
    } else {
        2 * core::mem::size_of::<usize>()
    };
    assert_ne!(mapper.mem.base() as usize, 0);
    assert_eq!(mapper.mem.base() as usize % align, 0);
    assert_eq!(mapper.write_slice_at(START, &[0x7b; SIZE]), Ok(SIZE));
    // SAFETY: write_slice_at initialized this byte within the owned allocation.
    assert_eq!(unsafe { mapper.mem.base().add(SIZE - 1).read() }, 0x7b);
    assert_freed_once(mapper);
}

#[test]
fn invalid_range_does_not_allocate() {
    let mut mapper = MemoryMapper::new(None);
    let (result, counts) = observe(true, || mapper.allocate_memory());
    assert_eq!(result, Err("Illegal memory size"));
    assert_eq!(counts, Counts::default());
    assert_eq!(mapper.real_start(), Err("Memory not allocated yet"));
}

#[test]
fn invalid_layout_preserves_old_allocation() {
    let mut mapper = allocated_mapper();
    let old_base = mapper.mem.base();
    mapper.update_start(0).update_end(usize::MAX);
    let (result, counts) = observe(true, || mapper.allocate_memory());
    assert_eq!(result, Err("Illegal memory layout"));
    assert_eq!(counts, Counts::default());
    assert_eq!(mapper.mem.base(), old_base);
    assert_eq!(mapper.mem.size(), SIZE);
    assert_freed_once(mapper);
}

#[test]
fn zero_size_does_not_allocate_or_free() {
    let mut mapper = mapper(0);
    let (result, counts) = observe(true, || {
        let result = (
            mapper.allocate_memory(),
            mapper.mem.base().is_null(),
            mapper.mem.size(),
        );
        drop(mapper);
        result
    });
    assert_eq!(result, (Ok(0), false, 0));
    assert_eq!(counts, Counts::default());
}
