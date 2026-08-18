//! Global Descriptor Table initialization.
//!
//! The GDT is deliberately kept behind this architecture module so later
//! scheduler and userspace work does not depend on descriptor-table details.

use spin::Once;
use x86_64::instructions::segmentation::{Segment, CS};
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};

struct GdtState {
    table: GlobalDescriptorTable,
    kernel_code: SegmentSelector,
}

static GDT: Once<GdtState> = Once::new();

pub fn init() {
    let state = GDT.call_once(|| {
        let mut table = GlobalDescriptorTable::new();
        let kernel_code = table.append(Descriptor::kernel_code_segment());

        GdtState { table, kernel_code }
    });

    state.table.load();

    // SAFETY: `kernel_code` refers to the code descriptor stored in the
    // permanently initialized GDT above. The selector remains valid for the
    // lifetime of the kernel.
    unsafe {
        CS::set_reg(state.kernel_code);
    }
}
