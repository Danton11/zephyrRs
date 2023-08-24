

import re
from collections import defaultdict

# Dictionary to track allocations
allocations = defaultdict(int)

# Read the log file
with open('serial_output.txt', 'r') as file:
    for line in file:
        # Skip lines that don't start with [MEM_STATS]
        if not line.startswith("[MEM_STATS]"):
            continue

        # Extract allocation and deallocation regions using regular expressions
        allocation_match = re.search(r'AR\[start: (0x[0-9A-Fa-f]+), end: (0x[0-9A-Fa-f]+)\]', line)
        deallocation_match = re.search(r'DR\[start: (0x[0-9A-Fa-f]+), end: (0x[0-9A-Fa-f]+)\]', line)

        if allocation_match:
            start_address = allocation_match.group(1)
            end_address = allocation_match.group(2)
            allocations[(start_address, end_address)] += 1

        if deallocation_match:
            start_address = deallocation_match.group(1)
            end_address = deallocation_match.group(2)
            allocations[(start_address, end_address)] -= 1

# Check for memory leaks
leaks = {k: v for k, v in allocations.items() if v > 0}
if leaks:
    print("Memory leaks detected:")
    for (start_address, end_address), count in leaks.items():
        print(f"Leaked region from {start_address} to {end_address}, leaked {count} times")
else:
    print("No memory leaks detected.")

