
import re
from collections import defaultdict

def analyze_memory(log_file_path):
    rsp_count = defaultdict(int)  # Count of occurrences for each RSP value
    rsp_threads = defaultdict(set)  # Set of thread IDs for each RSP value

    # Regular expression to match the RSP value and Thread ID in your log format
    regex_rsp = r"RSP:\s+(0x[\dA-F]+)"
    regex_thread_id = r"Thread ID:\s+(\d+)"

    current_thread_id = None  # To keep track of the current thread ID while reading the log

    with open(log_file_path, 'r') as f:
        for line in f:
            # Check for Thread ID
            match_thread_id = re.search(regex_thread_id, line)
            if match_thread_id:
                current_thread_id = match_thread_id.group(1)

            # Check for RSP value
            match_rsp = re.search(regex_rsp, line)
            if match_rsp:
                rsp_value = match_rsp.group(1)
                rsp_count[rsp_value] += 1
                if current_thread_id:
                    rsp_threads[rsp_value].add(current_thread_id)

    # Find reused RSP values
    for rsp_value, count in rsp_count.items():
        if count > 1:
            print(f"STACK MEMORY at {rsp_value} REUSED {count} TIMES BY THREADS[ID] {list(rsp_threads[rsp_value])}")

analyze_memory("serial_output.log")

