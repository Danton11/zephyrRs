# zephyrRs
# Rust microkernel master's project
## ZephyrRS Kernel Documentation

### main.rs
The entry point of the kernel. It calls the init function defined in lib.rs, which initializes various components of the system. It also sets up a simple memory mapping for testing purposes.

### lib.rs
Serves as a library for the kernel, providing the init function that initializes the system, including the Global Descriptor Table (GDT), the Interrupt Descriptor Table (IDT), and enabling hardware interrupts. It also provides several testing utilities and a QemuExitCode enum for handling QEMU exit codes.

### gdt.rs
Sets up the Global Descriptor Table (GDT) and the Task State Segment (TSS). The init function in lib.rs calls the init function in _gdt.rs_ to set up the GDT and TSS. The setup of the GDT and TSS is crucial for handling task switches and interrupts, core functions of an operating system kernel.

###  interrupts.rs
Handles the setup and handling of hardware interrupts. The init function in lib.rs calls the init_idt function in interrupts.rs to load the IDT into the CPU. The handlers set in this file manage various events, such as hardware interrupts from the timer or the keyboard, and software interrupts like breakpoints, double faults, divide errors, invalid opcodes, and page faults.

###  memory.rs
Handles the setup and management of memory for the kernel. The main.rs file calls the init function in memory.rs to set up the memory management. Memory management is crucial for an operating system kernel, which needs to manage the allocation and deallocation of memory in order to run processes.

### vga_buffer.rs
Handles operations related to the VGA text buffer, which is used for displaying text on the screen in a low-level system like a kernel. The lib.rs and main.rs files likely use the print! and println! macros defined in this file to write text to the VGA buffer, which then appears on the screen.

### serial.rs
Handles operations related to the serial port, which is often used for low-level debugging in operating system kernels. The lib.rs and main.rs files likely use the serial_print! and serial_println! macros defined in this file to write debug information to the serial port.

