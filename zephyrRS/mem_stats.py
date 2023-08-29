
import re

import matplotlib.pyplot as plt


def parse_regions(log_file):
    allocation_regions = []
    deallocation_regions = []

    with open(log_file, 'r') as file:
        for line in file:
            # Extracting allocation regions
            allocation_start = line.find('AR[start:')
            if allocation_start != -1:
                allocation_end = line.find(', end:', allocation_start)
                start_address = int(line[allocation_start + 10:allocation_end], 16)
                end_address = int(line[allocation_end + 7:line.find(']', allocation_end)], 16)
                allocation_regions.append((start_address, end_address))

            # Extracting deallocation regions
            deallocation_start = line.find('DR[start:')
            if deallocation_start != -1:
                deallocation_end = line.find(', end:', deallocation_start)
                start_address = int(line[deallocation_start + 10:deallocation_end], 16)
                end_address = int(line[deallocation_end + 7:line.find(']', deallocation_end)], 16)
                deallocation_regions.append((start_address, end_address))

    return allocation_regions, deallocation_regions

def analyze_regions(log_file):
    allocation_regions, deallocation_regions = parse_regions(log_file)

    print("Allocation Regions:")
    for start, end in allocation_regions:
        print(f"Start: {start:#x}, End: {end:#x}, Size: {end - start}")

    print("\nDeallocation Regions:")
    for start, end in deallocation_regions:
        print(f"Start: {start:#x}, End: {end:#x}, Size: {end - start}")

def parse_memory_statistics(log_lines):
    memory_statistics = []
    pattern = re.compile(r'\[MEM_STATS\]: TM\[(\d+)\]UM\[(\d+)\]FM\[(\d+)\]AS\[(\d+)\]AF\[(\d+)\]AR\[start: (.*?), end: (.*?)\]DS\[(\d+)\]DF\[(\d+)\]DR\[start: (.*?), end: (.*?)\]')
    for line in log_lines:
        match = pattern.match(line)
        if match:
            TM, UM, FM, AS, AF, AR_start, AR_end, DS, DF, DR_start, DR_end = match.groups()
            memory_statistics.append({
                'TM': int(TM),
                'UM': int(UM),
                'FM': int(FM),
                'AS': int(AS),
                'AF': int(AF),
                'AR_start': AR_start,
                'AR_end': AR_end,
                'DS': int(DS),
                'DF': int(DF),
                'DR_start': DR_start,
                'DR_end': DR_end
            })
    return memory_statistics

def analyze_memory_statistics(memory_statistics):
    unused_memory = [entry['UM'] for entry in memory_statistics]
    free_memory = [entry['FM'] for entry in memory_statistics]

    plt.plot(unused_memory, label='Unused Memory')
    plt.plot(free_memory, label='Free Memory')
    plt.xlabel('Time')
    plt.ylabel('Memory (bytes)')
    plt.title('Memory Statistics Over Time')
    plt.legend()
    plt.show()

def analyze_memory(log_file):
    with open(log_file, 'r') as file:
        log_lines = file.readlines()

    memory_statistics = parse_memory_statistics(log_lines)

    analyze_memory_statistics(memory_statistics)


def plot_regions(regions, title):
    starts, ends = zip(*regions)
    sizes = [end - start for start, end in regions]

    plt.bar(starts, sizes, width=1)
    plt.title(title)
    plt.xlabel('Memory Address')
    plt.ylabel('Size')
    plt.show()

allocation_regions, deallocation_regions = parse_regions('serial_output.log')
plot_regions(allocation_regions, 'Allocation Regions')
plot_regions(deallocation_regions, 'Deallocation Regions')
