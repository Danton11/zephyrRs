
import re
from collections import defaultdict

def calculate_fragmentation(log_file_path):
    # Dictionary to track allocated and deallocated regions
    allocations = defaultdict(int)
    # List to store all memory regions
    all_regions = []

    with open(log_file_path, 'r') as f:
        for line in f:
            # Skip lines that don't start with [MEM_STATS]
            if not line.startswith("[MEM_STATS]"):
                continue

            # Extract allocation and deallocation regions using regular expressions
            allocation_match = re.search(r'AR\[start: (0x[0-9A-Fa-f]+), end: (0x[0-9A-Fa-f]+)\]', line)
            deallocation_match = re.search(r'DR\[start: (0x[0-9A-Fa-f]+), end: (0x[0-9A-Fa-f]+)\]', line)

            if allocation_match:
                start_address = int(allocation_match.group(1), 16)
                end_address = int(allocation_match.group(2), 16)
                all_regions.append((start_address, end_address, 'A'))

            if deallocation_match:
                start_address = int(deallocation_match.group(1), 16)
                end_address = int(deallocation_match.group(2), 16)
                all_regions.append((start_address, end_address, 'D'))

    # Sort all regions by their start address
    all_regions.sort(key=lambda x: x[0])

    # Calculate fragmentation
    fragmentation_count = 0
    last_end_address = 0

    for start_address, end_address, action in all_regions:
        if action == 'A':  # Allocation
            gap = start_address - last_end_address
            if gap > 0:
                fragmentation_count += gap
            last_end_address = max(last_end_address, end_address)
        elif action == 'D':  # Deallocation
            pass  # For now, we ignore deallocations

    print(f"Total Fragmentation: {fragmentation_count} bytes")

calculate_fragmentation("memory_logs.log")

