Arc foundation contracts
========================

Import blueos_infra::arc::{Arc, UniqueArc}; these are distinct from the old
TinyArc and the kernel's current Arc alias. This module is a heap-only,
strong-only, Sized foundation, not a replacement for existing containers.

UniqueArc owns one initialized T and may mutate/replace it. Conversion into
Pin<UniqueArc<T>> or Arc<T> fixes the value's address through destruction.
Both handles are Unpin independently of T; Pin's target restrictions still
prevent extracting or replacing a !Unpin payload. All consuming transitions
preserve the allocation without allocating or incrementing its reference count.
No reverse transition, mutable shared access, or owning raw-pointer API exists.

A private repr(C) control block stores AtomicUsize-backed Refcount and T. All
constructors are fallible. Even a zero-sized T requires a control block.
Arc's Send and Sync require T: Send + Sync. UniqueArc's Send requires T: Send,
and its Sync requires T: Sync: borrowing it cannot clone or transfer ownership.

Reference-count safety
----------------------
Each Arc owns one reference; UniqueArc exclusively owns the initial reference.
An outstanding owner keeps the count nonzero while Clone performs a Relaxed CAS.
This operation extends lifetime, not publication: transfer between threads
still requires synchronization by the caller. A release consumes the caller's
reference and uses a Release CAS. All successful count changes are atomic RMWs,
so the last decrement and its Acquire fence synchronize with prior releases
through their release sequences before T is destroyed. The last releaser alone
reconstructs the original Box, which also releases storage if T::drop unwinds.

usize::MAX is an absorbing saturation state. Incrementing MAX-1 saturates;
subsequent increments and releases cannot change it. CAS observes the actual
current state, so a racing release cannot undo another thread's saturation.
No operation transiently wraps. Saturation intentionally leaks both storage and
payload instead of risking a use-after-free. Tests exercise the private count
without leaking actual allocations or changing the production maximum.

Validation
----------
check_infra_arc: public contracts, private arc::refcount_tests:: count tests, and
positive/negative API compile checks. check_infra_arc_by_miri runs public and
private tests under both default and Tree Borrows models. The private tests
compile the normal library test entry point and filter only execution; no
production implementation is included via #[path] or copied into a test crate.
The test filter does not disable any Miri checks. Concurrent tests use a bounded
number of threads and reduce only iteration counts under Miri; they are not an
exhaustive proof of the atomic algorithm.

Use the same GNRT sync/compiler environment as other isolated infra tests.
Changing toolchains requires synchronization before generating/building targets.
The older TinyArc, external storage APIs and intrusive containers retain their
previous safety limitations; passing these targets does not audit them.

Initialization
--------------
Import blueos_infra::init::{InPlaceInit, InPlaceWrite}, not pin_init's trait
of the same name. GN callers of pin-init macros need a direct dependency on
//external/pin-init:pin_init. Both owners implement four fallible constructors;
UniqueArc returns Pin<UniqueArc<T>> for pin initialization, Arc returns itself.
Arc delegates every initialization path to UniqueArc before sharing ownership.

UniqueArc::<T>::try_new_uninit() creates a control block in place using the
existing Box initializer and pin_init::uninit. InPlaceWrite consumes that owner.
The initializer cleans completed fields on error/unwind, then the owner frees
storage without dropping a complete T. Success changes the owner type to T.
The private cast relies on repr(C) field offsets and MaybeUninit<T>'s identical
size/alignment, not on an enclosing Rust layout or niche optimization. The
count is already valid throughout; only the payload is uninitialized. A pinned
initializer's successful value is immediately pinned without movable exposure.
