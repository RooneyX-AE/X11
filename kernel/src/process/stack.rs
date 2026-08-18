//! User-stack mapping plan.
//!
//! The guard page is intentionally absent from the plan. Mapping ownership is
//! performed by the process loader, while this module owns only the layout.

use crate::memory::{user_stack_range, Page4K, USER_STACK_PAGES, PAGE_SIZE_4K};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StackPlanError {
    InvalidRange,
    InvalidPage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserStackPlan {
    pages: [Option<Page4K>; USER_STACK_PAGES as usize],
    count: usize,
    initial_rsp: u64,
}

impl UserStackPlan {
    pub fn build() -> Result<Self, StackPlanError> {
        let range = user_stack_range().ok_or(StackPlanError::InvalidRange)?;
        let mut pages = [None; USER_STACK_PAGES as usize];
        let mut address = range.start();
        let mut count = 0usize;

        while address < range.end() {
            let page = Page4K::from_start_address(address).ok_or(StackPlanError::InvalidPage)?;
            pages[count] = Some(page);
            count += 1;
            address = address.checked_add(PAGE_SIZE_4K).ok_or(StackPlanError::InvalidRange)?;
        }

        if count != USER_STACK_PAGES as usize {
            return Err(StackPlanError::InvalidRange);
        }

        Ok(Self {
            pages,
            count,
            initial_rsp: range.end(),
        })
    }

    pub const fn count(self) -> usize { self.count }
    pub const fn initial_rsp(self) -> u64 { self.initial_rsp }
    pub fn page(self, index: usize) -> Option<Page4K> {
        if index >= self.count { None } else { self.pages[index] }
    }
}

#[cfg(test)]
mod tests {
    use super::UserStackPlan;
    use crate::memory::{user_stack_guard_range, user_stack_range};

    #[test]
    fn plans_all_stack_pages() {
        let plan = UserStackPlan::build().unwrap();
        assert_eq!(plan.count(), 16);
        assert_eq!(plan.initial_rsp(), user_stack_range().unwrap().end());
    }

    #[test]
    fn guard_page_is_not_part_of_plan() {
        let plan = UserStackPlan::build().unwrap();
        let guard = user_stack_guard_range().unwrap();
        assert!(plan.page(0).unwrap().start_address() >= guard.end());
    }
}
