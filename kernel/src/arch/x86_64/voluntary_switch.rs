//! Single-CPU A→B→A voluntary context-switch self-test.
//!
//! This is intentionally isolated from the timer path. It verifies that the
//! saved continuation, kernel stacks, and context-switch ABI cooperate without
//! requiring preemption or userspace state.

use core::sync::atomic::{AtomicU8, Ordering};

use super::context_switch::{switch, Context};

const START: u8 = 0;
const TASK_A: u8 = 1;
const TASK_B: u8 = 2;
const RETURNED_A: u8 = 3;
const COMPLETE: u8 = 4;

#[repr(C)]
struct TestState {
    boot: Context,
    a: Context,
    b: Context,
    state: AtomicU8,
}

extern "C" fn task_a() -> ! {
    extern "C" fn unreachable() -> ! { loop { core::hint::spin_loop(); } }
    let state_ptr: *mut TestState;
    unsafe {
        core::arch::asm!("mov {}, r12", out(reg) state_ptr, options(nomem, nostack, preserves_flags));
        (*state_ptr).state.store(TASK_A, Ordering::SeqCst);
        switch(&mut (*state_ptr).a, &(*state_ptr).b);
        (*state_ptr).state.store(RETURNED_A, Ordering::SeqCst);
        switch(&mut (*state_ptr).a, &(*state_ptr).boot);
    }
    unreachable()
}

extern "C" fn task_b() -> ! {
    let state_ptr: *mut TestState;
    unsafe {
        core::arch::asm!("mov {}, r12", out(reg) state_ptr, options(nomem, nostack, preserves_flags));
        (*state_ptr).state.store(TASK_B, Ordering::SeqCst);
        switch(&mut (*state_ptr).b, &(*state_ptr).a);
    }
    loop { core::hint::spin_loop(); }
}

pub fn run() -> bool {
    let mut state = TestState {
        boot: Context::empty(),
        a: Context::empty(),
        b: Context::empty(),
        state: AtomicU8::new(START),
    };
    let mut stack_a = [0u8; 16 * 1024];
    let mut stack_b = [0u8; 16 * 1024];

    let top_a = stack_a.as_mut_ptr() as usize + stack_a.len();
    let top_b = stack_b.as_mut_ptr() as usize + stack_b.len();

    let mut a = match super::context_switch::bootstrap_context(top_a as u64, &super::activation::ActivationRecord::new(super::super::scheduler::TaskId::new(0, 1), task_a), task_a) {
        Some(context) => context,
        None => return false,
    };
    let mut b = match super::context_switch::bootstrap_context(top_b as u64, &super::activation::ActivationRecord::new(super::super::scheduler::TaskId::new(1, 1), task_b), task_b) {
        Some(context) => context,
        None => return false,
    };

    a.r12 = (&mut state as *mut TestState) as usize as u64;
    b.r12 = (&mut state as *mut TestState) as usize as u64;
    state.a = a;
    state.b = b;
    state.state.store(START, Ordering::SeqCst);

    unsafe {
        switch(&mut state.boot, &state.a);
    }

    state.state.load(Ordering::SeqCst) == RETURNED_A
}
