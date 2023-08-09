use x86_64::VirtAddr;
use x86_64::structures::tss::TaskStateSegment;

use spin::Mutex;
use lazy_static::lazy_static;

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;
pub const PAGE_FAULT_IST_INDEX: u16 = 0;
pub const GENERAL_PROTECTION_FAULT_IST_INDEX: u16 = 0;
pub const TIMER_INTERRUPT_INDEX: u16 = 1;
pub const KEYBOARD_INTERRUPT_INDEX: u16 = 1;
pub const SYSCALL_TEMP_INDEX: u16 = 2;


// lazy initialise the TSS
lazy_static! {
    static ref TSS: Mutex<TaskStateSegment> = {
        //create the TSS
        let mut tss = TaskStateSegment::new();

        // Set up one of 7 stacks for handling double faults
        tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
            //define size of the stack (5 pages)
            const STACK_SIZE: usize = 4096 * 5;

            //create the stack as a static mutable array of bytes (no memory implemented yet)
            //this is unsafe because it can cause data races
            //will be safe as long the handler does not do anything that could overflow the stack
            static mut STACK: [u8; STACK_SIZE] = [0;STACK_SIZE];

            //get the start of the stack in memory using VirtAddr
            let stack_start = VirtAddr::from_ptr(unsafe {&STACK});
            // x86 stack grows downwards, so plus
            let stack_end = stack_start + STACK_SIZE;
            stack_end
        };
        tss.interrupt_stack_table[TIMER_INTERRUPT_INDEX as usize] = tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize];
        
        Mutex::new(tss)
    };
}

pub fn tss_addr() -> u64 {
    let tss_ptr = &*TSS.lock() as *const TaskStateSegment;
    tss_ptr as u64

}

unsafe fn tss_ref() -> &'static TaskStateSegment {
    let tss_ptr = &*TSS.lock() as *const TaskStateSegment;
    & *tss_ptr
}

pub fn set_interrupt_stack_table(index: usize, stack_end: VirtAddr){
    TSS.lock().interrupt_stack_table[index] = stack_end;
}


use x86_64::structures::gdt::{GlobalDescriptorTable, Descriptor, SegmentSelector};


lazy_static! {
    static ref GDT: (GlobalDescriptorTable, Selectors) = {
        //create a new GDT
        let mut gdt = GlobalDescriptorTable::new();

        //add a descriptor for the kernel code segment
        //this is a segment descriptor that defines the properties of the code segment,
        //such as its base address, limit and access rights
        let code_selector = gdt.add_entry(Descriptor::kernel_code_segment());
        let data_selector = gdt.add_entry(Descriptor::kernel_data_segment());

        //add a descriptor for the task state segment
        //this is a special kind of segment descriptor that doesn't describe a segment
        //but rather a data structure used by the CPU for task switches and interrupts
        let tss_selector = gdt.add_entry(Descriptor::tss_segment(unsafe {tss_ref()}));

        let user_data_selector = gdt.add_entry(Descriptor::user_data_segment());
        let user_code_selector = gdt.add_entry(Descriptor::user_code_segment());


        //return the gdt and the selectors
        (gdt, Selectors { code_selector, data_selector, tss_selector, user_code_selector, user_data_selector })
    };
}


struct Selectors {
    code_selector: SegmentSelector,
    data_selector: SegmentSelector,
    tss_selector: SegmentSelector,
    user_data_selector: SegmentSelector,
    user_code_selector: SegmentSelector
}

pub fn init() {
    use x86_64::instructions::segmentation::{Segment, CS, DS};
    use x86_64::instructions::tables::load_tss;

    GDT.0.load();
    unsafe {
        CS::set_reg(GDT.1.code_selector);
        DS::set_reg(GDT.1.data_selector);
        load_tss(GDT.1.tss_selector);
    }
}

pub fn get_user_segments() -> (SegmentSelector, SegmentSelector) {
    (GDT.1.user_code_selector, GDT.1.user_data_selector)
}

pub fn get_kernel_segments() -> (SegmentSelector, SegmentSelector) {
    (GDT.1.code_selector, GDT.1.data_selector)
}
