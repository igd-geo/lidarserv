import json
import matplotlib.pyplot as plt

def load_json(file_path):
    with open(file_path) as f:
        data = json.load(f)
    return data

def calculate_averages(json_data):
    num_nodes_sum = 0
    num_nodes_per_level_sum = [0] * 11
    num_points_sum = 0

    for key in range(1, 21):
        num_nodes_sum += json_data[str(key)]["runs"]["indexing"][0]["results"]["index_info"]["directory_info"]["num_nodes"]
        num_nodes_per_level = json_data[str(key)]["runs"]["indexing"][0]["results"]["index_info"]["directory_info"]["num_nodes_per_level"]
        num_nodes_per_level_sum = [sum(x) for x in zip(num_nodes_per_level_sum, num_nodes_per_level)]
        num_points_sum += json_data[str(key)]["runs"]["indexing"][0]["results"]["insertion_rate"]["nr_points"]

    num_nodes_avg = num_nodes_sum / 20
    num_nodes_per_level_avg = [x / 20 for x in num_nodes_per_level_sum]
    num_points_avg = num_points_sum / 20

    return num_nodes_avg, num_nodes_per_level_avg, num_points_avg

def compare_and_plot(json_data_1, json_data_2, name):
    num_nodes_avg_1, num_nodes_per_level_avg_1, num_points_avg1 = calculate_averages(json_data_1)
    num_nodes_avg_2, num_nodes_per_level_avg_2, num_points_avg2 = calculate_averages(json_data_2)

    plt.figure(figsize=(10, 10))
    plt.subplot(1, 1, 1)
    plt.plot(range(1, 12), num_nodes_per_level_avg_1, label='Freiburg')
    plt.plot(range(1, 12), num_nodes_per_level_avg_2, label='Frankfurt')
    plt.xlabel('Level')
    plt.ylabel('Average num_nodes_per_level')
    plt.title('Average num_nodes_per_level Comparison ' + name)
    plt.legend()
    plt.savefig('plots/num_nodes_per_level_comparison_' + name + '.pdf')

    plt.figure(figsize=(10, 10))
    plt.subplot(1, 1, 1)
    plt.bar(['Freiburg', 'Frankfurt'], [num_nodes_avg_1, num_nodes_avg_2])
    plt.xlabel('City')
    plt.ylabel('Average num_nodes')
    plt.title('Average num_nodes Comparison ' + name)
    plt.savefig('plots/num_nodes_comparison_' + name + '.pdf')

    plt.figure(figsize=(10, 10))
    plt.subplot(1,1,1)
    plt.bar(['Freiburg', 'Frankfurt'], [num_points_avg1, num_points_avg2])
    plt.xlabel('City')
    plt.ylabel('Average num_points')
    plt.title('Average num_points Comparison ' + name)
    plt.savefig('plots/num_points_comparison_' + name + '.pdf')

if __name__ == '__main__':
    # Freiburg
    file_path_1 = 'merged_output/merged_freiburg_0,1s.json'
    # Frankfurt
    file_path_2 = 'merged_output/merged_frankfurt_0,1s.json'

    json_data_1 = load_json(file_path_1)
    json_data_2 = load_json(file_path_2)

    compare_and_plot(json_data_1, json_data_2, "0,1s chunks")

    # Freiburg
    file_path_1 = 'merged_output/merged_freiburg_100000p.json'
    # Frankfurt
    file_path_2 = 'merged_output/merged_frankfurt_100000p.json'

    json_data_1 = load_json(file_path_1)
    json_data_2 = load_json(file_path_2)

    compare_and_plot(json_data_1, json_data_2, "100000p chunks")

