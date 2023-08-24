import re

def read_memory_regions(filename):
    regions = []
    with open(filename, 'r') as file:
        content = file.read()
        matches = re.findall(r'MemoryRegion { range: FrameRange\((0x[0-9a-f]+)\.\.(0x[0-9a-f]+)\), region_type: (\w+)', content)
        for match in matches:
            start_addr = int(match[0], 16)
            end_addr = int(match[1], 16)
            region_type = match[2]
            regions.append((start_addr, end_addr, region_type))
    return regions

def print_memory_regions(filename):
    regions = read_memory_regions(filename)
    print("Memory Regions:")
    print("---------------")
    for start, end, region_type in regions:
        start_str = hex(start)
        end_str = hex(end)
        print(f"Range: {start_str} to {end_str}, Type: {region_type}")

filename =filename = "serial_output.txt"  # Change to the path to your file
print_memory_regions(filename)

