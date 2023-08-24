
import re
import matplotlib.pyplot as plt

def read_memory_usage(filename):
    free_count = 0
    used_count = 0

    # Regular expression to match the pattern in the output
    pattern = re.compile(r'\[MEMORY_MONITOR\]\[Frame (\d+)\]: (Free|Used)')

    with open(filename, 'r') as file:
        for line in file:
            print(line)
            match = pattern.search(line)
            if match:
                frame_index, status = match.groups()
                if status == "Free":
                    free_count += 1
                elif status == "Used":
                    used_count += 1

    return free_count, used_count


def visualize_memory_usage(filename):
    free_count, used_count = read_memory_usage(filename)

    # Check if there is at least one free or used frame
    if free_count == 0 and used_count == 0:
        print("No data available to visualize.")
        return
    
    # Create a pie chart
    labels = 'Free', 'Used'
    sizes = [free_count, used_count]
    colors = ['green', 'red']
    explode = (0, 0.1)  # explode a slice for emphasis

    plt.pie(sizes, explode=explode, labels=labels, colors=colors,
            autopct='%1.1f%%', shadow=True, startangle=140)
    
    plt.axis('equal')
    plt.title('Memory Usage')
    plt.show()


filename = "serial_output.txt"  # Change to the path to your file
visualize_memory_usage(filename)

