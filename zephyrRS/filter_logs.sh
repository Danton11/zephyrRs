#!/bin/bash

# Check if the program's output file exists
if [ ! -f "serial_output.log" ]; then
    echo "program_output.txt not found. Please make sure it exists."
    exit 1
fi

# Filter lines containing [MEM_STATS] and save them to memory_logs.log
grep '\[MEM_STATS\]' serial_output.log > memory_logs.log

# Filter lines containing [STACK_MONITOR] and save them to stack_logs.log
grep '\[STACK_MONITOR\]' serial_output.log > stack_logs.log

echo "Logs have been filtered and saved."
