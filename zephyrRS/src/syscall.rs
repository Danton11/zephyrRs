use core::{arch::asm, slice, str};

use crate::println;

const MSR_STAR: usize = 0xc0000081;
const MSR_LSTAR: usize = 0xc0000082;
const MSR_FMASK: usize = 0xc0000084;

#[naked]
extern "C" fn handle_syscall() {
    unsafe {
        asm!(
            // Here should switch stack to avoid messing with user stack
            // backup registers for sysretq
            "push rcx",
            "push r11",
            "push rbp",
            "push rbx", // save callee-saved registers
            "push r12",
            "push r13",
            "push r14",
            "push r15",

            // Call the rust handler
            "cmp rax, 0",       // if rax == 0 {
            "jne 1f",
            "call {sys_read}",  //   sys_read();
            "1: cmp rax, 1",    // } if rax == 1 {
            "jne 2f",
            "call {sys_write}", //   sys_write();
            "2: ",              // }

            "pop r15", // restore callee-saved registers
            "pop r14",
            "pop r13",
            "pop r12",
            "pop rbx",
            "pop rbp", // restore stack and registers for sysretq
            "pop r11",
            "pop rcx",
            "sysretq", // back to userland
            sys_read = sym sys_read,
            sys_write = sym sys_write,
            options(noreturn));
    }
}

extern "C" fn sys_write(ptr: *mut u8, size:usize) {
    if size == 0 {
        return;
    }

    let slice = unsafe {slice::from_raw_parts(ptr, size)};

    if let Ok(s) = str::from_utf8(slice) {
        println!("[write] '{}'",s);
    }
}

extern "C" fn sys_read() {
    println!("read");
}

pub fn init() {
    let handler_addr = handle_syscall as *const () as u64;
    unsafe {
        asm!("mov ecx, 0xC0000080",
             "rdmsr",
             "or eax, 1",
             "wrmsr");
        
        // clear Interrupt flag on syscall with AMD's MSR_FSTAR register
        asm!("xor rdx, rdx",
             "mov rax, 0x200",
             "wrmsr",
             in("rcx") MSR_FMASK);
        // write handler address to AMD's MSR_LSTAR register
        asm!("mov rdx, rax",
             "shr rdx, 32",
             "wrmsr",
             in("rax") handler_addr,
             in("rcx") MSR_LSTAR);
        // write segments to use on syscall/sysret to AMD'S MSR_STAR register
        asm!(
            "xor rax, rax",
            "mov rdx, 0x230008", // use seg selectors 8, 16 for syscall and 43, 51 for sysret
            "wrmsr",
            in("rcx") MSR_STAR);
    }
}
