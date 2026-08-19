//! x86_64 adapter from the generic userspace launch plan to `iretq` state.

use crate::process::UserLaunchPlan;

use super::address_space::AddressSpaceRoot;
use super::user_return::{UserReturnError, UserReturnFrame};

pub const USER_CODE_SELECTOR: u16 = 0x1b;
pub const USER_DATA_SELECTOR: u16 = 0x13;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserLaunchAdapterError {
    InvalidReturnFrame(UserReturnError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreparedUserLaunch {
    root: AddressSpaceRoot,
    frame: UserReturnFrame,
}

impl PreparedUserLaunch {
    pub const fn root(self) -> AddressSpaceRoot { self.root }
    pub const fn frame(self) -> UserReturnFrame { self.frame }
}

pub fn prepare_launch(
    root: AddressSpaceRoot,
    plan: UserLaunchPlan,
) -> Result<PreparedUserLaunch, UserLaunchAdapterError> {
    let context = crate::process::InitialContext::new(plan.entry(), plan.stack_pointer())
        .map_err(|_| UserLaunchAdapterError::InvalidReturnFrame(UserReturnError::InvalidRflags))?;
    let frame = UserReturnFrame::from_initial(context, USER_CODE_SELECTOR, USER_DATA_SELECTOR)
        .map_err(UserLaunchAdapterError::InvalidReturnFrame)?;
    Ok(PreparedUserLaunch { root, frame })
}

#[cfg(test)]
mod tests {
    use super::{prepare_launch, USER_CODE_SELECTOR, USER_DATA_SELECTOR};
    use crate::arch::x86_64::address_space::AddressSpaceRoot;
    use crate::memory::{user_stack_range, USER_SPACE_START};
    use crate::process::UserLaunchPlan;

    #[test]
    fn prepared_launch_keeps_root_and_iret_state_together() {
        let root = AddressSpaceRoot::from_physical_address(0x1234_5000).unwrap();
        let stack = user_stack_range().unwrap();
        let plan = UserLaunchPlan { entry: USER_SPACE_START + 0x1000, stack_pointer: stack.end() };
        let prepared = prepare_launch(root, plan).unwrap();
        assert_eq!(prepared.root(), root);
        assert_eq!(prepared.frame().rip, plan.entry());
        assert_eq!(prepared.frame().rsp, plan.stack_pointer());
        assert_eq!(prepared.frame().cs, USER_CODE_SELECTOR as u64);
        assert_eq!(prepared.frame().ss, USER_DATA_SELECTOR as u64);
    }
}
