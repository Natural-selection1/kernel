// Copyright (c) 2026 vivo Mobile Communication Co., Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Safe-API contract probes, intentionally exposing main's current violations.
//! Run only under Miri: running these native could execute undefined behavior.
//! These are normal tests, not expected-panic tests, and must turn green after fixes.
use crate::{ringbuffer::BoxedRingBuffer, storage::Storage, tinyarc::TinyArc};
use core::{alloc::Layout, cell::Cell};
use std::sync::{Arc, Barrier};

#[test]
fn storage_empty_shared_slice() {
    assert!(Storage::new().as_slice().is_empty());
}

#[test]
fn storage_empty_mutable_slice() {
    assert!(Storage::new().as_mut_slice().is_empty());
}

#[test]
fn storage_zero_size_allocation() {
    let storage = Storage::from_layout(Layout::from_size_align(0, 64).unwrap());
    assert_eq!(storage.size(), 0);
}

#[test]
fn storage_uninitialized_bytes_exposed_by_safe_api() {
    let storage = Storage::from_layout(Layout::new::<u64>());
    std::hint::black_box(storage.as_slice()[0]);
}

#[test]
fn storage_safe_public_variant_can_own_null() {
    drop(Storage::Alloc(core::ptr::null_mut(), Layout::new::<u64>()));
}

#[test]
fn storage_initialized_owned_bytes() {
    let mut storage = Storage::from_layout(Layout::new::<u64>());
    // SAFETY: this nonzero allocation is live, aligned and uniquely owned.
    unsafe { storage.base().write_bytes(0x42, storage.size()) };
    assert_eq!(storage.as_slice(), &[0x42; 8]);
    storage.as_mut_slice()[0] = 0x11;
    assert_eq!(storage.as_slice()[0], 0x11);
}

#[test]
fn storage_borrowed_initialized_bytes() {
    let mut bytes = [0x42; 8];
    // SAFETY: storage is dropped before bytes; no other access until then.
    let storage = unsafe { Storage::from_raw(bytes.as_mut_ptr(), bytes.len()) };
    assert_eq!(storage.as_slice(), &[0x42; 8]);
    drop(storage);
    assert_eq!(bytes, [0x42; 8]);
}

#[test]
fn ringbuffer_owned_writer_exposes_uninitialized_bytes() {
    let buffer = BoxedRingBuffer::new(8);
    // SAFETY: this is the sole writer and the buffer outlives it.
    let mut writer = unsafe { buffer.writer() };
    std::hint::black_box(writer.push_slice()[0]);
}

#[test]
fn ringbuffer_zero_capacity() {
    let buffer = BoxedRingBuffer::new(0);
    assert_eq!(buffer.capacity(), 0);
}

#[test]
fn tinyarc_cell_shared_across_threads() {
    let value = TinyArc::new(Cell::new(0));
    let peer = value.clone();
    let barrier = Arc::new(Barrier::new(2));
    let peer_barrier = barrier.clone();
    let worker = std::thread::spawn(move || {
        peer_barrier.wait();
        peer.set(1);
    });
    barrier.wait();
    value.set(2);
    worker.join().unwrap();
}

#[test]
fn tinyarc_plain_ownership() {
    let value = TinyArc::new(42);
    let clone = value.clone();
    drop(value);
    assert_eq!(*clone, 42);
}

#[test]
fn nolock_overlapping_mutable_references() {
    let lock = crate::nolock::NoLock::new(0);
    let mut first = lock.write();
    let mut second = lock.write();
    let first_ref = &mut *first;
    let second_ref = &mut *second;
    *first_ref = 1;
    *second_ref = 2;
    std::hint::black_box(*first_ref);
}

#[test]
fn nolock_reader_overlaps_writer() {
    let lock = crate::nolock::NoLock::new(0);
    let reader = lock.read();
    let shared = &*reader;
    let mut writer = lock.write();
    *writer = 1;
    std::hint::black_box(*shared);
}

#[test]
fn nolock_shared_across_threads() {
    let lock = Arc::new(crate::nolock::NoLock::new(0));
    let peer = lock.clone();
    let barrier = Arc::new(Barrier::new(2));
    let peer_barrier = barrier.clone();
    let worker = std::thread::spawn(move || {
        peer_barrier.wait();
        *peer.write() = 1;
    });
    barrier.wait();
    *lock.write() = 2;
    worker.join().unwrap();
}

struct IntrusiveRwValue {
    value: usize,
    lock: crate::tinyrwlock::IRwLock<Self, RwOffset>,
}
crate::impl_simple_intrusive_adapter!(RwOffset, IntrusiveRwValue, lock);

#[test]
fn intrusive_rwlock_projects_mutable_owner() {
    let value = IntrusiveRwValue {
        value: 0,
        lock: crate::tinyrwlock::IRwLock::new(),
    };
    let shared = &value.value;
    value.lock.write().value = 42;
    std::hint::black_box(*shared);
    assert_eq!(value.lock.read().value, 42);
}

struct IntrusiveNoLockValue {
    value: usize,
    lock: crate::nolock::INoLock<Self, NoLockOffset>,
}
crate::impl_simple_intrusive_adapter!(NoLockOffset, IntrusiveNoLockValue, lock);

#[test]
fn intrusive_nolock_projects_mutable_owner() {
    let value = IntrusiveNoLockValue {
        value: 0,
        lock: crate::nolock::INoLock::new(),
    };
    let shared = &value.value;
    value.lock.write().value = 42;
    std::hint::black_box(*shared);
    assert_eq!(value.lock.read().value, 42);
}
