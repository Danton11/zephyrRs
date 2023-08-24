// memory_logger.rs

use crate::{println, serial_println};
use bootloader::{bootinfo::{MemoryMap, MemoryRegionType}, BootInfo};
use x86_64::{structures::paging::{FrameAllocator, Mapper, mapper::MapToError,OffsetPageTable, Page, PageTable, PhysFrame, Size4KiB, page::PageRangeInclusive, PageSize, page_table::PageTableEntry, Translate,},PhysAddr, VirtAddr,};
use crate::mem::allocator;
use alloc::{vec, borrow::ToOwned};
use alloc::string::String;
use core::fmt;
use x86_64::instructions::interrupts;
use x86_64::structures::paging::PageTableFlags;
use core::arch::asm;
use alloc::vec::Vec;

pub struct MemoryLogger {
    total_memory: usize,
    used_memory: usize,
    free_memory: usize,
    allocation_successes: usize,
    allocation_failures: usize,
    allocation_region: MemoryRegion,
    deallocation_successes: usize,
    deallocation_failures: usize,
    deallocation_region: MemoryRegion,
    peak_memory_usage: usize,
    memory_regions: Vec<MemoryRegion>,
    // Other fields as needed
}

#[derive(Debug)]
pub struct MemoryRegion {
    pub start_address: u64,
    pub end_address: u64,
    // Other details about the region
}


impl MemoryLogger {
    // Constructor to initialize the logger
    pub fn new() -> Self {
        Self {
            total_memory: 0,
            used_memory: 0,
            free_memory: 0,
            allocation_successes: 0,
            allocation_failures: 0,
            allocation_region: MemoryRegion {start_address:0,end_address:0},
            deallocation_successes: 0,
            deallocation_failures: 0,
            deallocation_region: MemoryRegion {start_address:0,end_address:0},
            peak_memory_usage: 0,
            memory_regions: Vec::new(),
            
            // Other initializations as needed
        }
    }

    pub fn setTotalMemory(&mut self, total_memory: usize){
        self.total_memory = total_memory;
        self.free_memory = total_memory;
    }

   
    pub fn log_allocation(&mut self, mem_region: MemoryRegion, size: usize, success: bool) {
        if success {
            self.used_memory += size;
            self.free_memory -= size;
            self.allocation_successes += 1;
            self.allocation_region = mem_region;
            if self.used_memory > self.peak_memory_usage {
                self.peak_memory_usage = self.used_memory;
            }
        } else {
            self.allocation_failures += 1;
        }

        serial_println!("[MEM_STATS]: TM[{:?}]UM[{:?}]FM[{:?}]AS[{}]AF[{}]AR[start: 0x{:016X}, end: 0x{:016X}]DS[{}]DF[{}]DR[]PU[{:?}]MR[{:?}]",
            self.total_memory, self.used_memory, self.free_memory, self.allocation_successes, self.allocation_failures,
            self.allocation_region.start_address, self.allocation_region.end_address,
            self.deallocation_successes, self.deallocation_failures,
            self.peak_memory_usage, self.memory_regions);
    }



    pub fn log_deallocation(&mut self, mem_region: MemoryRegion, size: usize, success: bool) {
        if success {
            self.used_memory -= size;
            self.free_memory += size;
            self.deallocation_successes += 1;
            self.deallocation_region = mem_region;
        } else {
            self.deallocation_failures += 1;
        }

        serial_println!("[MEM_STATS]: TM[{:?}]UM[{:?}]FM[{:?}]AS[{}]AF[{}]AR[]DS[{}]DF[{}]DR[start: 0x{:016X}, end: 0x{:016X}]PU[{:?}]MR[{:?}]",
            self.total_memory, self.used_memory, self.free_memory, self.allocation_successes, self.allocation_failures,
            self.deallocation_successes, self.deallocation_failures,
            self.deallocation_region.start_address, self.deallocation_region.end_address,
            self.peak_memory_usage, self.memory_regions);
    }
    // Other methods to log specific details, such as regions, fragmentation, etc.

    // Method to print or store the current statistics
    pub fn log_stats(&self) {
        serial_println!("[MEM_STATS]: TM[{:?}]UM[{:?}]FM[{:?}]AS[{}]AF[{}]AR[start: 0x{:016X}, end: 0x{:016X}]DS[{}]DF[{}]DR[start: 0x{:016X}, end: 0x{:016X}]PU[{:?}]MR[{:?}]", self.total_memory, self.used_memory, self.free_memory, self.allocation_successes, self.allocation_failures, self.allocation_region.start_address, self.allocation_region.end_address, self.deallocation_successes, self.deallocation_failures, self.deallocation_region.start_address, self.deallocation_region.end_address, self.peak_memory_usage, self.memory_regions);
    }
}
