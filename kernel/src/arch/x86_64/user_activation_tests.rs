//! Structural integration checks for the userspace activation boundary.
//!
//! These tests intentionally stop before executing `iretq`. Hardware execution
//! belongs to QEMU/CI because a unit test cannot prove a real CPL3 transition.

use super::address_space::AddressSpaceRoot;
use super::user_launch::{prepare_launch, USER_CODE_SELECTOR, USER_DATA_SELECTOR};
use crate::memory::{user_stack_range, USER_SPACE_START};
use crate::process::UserLaunchPlan;

#[test]
fn prepared_launch_matches_gdt_selector_contract() {
    let root = AddressSpaceRoot::from_physical_address(0x1234_5000).unwrap();
    let stack = user_stack_range().unwrap();
    let plan = UserLaunchPlan {
        entry: USER_SPACE_START + 0x1000,
        stack_pointer: stack.end(),
    };
    let prepared = prepare_launch(root, plan).unwrap();

    assert_eq!(prepared.root(), root);
    assert_eq!(prepared.frame().cs, USER_CODE_SELECTOR as u64);
    assert_eq!(prepared.frame().ss, USER_DATA_SELECTOR as u64);
    assert_eq!(prepared.frame().rip, plan.entry());
    assert_eq!(prepared.frame().rsp, plan.stack_pointer());
    assert_eq!(prepared.frame().rflags, 0x202);
}
