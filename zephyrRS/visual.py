import matplotlib.pyplot as plt
import matplotlib
matplotlib.use('TkAgg')
# Read the captured serial output.
with open("serial_output.txt", "r") as f:
    lines = f.readlines()

# Filter out stack monitoring logs.
stack_logs = [line for line in lines if "[STACK_MONITOR]" in line]


# Process the logs.
threads = {}
for log in stack_logs:
    parts = log.split(" ")
    thread_id = int(parts[1][:-1])  # Adjust based on actual log format.
    usage = int(parts[4])           # Adjust based on actual log format.

    if usage > 5000000 or usage < 0: 
        continue

    if thread_id not in threads:
        threads[thread_id] = []

    threads[thread_id].append(usage)

print(threads)
# Plot the data.
for thread_id, usages in threads.items():
    plt.plot(usages, label=f"Thread {thread_id}")

plt.legend()
plt.xlabel("Time (context switches)")
plt.ylabel("Stack Usage (bytes)")
plt.title("Stack Usage Over Time")
plt.show()
