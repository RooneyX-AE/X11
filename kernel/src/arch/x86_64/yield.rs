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

/// Validate a voluntary context transition without mutating either context.
///
/// This function is intentionally pure with respect to scheduler state. It is
/// the last checked boundary before the unsafe context-switch primitive.
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

    // SAFETY: callers only pass live context pointers; this validation occurs
    // before the context switch and does not mutate the pointed-to values.
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

/// Perform a voluntary context switch after scheduler/runtime validation.
///
/// # Safety
/// The caller must guarantee that both contexts and their backing stacks remain
/// live and exclusive for the duration of the switch. Interrupt/preemption
/// state must be managed by a higher layer.
pub unsafe fn switch(current: *mut Context, next: *const Context) -> Result<(), YieldError> {
    validate(current, next)?;
    // SAFETY: validated non-null, distinct, initialized contexts; lifetime and
    // exclusivity are caller obligations described above.
    unsafe { context_switch::switch(current, next) };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{validate, YieldError};
    use crate::arch::x86_64::context_switch::Context;

    #[test]
    fn rejects_null_contexts() {
        let mut current = Context::empty();
        let result = validate(&mut current, core::ptr::null());
        assert_eq!(result, Err(YieldError::NullNext));
    }

    #[test]
    fn rejects_same_context() {
        let mut current = Context {
            rsp: 1,
            rip: 2,
            ..Context::empty()
        };
        let ptr = &current as *const Context;
        assert_eq!(validate(&mut current, ptr), Err(YieldError::SameContext));
    }

    #[test]
    fn rejects_uninitialized_current_context() {
        let mut current = Context::empty();
        let next = Context {
            rsp: 1,
            rip: 2,
            ..Context::empty()
        };
        assert_eq!(
            validate(&mut current, &next),
            Err(YieldError::CurrentUninitialized)
        );
    }

    #[test]
    fn accepts_two_initialized_contexts() {
        let mut current = Context {
            rsp: 1,
            rip: 2,
            ..Context::empty()
        };
        let next = Context {
            rsp: 3,
            rip: 4,
            ..Context::empty()
        };
        assert_eq!(validate(&mut current, &next), Ok(()));
    }
}
