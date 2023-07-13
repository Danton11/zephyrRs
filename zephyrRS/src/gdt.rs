use x86_64::VirtAddr;
use x86_64::structures::tss::TaskStateSegment;
use lazy_static::lazy_static;
use x86_64::structures::gdt::{GlobalDescriptorTable,Descriptor};
use x86_64::structures::gdt::SegmentSelector;

use crate::println;


//Define the index into the IST for double dault handling 
//normally the IST starts from 1
pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

// lazy initialise the TSS
lazy_static! {
    static ref TSS: TaskStateSegment = {
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
    tss
    };   
}

lazy_static! { 
    static ref GDT: (GlobalDescriptorTable, Selectors) = {
        //create a new GDT
        let mut gdt = GlobalDescriptorTable::new();
    
        //add a descriptor for the kernel code segment
        //this is a segment descriptor that defines the properties of the code segment,
        //such as its base address, limit and access rights
        let code_selector = gdt.add_entry(Descriptor::kernel_code_segment());

        //add a descriptor for the task state segment
        //this is a special kind of segment descriptor that doesn't describe a segment
        //but rather a data structure used by the CPU for task switches and interrupts
        let tss_selector = gdt.add_entry(Descriptor::tss_segment(&TSS));
        //return the gdt and the selectors
        (gdt, Selectors { code_selector, tss_selector })
    };
}

struct Selectors {
    code_selector: SegmentSelector,
    tss_selector: SegmentSelector,
} 

pub fn init() {

    use x86_64::instructions::tables::load_tss;
    use x86_64::instructions::segmentation::{CS,Segment};

    GDT.0.load();

    // unsafe due to directly manipulating registers
    unsafe {
        // tells the cpu where to find the code segment
        CS::set_reg(GDT.1.code_selector);

        // tells the cpu where to find the code segment
        load_tss(GDT.1.tss_selector);
    }

    println!("Initialised GDT...");
}
