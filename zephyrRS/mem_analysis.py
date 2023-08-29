

import re
from collections import defaultdict

def analyze_memory(log_file_path):
    # Dictionary to track allocations and deallocations
    allocations = defaultdict(int)
    # Sets to track unique allocated and deallocated regions
    allocated_regions = set()
    deallocated_regions = set()
    # Dictionary to track reuse count
    region_reuse_count = {}

    with open(log_file_path, 'r') as f:
        for line in f:
            # Skip lines that don't start with [MEM_STATS]
            if not line.startswith("[MEM_STATS]"):
                continue

            # Extract allocation and deallocation regions using regular expressions
            allocation_match = re.search(r'AR\[start: (0x[0-9A-Fa-f]+), end: (0x[0-9A-Fa-f]+)\]', line)
            deallocation_match = re.search(r'DR\[start: (0x[0-9A-Fa-f]+), end: (0x[0-9A-Fa-f]+)\]', line)

            if allocation_match:
                start_address = allocation_match.group(1)
                end_address = allocation_match.group(2)
                region = (start_address, end_address)
                allocated_regions.add(region)
                allocations[region] += 1
                region_reuse_count[region] = region_reuse_count.get(region, 0) + 1

            if deallocation_match:
                start_address = deallocation_match.group(1)
                end_address = deallocation_match.group(2)
                region = (start_address, end_address)
                deallocated_regions.add(region)
                allocations[region] -= 1

    

    # Find reused regions
    for region, count in region_reuse_count.items():
        if count > 1:
            print(f"Region {region} reused {count} times")

    # Find undeallocated regions
    for region in allocated_regions:
        if region not in deallocated_regions:
            print(f"Region {region} was not deallocated")

analyze_memory("memory_logs.log")

