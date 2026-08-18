//! Voluntary kernel-task yield boundary.
//!
//! A yield never holds a mutable Rust borrow of the runtime across a context
//! switch. The scheduler/runtime validates the task transition first, then
//! this architecture boundary receives raw context pointers whose lifetimes
//! are guaranteed by the caller.

use super::context_switch::{self, Context};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum YieldError {
    NullCurrent,
    NullNext,
    SameContext,
    CurrentUninitialized,
    NextUninitialized,
}

pub fn validate(current: *mut Context, next: *const Context) -> Result<(), YieldError> {
    if current.is_null() {
        return Err(YieldError::NullCurrent);
    }
    if next.is_null() {
        return Err(YieldError::NullNext);
    }
    if core::ptr::eq(current.cast_const(), next) {
        return Err(YieldError::SameContext);
    }

    // SAFETY: callers only pass live context pointers; validation does not mutate them.
    let current_ref = unsafe { &*current };
    let next_ref = unsafe { &*next };

    if !current_ref.is_initialized() {
        return Err(YieldError::CurrentUninitialized);
    }
    if !next_ref.is_initialized() {
        return Err(YieldError::NextUninitialized);
    }

    Ok(())
}

pub unsafe fn activate_first(
    current: *mut Context,
    next: *const Context,
) -> Result<(), YieldError> {
    if current.is_null() {
        return Err(YieldError::NullCurrent);
    }
    if next.is_null() {
        return Err(YieldError::NullNext);
    }
    if core::ptr::eq(current.cast_const(), next) {
        return Err(YieldError::SameContext);
    }

    // SAFETY: caller guarantees a live boot context and initialized next task.
    let next_ref = unsafe { &*next };
    if !next_ref.is_initialized() {
        return Err(YieldError::NextUninitialized);
    }

    // SAFETY: all activation invariants have been checked above.
    unsafe { context_switch::switch(current, next) };
    Ok(())
}

pub unsafe fn switch(current: *mut Context, next: *const Context) -> Result<(), YieldError> {
    validate(current, next)?;
    // SAFETY: contexts are distinct, non-null, initialized, and live per caller contract.
    unsafe { context_switch::switch(current, next) };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{activate_first, validate, YieldError};
    use crate::arch::x86_64::context_switch::Context;

    #[test]
    fn rejects_null_contexts() {
        let mut current = Context::empty();
        assert_eq!(validate(&mut current, core::ptr::null()), Err(YieldError::NullNext));
    }

    #[test]
    fn rejects_same_context() {
        let mut current = Context { rsp: 1, rip: 2, ..Context::empty() };
        let ptr = &current as *const Context;
        assert_eq!(validate(&mut current, ptr), Err(YieldError::SameContext));
    }

    #[test]
    fn rejects_uninitialized_current_context() {
        let mut current = Context::empty();
        let next = Context { rsp: 1, rip: 2, ..Context::empty() };
        assert_eq!(validate(&mut current, &next), Err(YieldError::CurrentUninitialized));
    }

    #[test]
    fn accepts_two_initialized_contexts() {
        let mut current = Context { rsp: 1, rip: 2, ..Context::empty() };
        let next = Context { rsp: 3, rip: 4, ..Context::empty() };
        assert_eq!(validate(&mut current, &next), Ok(()));
    }

    #[test]
    fn first_activation_rejects_uninitialized_next_context() {
        let mut current = Context::empty();
        let next = Context::empty();
        let result = unsafe { activate_first(&mut current, &next) };
        assert_eq!(result, Err(YieldError::NextUninitialized));
    }
}