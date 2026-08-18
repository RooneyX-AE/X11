//! User-address validation primitives.
//!
//! This module never dereferences a user pointer. It only proves that a
//! pointer/length pair lies entirely inside the kernel's declared user range.
//! Actual page-table/access checks belong to the active address-space layer.

use super::{KERNEL_SPACE_START, USER_SPACE_START, VirtRange};
use x11_os_abi::UserSlice;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserRangeError {
    NullPointer,
    AddressOverflow,
    OutsideUserSpace,
}

pub fn validate_slice(slice: UserSlice) -> Result<VirtRange, UserRangeError> {
    if slice.len == 0 {
        return Ok(VirtRange::new(slice.ptr, slice.ptr).expect("empty range must be ordered"));
    }
    if slice.ptr == 0 {
        return Err(UserRangeError::NullPointer);
    }

    let end = slice
        .ptr
        .checked_add(slice.len)
        .ok_or(UserRangeError::AddressOverflow)?;
    let range = VirtRange::new(slice.ptr, end).ok_or(UserRangeError::AddressOverflow)?;
    if range.start() < USER_SPACE_START || range.end() > KERNEL_SPACE_START {
        return Err(UserRangeError::OutsideUserSpace);
    }
    Ok(range)
}

#[cfg(test)]
mod tests {
    use super::{validate_slice, UserRangeError, KERNEL_SPACE_START, USER_SPACE_START};
    use x11_os_abi::UserSlice;

    #[test]
    fn accepts_normal_user_buffer() {
        let range = validate_slice(UserSlice {
            ptr: USER_SPACE_START,
            len: 4096,
        })
        .unwrap();
        assert_eq!(range.start(), USER_SPACE_START);
        assert_eq!(range.end(), USER_SPACE_START + 4096);
    }

    #[test]
    fn rejects_null_non_empty_buffer() {
        assert_eq!(
            validate_slice(UserSlice { ptr: 0, len: 1 }),
            Err(UserRangeError::NullPointer)
        );
    }

    #[test]
    fn rejects_overflow() {
        assert_eq!(
            validate_slice(UserSlice {
                ptr: u64::MAX - 3,
                len: 8,
            }),
            Err(UserRangeError::AddressOverflow)
        );
    }

    #[test]
    fn rejects_kernel_address() {
        assert_eq!(
            validate_slice(UserSlice {
                ptr: KERNEL_SPACE_START,
                len: 1,
            }),
            Err(UserRangeError::OutsideUserSpace)
        );
    }

    #[test]
    fn allows_empty_slice_without_dereference_requirement() {
        assert_eq!(validate_slice(UserSlice::empty()).unwrap().len(), 0);
    }
}
