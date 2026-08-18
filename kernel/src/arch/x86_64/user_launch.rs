//! x86_64 adapter from the generic userspace launch plan to `iretq` state.

use crate::process::UserLaunchPlan;

use super::user_return::{UserReturnError, UserReturnFrame};

pub const USER_CODE_SELECTOR: u16 = 0x23;
pub const USER_DATA_SELECTOR: u16 = 0x1b;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserLaunchAdapterError {
    InvalidReturnFrame(UserReturnError),
}

pub fn make_return_frame(
    plan: UserLaunchPlan,
) -> Result<UserReturnFrame, UserLaunchAdapterError> {
    let context = crate::process::InitialContext::new(plan.entry(), plan.stack_pointer())
        .map_err(|_| UserLaunchAdapterError::InvalidReturnFrame(UserReturnError::InvalidRflags))?;
    UserReturnFrame::from_initial(context, USER_CODE_SELECTOR, USER_DATA_SELECTOR)
        .map_err(UserLaunchAdapterError::InvalidReturnFrame)
}
