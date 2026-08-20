//! Global Descriptor Table and Task State Segment initialization.
//!
//! Descriptor details remain inside the architecture layer. Scheduler and
//! userspace code consume stable contracts instead of selector/table layout.

use spin::Once;
use x86_64::instructions::segmentation::{CS, DS, ES, SS, Segment};
use x86_64::instructions::tables::load_tss;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;
pub const KERNEL_ENTRY_STACK_SIZE: usize = 32 * 1024;
const DOUBLE_FAULT_STACK_SIZE: usize = 32 * 1024;

/// Ring-3 selectors are the single source of truth for the privilege-transition contract.
pub(crate) const USER_DATA_SELECTOR: u16 = 0x13;
pub(crate) const USER_CODE_SELECTOR: u16 = 0x1b;

const _: () = {
    assert!((USER_DATA_SELECTOR >> 3) == 2);
    assert!((USER_CODE_SELECTOR >> 3) == 3);
    assert!((USER_DATA_SELECTOR & 3) == 3);
    assert!((USER_CODE_SELECTOR & 3) == 3);
};

#[repr(align(16))]
struct AlignedStack<const SIZE: usize>([u8; SIZE]);

static KERNEL_ENTRY_STACK: AlignedStack<KERNEL_ENTRY_STACK_SIZE> = AlignedStack([0; KERNEL_ENTRY_STACK_SIZE]);
static DOUBLE_FAULT_STACK: AlignedStack<DOUBLE_FAULT_STACK_SIZE> = AlignedStack([0; DOUBLE_FAULT_STACK_SIZE]);
static TSS: Once<TaskStateSegment> = Once::new();

struct GdtState {
    table: GlobalDescriptorTable,
    kernel_code: SegmentSelector,
    kernel_data: SegmentSelector,
    user_data: SegmentSelector,
    user_code: SegmentSelector,
    tss_selector: SegmentSelector,
}

static GDT: Once<GdtState> = Once::new();

pub fn init() {
    let tss = TSS.call_once(|| {
        let mut tss = TaskStateSegment::new();

        let kernel_stack_start = x86_64::VirtAddr::from_ptr(core::ptr::addr_of!(KERNEL_ENTRY_STACK));
        let kernel_stack_end = kernel_stack_start + KERNEL_ENTRY_STACK_SIZE as u64;
        tss.privilege_stack_table[0] = kernel_stack_end;

        let stack_start = x86_64::VirtAddr::from_ptr(core::ptr::addr_of!(DOUBLE_FAULT_STACK));
        let stack_end = stack_start + DOUBLE_FAULT_STACK_SIZE as u64;
        tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = stack_end;
        tss
    });

    let state = GDT.call_once(|| {
        let mut table = GlobalDescriptorTable::new();
        let kernel_code = table.append(Descriptor::kernel_code_segment());
        let kernel_data = table.append(Descriptor::kernel_data_segment());
        let user_data = table.append(Descriptor::user_data_segment());
        let user_code = table.append(Descriptor::user_code_segment());
        let tss_selector = table.append(Descriptor::tss_segment(tss));

        let actual_user_data = (user_data.index() << 3) | 3;
        let actual_user_code = (user_code.index() << 3) | 3;
        assert_eq!(actual_user_data, USER_DATA_SELECTOR);
        assert_eq!(actual_user_code, USER_CODE_SELECTOR);

        GdtState {
            table,
            kernel_code,
            kernel_data,
            user_data,
            user_code,
            tss_selector,
        }
    });

    state.table.load();

    // SAFETY: selectors reference descriptors stored in the permanently
    // initialized GDT and TSS above. Both live for the kernel lifetime.
    // Reloading the data segments prevents stale selectors from referring to
    // descriptor entries in the pre-GDT firmware/bootloader table.
    unsafe {
        CS::set_reg(state.kernel_code);
        DS::set_reg(state.kernel_data);
        ES::set_reg(state.kernel_data);
        SS::set_reg(state.kernel_data);
        load_tss(state.tss_selector);
    }
}

pub fn kernel_code_selector() -> Option<u16> {
    GDT.get().map(|state| state.kernel_code.index() << 3)
}

pub fn user_code_selector() -> Option<u16> {
    GDT.get().map(|state| (state.user_code.index() << 3) | 3)
}

pub fn user_data_selector() -> Option<u16> {
    GDT.get().map(|state| (state.user_data.index() << 3) | 3)
}

#[cfg(test)]
mod tests {
    use super::{KERNEL_ENTRY_STACK_SIZE, USER_CODE_SELECTOR, USER_DATA_SELECTOR};

    #[test]
    fn ring3_selector_contract_is_stable() {
        assert_eq!(USER_DATA_SELECTOR, 0x13);
        assert_eq!(USER_CODE_SELECTOR, 0x1b);
        assert_eq!(USER_DATA_SELECTOR & 3, 3);
        assert_eq!(USER_CODE_SELECTOR & 3, 3);
    }

    #[test]
    fn kernel_entry_stack_has_expected_size() {
        assert_eq!(KERNEL_ENTRY_STACK_SIZE, 32 * 1024);
    }
}
