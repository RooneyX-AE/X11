//! Global Descriptor Table and Task State Segment initialization.
//!
//! The TSS is kept in the architecture layer because IST stack selection is a
//! hardware mechanism. Higher kernel layers depend only on exception events,
//! not descriptor-table implementation details.

use spin::Once;
use x86_64::instructions::segmentation::{CS, Segment};
use x86_64::instructions::tables::load_tss;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;
use x86_64::VirtAddr;

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;
const DOUBLE_FAULT_STACK_SIZE: usize = 4096 * 5;

#[repr(align(16))]
struct ExceptionStack([u8; DOUBLE_FAULT_STACK_SIZE]);

static mut DOUBLE_FAULT_STACK: ExceptionStack = ExceptionStack([0; DOUBLE_FAULT_STACK_SIZE]);
static mut TSS: TaskStateSegment = TaskStateSegment::new();

struct GdtState {
    table: GlobalDescriptorTable,
    kernel_code: SegmentSelector,
    tss: SegmentSelector,
}

static GDT: Once<GdtState> = Once::new();

pub fn init() {
    // SAFETY: early boot runs on the BSP before interrupts are enabled; the
    // TSS and IST stack are initialized exactly once and never moved afterward.
    unsafe {
        let stack_start = VirtAddr::from_ptr(&raw const DOUBLE_FAULT_STACK);
        TSS.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] =
            stack_start + DOUBLE_FAULT_STACK_SIZE as u64;
    }

    let state = GDT.call_once(|| {
        // SAFETY: TSS has been initialized above and is never moved afterward.
        let tss = unsafe { &*(&raw const TSS) };
        let mut table = GlobalDescriptorTable::new();
        let kernel_code = table.append(Descriptor::kernel_code_segment());
        let tss = table.append(Descriptor::tss_segment(tss));

        GdtState {
            table,
            kernel_code,
            tss,
        }
    });

    state.table.load();

    // SAFETY: selectors refer to the descriptors stored in the permanently
    // initialized GDT above and remain valid for the lifetime of the kernel.
    unsafe {
        CS::set_reg(state.kernel_code);
        load_tss(state.tss);
    }
}
