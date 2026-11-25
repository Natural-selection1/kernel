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

//! CPU-local execution leases. All bookkeeping is protected by the kernel IRQ
//! mask. No allocation, thread reference counting, or atomic RMW is required.
use crate::{arch, support::DisableInterruptGuard};
use core::{
    cell::Cell,
    sync::atomic::{compiler_fence, Ordering},
};

const CPUS: usize = blueos_kconfig::CONFIG_NUM_CORES as usize;
#[cfg(not(smp))]
const _: () = assert!(CPUS == 1, "the local lock backend requires exactly one CPU");

struct CpuState {
    irq_depth: Cell<usize>,
    preempt_depth: Cell<usize>,
    saved_irq_state: Cell<usize>,
    reschedule_pending: Cell<bool>,
}

impl CpuState {
    const fn new() -> Self {
        Self {
            irq_depth: Cell::new(0),
            preempt_depth: Cell::new(0),
            saved_irq_state: Cell::new(0),
            reschedule_pending: Cell::new(false),
        }
    }
}

struct States([CpuState; CPUS]);
// SAFETY: each CPU accesses only its slot, with all kernel-capable asynchronous
// interrupts masked. Scheduling cannot move a live lease to another CPU.
unsafe impl Sync for States {}
static STATES: States = States([const { CpuState::new() }; CPUS]);

#[cold]
fn invalid_context() -> ! {
    // Do not enter the ordinary logger or panic handler from an excluded IRQ.
    core::intrinsics::abort()
}

#[inline(always)]
fn admit() {
    #[cfg(cortex_m)]
    {
        let ipsr: u32;
        unsafe {
            core::arch::asm!("mrs {}, ipsr", out(reg) ipsr, options(nomem, nostack, preserves_flags));
        }
        let exception = ipsr & 0x1ff;
        if exception != 0 {
            admit_exception(exception);
        }
    }
}

#[cfg(cortex_m)]
#[cold]
fn admit_exception(exception: u32) {
    match exception {
        // SVC is synchronous. Borrow-state/lease updates never invoke SVC
        // or callbacks. The actual switch gateway separately checks leases.
        11 | 14 => (),
        15 => {
            if unsafe {
                cortex_m::peripheral::SCB::get_priority(
                    cortex_m::peripheral::scb::SystemHandler::SysTick,
                )
            } < arch::irq::IRQ_PRIORITY_FOR_SCHEDULER
            {
                invalid_context();
            }
        }
        n if n >= 16 => {
            if arch::irq::get_irq_priority(arch::irq::IrqNumber::new((n - 16) as u16))
                < arch::irq::IRQ_PRIORITY_FOR_SCHEDULER
            {
                invalid_context();
            }
        }
        _ => invalid_context(),
    }
}

/// # Safety
/// Stay on the current CPU with kernel IRQs masked while accessing this slot.
#[inline(always)]
unsafe fn state() -> &'static CpuState {
    &STATES.0[arch::current_cpu_id()]
}

#[inline(always)]
pub(crate) fn enter_irq() {
    admit();
    let old = arch::disable_local_irq_save();
    // SAFETY: mask established before selecting the CPU slot.
    let s = unsafe { state() };
    let n = s.irq_depth.get();
    if n == usize::MAX {
        invalid_context();
    }
    if n == 0 {
        s.saved_irq_state.set(old);
    }
    s.irq_depth.set(n + 1);
}

#[inline(always)]
pub(crate) fn exit_irq() {
    admit();
    let s = unsafe { state() };
    let n = s.irq_depth.get();
    assert_ne!(n, 0);
    s.irq_depth.set(n - 1);
    if n != 1 {
        return;
    }
    let old = s.saved_irq_state.get();
    let kick = s.preempt_depth.get() == 0 && s.reschedule_pending.replace(false);
    compiler_fence(Ordering::SeqCst);
    unsafe { arch::enable_local_irq_restore(old) };
    if kick {
        crate::scheduler::yield_me_now_or_later();
    }
}

/// Keeps the current CPU in this execution context while permitting kernel IRQs.
#[derive(Debug)]
#[must_use]
pub struct PreemptDisabled(());

impl !Send for PreemptDisabled {}
impl PreemptDisabled {
    #[inline(always)]
    pub(crate) fn new_with(_irq: &DisableInterruptGuard) -> Self {
        // SAFETY: the borrowed guard protects this update on the current CPU.
        let s = unsafe { state() };
        s.preempt_depth.set(
            s.preempt_depth
                .get()
                .checked_add(1)
                .unwrap_or_else(|| invalid_context()),
        );
        Self(())
    }
}

impl Drop for PreemptDisabled {
    #[inline(always)]
    fn drop(&mut self) {
        let _irq = DisableInterruptGuard::new();
        let s = unsafe { state() };
        s.preempt_depth.set(
            s.preempt_depth
                .get()
                .checked_sub(1)
                .expect("unbalanced preemption lease"),
        );
    }
}

// Inspect only CPU-local scalars while the hardware mask is in effect.
#[inline]
fn with_masked_state<R>(f: impl FnOnce(&CpuState, usize) -> R) -> R {
    admit();
    let old = arch::disable_local_irq_save();
    // SAFETY: these private callers neither schedule nor modify the mask.
    let result = f(unsafe { state() }, old);
    unsafe { arch::enable_local_irq_restore(old) };
    result
}

/// Whether this CPU has an owning IRQ guard or a preemption guard. A blocking
/// adapter must check this before temporarily enabling IRQs for a foreign caller.
#[inline]
pub fn execution_guard_held() -> bool {
    with_masked_state(|s, _| s.irq_depth.get() != 0 || s.preempt_depth.get() != 0)
}

#[inline]
pub(crate) fn defer_if_held() -> bool {
    with_masked_state(|s, _| {
        let held = s.irq_depth.get() != 0 || s.preempt_depth.get() != 0;
        if held {
            s.reschedule_pending.set(true);
        }
        held
    })
}

// An exception return that will unmask interrupts must not invalidate an IRQ
// proof, including one leaked by a handler. Plain guards permit interrupts.
#[inline]
pub(crate) fn assert_irqs_can_enable() {
    assert!(
        with_masked_state(|s, _| s.irq_depth.get() == 0),
        "live IRQ guard at exception return"
    );
}

pub(crate) fn assert_can_suspend(queue_owns_irq: bool) {
    let ok = with_masked_state(|s, old| {
        let resume_mask = if queue_owns_irq {
            s.saved_irq_state.get()
        } else {
            old
        };
        #[cfg(cortex_m)]
        let resumes_enabled = resume_mask == 0;
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        let resumes_enabled = resume_mask & (1 << 3) != 0;
        #[cfg(target_arch = "aarch64")]
        let resumes_enabled = resume_mask & (1 << 7) == 0;
        s.irq_depth.get() == usize::from(queue_owns_irq)
            && s.preempt_depth.get() == 0
            && resumes_enabled
    });
    assert!(ok, "cannot suspend while other execution leases are held");
}

pub(crate) fn assert_can_switch() {
    assert!(
        !execution_guard_held(),
        "cannot switch while an execution lease is held"
    );
}
