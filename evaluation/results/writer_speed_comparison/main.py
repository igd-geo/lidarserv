import json
import matplotlib.pyplot as plt

# Load JSON data from file
with open("data.json", "r") as file:
    data = json.load(file)

# Extract the data for custom_las and las_crate
custom_las_data = data["custom_las"]
las_crate_data = data["las_crate"]

# Separate the data for reading and writing speeds
custom_las_read = [custom_las_data[key]["total_read"] for key in custom_las_data]
custom_las_write = [custom_las_data[key]["total_write"] for key in custom_las_data]

las_crate_read = [las_crate_data[key]["total_read"] for key in las_crate_data]
las_crate_write = [las_crate_data[key]["total_write"] for key in las_crate_data]

# Create the plots
plt.figure(figsize=(12, 6))

# Plot for reading speeds
plt.subplot(1, 2, 1)
plt.plot(list(custom_las_data.keys()), custom_las_read, marker='o', label='Custom LAS')
plt.plot(list(las_crate_data.keys()), las_crate_read, marker='o', label='LAS Crate')
plt.xscale('log')
plt.xlabel('Number of Records')
plt.ylabel('Reading Speed (s)')
plt.title('Reading Speed Comparison')
plt.legend()

# Plot for writing speeds
plt.subplot(1, 2, 2)
plt.plot(list(custom_las_data.keys()), custom_las_write, marker='o', label='Custom LAS')
plt.plot(list(las_crate_data.keys()), las_crate_write, marker='o', label='LAS Crate')
plt.xscale('log')
plt.xlabel('Number of Records')
plt.ylabel('Writing Speed (s)')
plt.title('Writing Speed Comparison')
plt.legend()

# Display the plots
plt.tight_layout()
plt.show()
