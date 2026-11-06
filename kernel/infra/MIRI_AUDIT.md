# infra Miri audit on main

This is a diagnostic baseline, not a claim that infra is memory-safe.
Production implementations are unchanged. The audit intentionally exits with an
error when any test reports UB, leaks, panics, unsupported operations, an ignored
test, or a timeout. No known-failure allowlist is used.

## Baseline and execution

- Source: local `main`, `7e124fa98f6acd4561736d4325a2edcb0f5b3de2`.
- Toolchain: `nightly-2026-05-24-x86_64-unknown-linux-gnu`.
- Models: default Stacked Borrows and `-Zmiri-tree-borrows`.
- Discover every `#[test]` through the actual infra library test entry point;
  benchmarks are listed in `discovery.log` but not executed.
- Each test runs in its own interpreter process with an exact filter and one
  libtest worker. Threads spawned by the test are unchanged.
- Default wall-clock limit: 60 seconds per test. A timeout is inconclusive, not UB.
- The original 113 tests also run individually on the native host.
- Add 16 Miri-only contract probes (including three positive controls). Potential
  UB reproducers are not run natively and are not marked `should_panic`.
- No production source is copied, no cross-directory `#[path]` is used, no Miri
  checks are disabled, and existing workloads/iteration counts are unchanged.

Prepare GNRT with the same Rust toolchain used for GN, following the repository's
GNRT workflow. The installed Miri sysroot must match Miri's compiler version.
For the audited environment:

```sh
export PATH="$HOME/.rustup/toolchains/nightly-2026-05-24-x86_64-unknown-linux-gnu/bin:$HOME/.cargo/bin:$PATH"
export MIRI_SYSROOT="$HOME/.cache/miri"
gnrt sync --all
gnrt sync --all --check
gn gen out/miri-audit
ninja -k 0 -j 2 -C out/miri-audit check_infra_miri_audit
```

`-k 0` allows both model targets to finish even when one fails. Each model writes:

```text
out/miri-audit/miri/bin/infra_miri_audit_{stacked,tree}.cases/
    discovery.log
    results.json
    results.md
    NNN-test_name.log
```

Every case log starts with its exact invocation. Run it from `out/miri-audit`.
The JSON report includes completion status, elapsed time and exit status. Reports
are updated after each case, so interrupted audits retain completed observations.
No passing GN stamp is written for a failed audit.

To narrow a failed audit or investigate a timeout:

```sh
BLUEOS_MIRI_TEST_FILTER='list::typed_ilist::tests::test_insert_and_detach' \
  ninja -k 0 -C out/miri-audit check_infra_miri_audit
BLUEOS_MIRI_CASE_TIMEOUT=300 \
  ninja -k 0 -C out/miri-audit check_infra_miri_audit
```

A filtered run overwrites that target's report; archive reports before rerunning.
If an earlier audit passed, Ninja may reuse its stamp: remove only the relevant
`out/miri-audit/miri/bin/infra_miri_audit_{stacked,tree}` stamp to force a new run.
Environment filters do not change Ninja's command fingerprint.

Native baseline and Python runner checks:

```sh
ninja -C out/miri-audit kernel/infra:infra_unittest
python3 build/scripts/miri/isolated_tests.py \
  --output-dir out/miri-audit-review/host --timeout 60 -- \
  out/miri-audit/bin/infra_unittest
python3 build/scripts/miri/test_isolated_tests.py
```

## Observed results (2026-11-06)

| Execution | Passed | UB diagnostics | Leak failures | Timeout | Total |
| --- | ---: | ---: | ---: | ---: | ---: |
| Native original tests | 113 | 0 | 0 | 0 | 113 |
| Miri Stacked Borrows, 60s/case | 71 | 56 | 2 | 0 | 129 |
| Miri Tree Borrows, 60s/case | 77 | 49 | 2 | 1 | 129 |

The sole timeout, `list::singly_linked_list::tests::test_push` (65,536 nodes),
passed in 97.916 seconds with a 180-second limit. Including this recheck,
Tree Borrows has **78 passes, 49 UB diagnoses, 2 leak failures, no unresolved
timeouts**. Its original 60-second result above is retained for transparency.

Of these, the 16 added probes yield 3 passes and 13 UB diagnoses in each model.
The original tests contribute 43 Stacked/36 Tree UB diagnoses. These are counts
of failing test cases, not counts of independent bugs.

The complete per-module/per-case table and original command logs are preserved in
`out/miri-audit-review/summary.md` and each model's `.cases` directory. The Python
runner's five regression tests pass, including failure continuation, timeout,
empty selection, discovery failure, and colored diagnostic classification.
Real-Miri runner integration also confirms that a passing case creates a GN
stamp, while a UB case returns failure without creating a passing stamp.

## Interpreting findings

A failing test is not necessarily an independent defect. Many TinyArcList and
sorted-list failures reach the same intrusive-list operations. Conversely, a
passing test only covers its executed paths. Neither model exhaustively explores
thread schedules, all inputs, all layouts, or all unsafe caller obligations.
Borrow-model-specific failures are recorded separately from data races and invalid
values; both aliasing models are experimental. An integer-to-pointer warning is
not counted as an additional UB finding.

### Allocation, initialization and safe access

`storage.rs:33` forwards zero-sized layouts to the raw allocator; zero size is
outside that allocation API's contract. `storage.rs:67` and `:72` form slice
references directly, including from the null empty representation. The public
`Storage::Alloc` variant lets safe code create an invalid owning pointer.

Fresh storage remains uninitialized, but `as_slice` exposes readable bytes safely.
`BoxedRingBuffer::new` inherits that storage and `Writer::push_slice` exposes
readable `&mut [u8]` before initialization. The contract probes read these bytes;
they do not assume any particular uninitialized value.

The initialized owned-storage, valid external-storage and plain TinyArc tests are
positive controls. Findings here concern main's implementation; they do not undo
or reassess the allocation work on the separate foundation branches.

### NoLock and intrusive lock projection

`NoLock::try_write` always returns a guard. Safe callers can retain overlapping
mutable references or retain a shared reference while writing. Its unconditional
lack of exclusion also permits races through `Arc<NoLock<usize>>` under its current
`Sync` implementation. Single-core intended use does not prevent same-thread
aliasing through the exposed safe API.

`INoLock::this_mut` (`nolock.rs:140`) and `IRwLock::this_mut`
(`tinyrwlock.rs:846`) recover an enclosing `&mut T` from `&self`. The enclosing
ordinary fields are not protected from safe shared references by `UnsafeCell`.
The probes retain a shared reference to an ordinary enclosing field, write that
field through the intrusive lock, and then read the reference. The offset adapter
is correct and the owner stays alive and unmoved throughout the test.

### TinyArc and TinyArcCas

`TinyArc<T>` implements `Send` and `Sync` without payload bounds. A safe test can
therefore share `TinyArc<Cell<i32>>` between threads, producing a data race. The
barrier starts the two writes but does not order them relative to each other.

`TinyArcCas` stores an ordinary `Option<TinyArc<T>>` and casts its address to
`&AtomicPtr<_>` in `cas`/`swap` (`tinyarc/mod.rs:584`, `:601`). A pointer cast does
not turn storage behind a shared reference into legal interior-mutable storage.
The existing CAS/swap tests reproduce the violation without adding new code paths.

### Intrusive nodes, aliases and owner reconstruction

- `typed_ilist`: stored node pointers are reused across fresh exclusive borrows;
  insertion/detachment reborrows can invalidate previously stored access paths.
  The legacy interfaces do not express a complete lifetime/pinning/alias contract.
- `typed_atomic_ilist`: atomic fields do not establish exclusive access to the
  entire node or enclosing object. Some concurrent tests call unsafe
  `TinyArc::get_mut_unchecked` while clones exist; diagnostics also reach the
  reference count because that helper first creates `&mut TinyArcInner<T>`.
  These cases involve the unsafe test usage/helper contract, not just list logic.
- `iheap`: `push` saves a raw pointer then reborrows the same node for the returned
  guard (`iheap.rs:246-248`); another path obtains a mutable embedded node then
  shares its whole owner with `bottom_up_adjust` (`:266`). Later traversal and
  writes fail the borrowing rules. This occurs even when an external lock
  serializes the algorithm.
- `rbtree`: `insert` obtains `&mut RBLink`, then passes a shared reference to its
  enclosing object into the comparator, then writes through the earlier mutable
  reference (`rbtree.rs:166-169`). The Stacked Borrows run detects this conflict.
  Tagged integer pointers additionally emit provenance warnings; do not silence
  them or count Tree Borrows success as complete provenance validation.
- TinyArcList and sorted-list tests inherit several of the above node problems.

Pinning alone cannot repair these aliasing violations. Fixes need to distinguish
stable addresses, lifetime/ownership and permissible shared/exclusive access.

### Test leaks, limited coverage and slow workloads

`typed_ilist::tests::test_listiterator` and `test_listreverseiterator` each
leak two nodes with `Box::leak` and do not recover them. This is a test cleanup issue, not evidence that every list
operation leaks. Leak checking remains enabled.

Several existing stress tests create tens of thousands of nodes or large numbers
of threads. Their timeouts are retained as inconclusive; passing native tests do
not substitute for Miri. The full Miri benchmark suite is not included.
`no_let_underscore` has no runtime test discovered by this audit; the adapter and
lifetime helper tests have limited coverage. Send/Sync bounds, refcount overflow,
external-memory lifetime, and all possible teardown schedules require additional
compile-time and targeted runtime checks beyond this baseline.

## Review scope

This branch adds test infrastructure, contract probes and this diagnostic record.
It does not repair production implementations, merge the ownership-foundation
branches, change main, or modify the parent conversation's checkout. The audit
remains opt-in rather than making known failures a green CI baseline.
