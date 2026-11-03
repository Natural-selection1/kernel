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

//! Tests the allocation boundary without running the other infra unit tests.
//! The library is linked normally, with this binary supplying its allocator.
//!
//! From the repository root, generate a board="none" GN output directory with
//! the desired nightly rustc on PATH, then run `check_infra_allocation` with
//! ninja. For Miri, also put that nightly's miri executable on PATH and set
//! MIRI_SYSROOT (from `cargo +nightly-2026-05-24 miri setup --print-sysroot`)
//! before gn gen. The `check_infra_allocation_by_miri` target runs both the
//! default aliasing model and Tree Borrows. Use separate output directories
//! for different rustc versions so the dependency libraries remain consistent.

#![feature(allocator_api)]

use blueos_infra::storage::Storage;
use std::alloc::Layout;

use allocation_test_support::{observe, Counts, TestAllocator};

#[global_allocator]
static ALLOCATOR: TestAllocator = TestAllocator;

#[test]
fn box_uninit_reports_allocation_failure() {
    let (result, counts) = observe(true, Box::<u64>::try_new_uninit);
    assert!(result.is_err());
    assert_eq!(
        counts,
        Counts {
            attempts: 1,
            allocations: 0,
            deallocations: 0
        }
    );
}

#[repr(align(4096))]
struct DropProbe(u8);

impl Drop for DropProbe {
    fn drop(&mut self) {
        panic!("uninitialized payload must not be dropped");
    }
}

#[test]
fn box_uninit_frees_storage_without_dropping_payload() {
    let (address, counts) = observe(false, || {
        Box::<DropProbe>::try_new_uninit().map(|storage| {
            let address = storage.as_ptr() as usize;
            drop(storage);
            address
        })
    });
    assert_eq!(address.unwrap() % 4096, 0);
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
fn box_uninit_can_be_initialized_in_place() {
    let (value, counts) = observe(false, || {
        Box::<u64>::try_new_uninit().map(|storage| {
            // Keep the allocation observable even when the optimizer can fold
            // the initialized payload into the returned integer.
            let mut storage = core::hint::black_box(storage);
            storage.write(42);
            // SAFETY: The entire payload was initialized by the preceding write.
            let storage = unsafe { storage.assume_init() };
            *storage
        })
    });
    assert_eq!(value.unwrap(), 42);
    assert_eq!(
        counts,
        Counts {
            attempts: 1,
            allocations: 1,
            deallocations: 1
        }
    );
}

#[repr(align(4096))]
struct AlignedZst;

#[test]
fn box_zero_size_skips_the_global_allocator() {
    let (address, counts) = observe(true, || {
        Box::<AlignedZst>::try_new_uninit().map(|storage| {
            let address = storage.as_ptr() as usize;
            drop(storage);
            address
        })
    });
    let address = address.unwrap();
    assert_ne!(address, 0);
    assert_eq!(address % 4096, 0);
    assert_eq!(counts, Counts::default());
}

#[test]
fn empty_storage_does_not_allocate_or_free() {
    let (_, counts) = observe(true, || {
        drop(Storage::new());
        drop(Storage::default());
    });
    assert_eq!(counts, Counts::default());
}

#[test]
fn storage_reports_allocation_failure_without_freeing() {
    let layout = Layout::from_size_align(37, 16).unwrap();
    let (result, counts) = observe(true, || Storage::try_from_layout(layout));
    assert!(result.is_err());
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
fn storage_has_the_requested_size_alignment_and_writable_extent() {
    for (size, align) in [(1, 1), (37, 16), (257, 4096)] {
        let layout = Layout::from_size_align(size, align).unwrap();
        let (result, counts) = observe(false, || {
            Storage::try_from_layout(layout).map(|storage| {
                let base = storage.base();
                // SAFETY: The successful allocation contains `size` writable
                // bytes. Initialize them before reading, without constructing
                // references to uninitialized bytes through the legacy API.
                unsafe { base.write_bytes(0xa5, size) };
                let contents_match = (0..size).all(|offset| {
                    // SAFETY: Each byte is in bounds and was initialized above.
                    unsafe { base.add(offset).read() == 0xa5 }
                });
                let result = (base as usize, storage.size(), contents_match);
                drop(storage);
                result
            })
        });
        let (address, actual_size, contents_match) = result.unwrap();
        assert_ne!(address, 0);
        assert_eq!(address % align, 0);
        assert_eq!(actual_size, size);
        assert!(contents_match);
        assert_eq!(
            counts,
            Counts {
                attempts: 1,
                allocations: 1,
                deallocations: 1
            }
        );
    }
}

#[test]
fn storage_can_be_dropped_without_initializing_its_bytes() {
    let layout = Layout::new::<DropProbe>();
    let (result, counts) = observe(false, || Storage::try_from_layout(layout).map(drop));
    assert!(result.is_ok());
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
fn storage_zero_size_skips_the_global_allocator() {
    for align in [1, 16, 4096, 1 << 20] {
        let layout = Layout::from_size_align(0, align).unwrap();
        let (result, counts) = observe(true, || {
            Storage::try_from_layout(layout).map(|storage| {
                let result = (storage.base() as usize, storage.size());
                drop(storage);
                result
            })
        });
        let (address, size) = result.unwrap();
        assert_ne!(address, 0);
        assert_eq!(address % align, 0);
        assert_eq!(size, 0);
        assert_eq!(counts, Counts::default());
    }
}

#[test]
fn infallible_storage_obeys_layout_and_moves_without_freeing() {
    for (size, align) in [(37, 16), (257, 4096), (0, 4096)] {
        let layout = Layout::from_size_align(size, align).unwrap();
        let (result, counts) = observe(size == 0, || {
            let storage = Storage::from_layout(layout);
            let before = storage.base();
            let moved = core::hint::black_box(storage);
            let result = (before == moved.base(), moved.base() as usize, moved.size());
            drop(moved);
            result
        });
        assert!(result.0);
        assert_ne!(result.1, 0);
        assert_eq!(result.1 % align, 0);
        assert_eq!(result.2, size);
        let n = usize::from(size != 0);
        assert_eq!(
            counts,
            Counts {
                attempts: n,
                allocations: n,
                deallocations: n
            }
        );
    }
}

#[test]
fn external_storage_preserves_pointer_and_never_frees() {
    let mut bytes = [7u8; 32];
    let base = bytes.as_mut_ptr();
    let (_, counts) = observe(true, || {
        // SAFETY: The initialized backing array outlives this storage and is
        // not accessed while the storage is being used.
        drop(unsafe { Storage::from_raw(base, bytes.len()) });
    });
    assert_eq!(counts, Counts::default());
    assert_eq!(bytes, [7; 32]);
    for base in [
        core::ptr::null_mut(),
        core::ptr::NonNull::<u8>::dangling().as_ptr(),
    ] {
        let (returned, counts) = observe(true, || {
            // SAFETY: A zero-length external range need not own any allocation.
            let storage = unsafe { Storage::from_raw(base, 0) };
            let result = (storage.base(), storage.size());
            drop(storage);
            result
        });
        assert_eq!(returned, (base, 0));
        assert_eq!(counts, Counts::default());
    }
}

// Only the host parent test launches this probe. OOM must not terminate Miri or
// the main test process. Explicit drop prevents an unused-value optimization.
#[cfg(all(unix, not(miri)))]
#[test]
#[ignore = "OOM probe launched in a child process"]
fn storage_oom_child() {
    assert_eq!(
        std::env::var("BLUEOS_STORAGE_OOM_PROBE").as_deref(),
        Ok("1")
    );
    let layout = Layout::from_size_align(37, 16).unwrap();
    let _ = observe(true, || drop(Storage::from_layout(layout)));
    panic!("allocation failure unexpectedly returned");
}

#[cfg(all(unix, not(miri)))]
#[test]
fn infallible_storage_terminates_on_oom() {
    use std::{os::unix::process::ExitStatusExt, process::Command};
    // Suppress core dumps. The executable and its arguments are passed as
    // separate shell arguments, not interpolated into shell source.
    let output = Command::new("sh")
        .args(["-c", "ulimit -c 0; exec \"$@\"", "storage-oom-probe"])
        .arg(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "storage_oom_child", "--nocapture"])
        .env("BLUEOS_STORAGE_OOM_PROBE", "1")
        .output()
        .unwrap();
    // infra exports a weak abort that uses an LLVM trap (SIGILL on x86),
    // whereas a libc-provided abort raises SIGABRT. Both are terminal; a Rust
    // test panic would instead return an ordinary exit code.
    assert!(
        matches!(output.status.signal(), Some(4 | 6)),
        "expected abort/trap, not a test panic: {output:?}"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("memory allocation of 37 bytes failed")
    );
}
