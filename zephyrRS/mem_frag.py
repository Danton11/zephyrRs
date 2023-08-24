
import re

def parse_regions(log_file):
    allocation_regions = []
    deallocation_regions = []
    with open(log_file, 'r') as file:
        for line in file:
            match = re.search(r'AR\[start: (\w+), end: (\w+)\]', line)
            if match:
                start_address = int(match.group(1), 16)
                end_address = int(match.group(2), 16)
                allocation_regions.append((start_address, end_address))

            match = re.search(r'DR\[start: (\w+), end: (\w+)\]', line)
            if match:
                start_address = int(match.group(1), 16)
                end_address = int(match.group(2), 16)
                deallocation_regions.append((start_address, end_address))

    return allocation_regions, deallocation_regions

def calculate_fragmentation(allocation_regions, deallocation_regions):
    total_regions = len(allocation_regions) + len(deallocation_regions)
    free_regions = len(deallocation_regions)
    fragmentation_ratio = free_regions / total_regions * 100

    print(f"Total Regions: {total_regions}")
    print(f"Allocated at some point regions: {len(allocation_regions)}")
    print(f"Free Regions: {free_regions}")
    print(f"Fragmentation Ratio: {fragmentation_ratio:.2f}%")

allocation_regions, deallocation_regions = parse_regions('serial_output.txt')
calculate_fragmentation(allocation_regions, deallocation_regions)

