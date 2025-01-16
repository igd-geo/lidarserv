import os.path
from os.path import join, dirname
import json
import matplotlib.pyplot as plt
import matplotlib as mpl
import numpy as np
from matplotlib.lines import Line2D
#from labellines import labelLine, labelLines
from mpl_toolkits.mplot3d import Axes3D
from mpl_toolkits import mplot3d
import matplotlib.cm as cm
from matplotlib.gridspec import SubplotSpec
from typing import List, Tuple
from scipy import stats
from math import ceil, floor
from pprint import pprint

PROJECT_ROOT = dirname(__file__)

def main():
    # plot style
    # plt.style.use("seaborn-notebook")

    # font magic to make the output pdf viewable in Evince, and probably other pdf viewers as well...
    # without this pdf rendering of pages with figures is extremely slow, especially when zooming in a lot and
    # regularly crashes the viewer...
    mpl.rcParams['pdf.fonttype'] = 42

    INSERTION_SPEED_COMPARISON = get_json_files(join(PROJECT_ROOT, "insertion_speed_comparison"))
    print(INSERTION_SPEED_COMPARISON)
    for file in INSERTION_SPEED_COMPARISON:
        with open(file, "r") as f:
            data = json.load(f)

        # ensure output folder exists
        output_folder = f"{file}.diagrams"
        os.makedirs(output_folder, exist_ok=True)

        plot_overall_performance_by_sizes(
            test_runs=data["runs"],
            filename=join(output_folder, "nodesize-performance_node.pdf"),
            nr_points=data["env"]["nr_points"],
        )

    file = join(PROJECT_ROOT, "overview_2024-11-14_1.json")
    with open(file, "r") as f:
        data = json.load(f)
        output_folder = f"{file}.diagrams"
        os.makedirs(output_folder, exist_ok=True)

        plot_insertion_rate_by_nr_threads(
            test_runs=data["runs"]["num_threads"],
            filename=join(output_folder, "insertion-rate-by-num-threads.pdf")
        )

        plot_insertion_rate_by_priority_function(
            test_runs=data["runs"]["priority_functions"],
            filename=join(output_folder, "insertion-rate-by-priority-function.pdf")
        )

        plot_insertion_rate_by_priority_function_bogus(
            test_runs=data["runs"]["bogus_points"],
            filename=join(output_folder, "insertion-rate-by-priority-function-bogus.pdf")
        )

        # get all runs where the name begins with "n1"
        n1_runs = {k: v for k, v in data["runs"].items() if k.startswith("n1")}
        plot_overall_performance_by_sizes(
            test_runs=n1_runs,
            filename=join(output_folder, "nodesize-performance_node.pdf"),
            nr_points=data["env"]["nr_points"],
        )

    # ahn queries
    files = ["article_measurements/lidarserv/ahn4_2025-01-07_1.json"]
    for file in files:
        path = join(PROJECT_ROOT, file)
        with open(path, "r") as f:
            data = json.load(f)
            output_folder = f"{path}.diagrams"
            os.makedirs(output_folder, exist_ok=True)

            print_querys(data)

            queries = [
                "classification_bridges",
                "classification_building",
                "classification_ground",
                "classification_vegetation",
                "intensity_high",
                "intensity_low",
                "time_1",
                "time_2",
                "time_3"
            ]

            labels = [
                "Classification\nBridges",
                "Classification\nBuildings",
                "Classification\nGround",
                "Classification\nVegetation",
                "Intensity\nHigh",
                "Intensity\nLow",
                "Time\nBig Slice",
                "Time\nSmall Slice",
                "Time\nMedium Slice"
            ]

            plot_query_by_time(
                data=data,
                queries=queries,
                labels=labels,
                filename=join(output_folder, "query-by-time-ahn4.pdf"),
                title="AHN4"
            )

            calculate_average_querying_speed(
                data=data,
                queries=queries,
                filename=join(output_folder, "average-querying-speed-ahn4.pdf"),
                title="AHN4"
            )

            plot_query_by_num_points(
                data=data,
                queries=queries,
                labels=labels,
                filename=join(output_folder, "query-by-points-ahn4.pdf"),
                title="AHN4"
            )

            plot_query_by_num_nodes(
                data=data,
                queries=queries,
                labels=labels,
                filename=join(output_folder, "query-by-nodes-ahn4.pdf"),
                title="AHN4"
            )

    # kitti queries
    files = ["article_measurements/lidarserv/kitti_2025-01-06_1.json"]
    for file in files:
        path = join(PROJECT_ROOT, file)
        with open(path, "r") as f:
            data = json.load(f)
            output_folder = f"{path}.diagrams"
            os.makedirs(output_folder, exist_ok=True)

            print_querys(data)

            queries = [
                "classification_building",
                "classification_ground",
                "pointsource1",
                "pointsource2",
                "rgb",
                "time1",
                "time2"
            ]

            labels = [
                "Classification\nBuildings",
                "Classification\nGround",
                "Pointsource1",
                "Pointsource2",
                "RGB",
                "Time\nBig Slice",
                "Time\nMedium Slice"
            ]

            plot_query_by_time(
                data=data,
                queries=queries,
                labels=labels,
                filename=join(output_folder, "query-by-time-kitti.pdf"),
                title="KITTI"
            )

            plot_query_by_num_points(
                data=data,
                queries=queries,
                labels=labels,
                filename=join(output_folder, "query-by-points-kitti.pdf"),
                title="KITTI"
            )

            plot_query_by_num_nodes(
                data=data,
                queries=queries,
                labels=labels,
                filename=join(output_folder, "query-by-nodes-kitti.pdf"),
                title="KITTI"
            )

            calculate_average_querying_speed(
                data=data,
                queries=queries,
                filename=join(output_folder, "average-querying-speed-kitti.pdf"),
                title="KITTI"
            )

    files = ["article_measurements/lidarserv/lille_2025-01-15_6.json"]
    for file in files:
        path = join(PROJECT_ROOT, file)
        with open(path, "r") as f:
            data = json.load(f)
            output_folder = f"{path}.diagrams"
            os.makedirs(output_folder, exist_ok=True)

            print_querys(data)

            queries = [
                "intensity_high",
                "intensity_low",
                "time_1",
                "time_2",
                "pointsource1",
                "pointsource2",
                "scananglerank1",
                "scananglerank2",
                "view_frustum1",
            ]

            labels = [
                "Intensity\nHigh",
                "Intensity\nLow",
                "Time\nBig Slice",
                "Time\nSmall Slice",
                "PointsourceID\n>=10",
                "PointsourceID\n>=5",
                "ScanAngleRank\n<=45°",
                "ScanAngleRank\n<=90°",
                "View Frustum",
            ]

            plot_query_by_time(
                data=data,
                queries=queries,
                labels=labels,
                filename=join(output_folder, "query-by-time-lille.pdf"),
                title="Lille",
            )

            plot_query_by_num_points(
                data=data,
                queries=queries,
                labels=labels,
                filename=join(output_folder, "query-by-points-lille.pdf"),
                title="Lille"
            )

            plot_query_by_num_nodes(
                data=data,
                queries=queries,
                labels=labels,
                filename=join(output_folder, "query-by-nodes-lille.pdf"),
                title="Lille"
            )

            calculate_average_querying_speed(
                data=data,
                queries=queries,
                filename=join(output_folder, "average-querying-speed-lille.pdf"),
                title="Lille"
            )

    path = join(PROJECT_ROOT, "article_measurements/insertion_speeds.json")
    with open(path, "r") as f:
        data = json.load(f)
        output_folder = f"{path}.diagrams"
        os.makedirs(output_folder, exist_ok=True)

        plot_insertion_speed_comparison(data, output_folder)

    path = join(PROJECT_ROOT, "article_measurements/index_sizes.json")
    with open(path, "r") as f:
        data = json.load(f)
        output_folder = f"{path}.diagrams"
        os.makedirs(output_folder, exist_ok=True)
        plot_index_size_comparison(data, output_folder)
        plot_compression_rate_comparison(data, output_folder)

    files = "article_measurements/lidarserv/kitti_2024-12-26_1.json", "article_measurements/lidarserv/lille_2024-12-26_3.json"
    for path in files:
        with open(path, "r") as f:
            data = json.load(f)
            output_folder = f"{path}.diagrams"
            os.makedirs(output_folder, exist_ok=True)

            plot_latency_comparison_violin(data, output_folder, query="full-point-cloud")

    # files = "article_measurements/lidarserv/lille_2025-01-15_6.json",
    # for path in files:
    #     with open(path, "r") as f:
    #         data = json.load(f)
    #         output_folder = f"{path}.diagrams"
    #         os.makedirs(output_folder, exist_ok=True)
    #
    #         plot_latency_comparison_violin(data, output_folder, query="view_frustum1")


def print_querys(data):
    points_file = data["settings"]["points_file"].split("/")[-1]
    total_points = data["env"]["nr_points"]
    queries = data["runs"]["main"][0]["results"]["query_performance"]
    query_statements = data["settings"]["queries"]

    for query in queries:
        query_statement = query_statements[query]
        try:
            nr_points = queries[query]["nr_points"]
            selectivity = nr_points / total_points * 100
            print(f"{points_file} & ${query_statement}$ & {selectivity:.2f}\% & {nr_points:.0f} \\\\ \hline")
        except:
            continue

def estimate_probability_density(
        quantiles: List[Tuple[int, float]]
) -> Tuple[List[float], List[float]]:
    """
    draws a smooth violin (in contrast to estimate_probability_density_raw)

     - adjust `nr_buckets` for more/less details / smoothness
     - adjust `nr_coords` for sampling frequency
    """
    value_min = min(r for l,r in quantiles)
    value_max = max(r for l,r in quantiles)

    nr_buckets = 100
    bucket_size = (value_max - value_min) / nr_buckets
    buckets = [0.0] * nr_buckets
    for (l1, r1), (l2, r2) in zip(quantiles[:-1], quantiles[1:]):
        for bucket in range(nr_buckets):
            bucket_min = value_min + bucket * bucket_size
            bucket_max = value_min + (bucket + 1) * bucket_size
            if r1 < bucket_max and r2 > bucket_min:
                factor = (min(bucket_max, r2) - max(bucket_min, r1)) / (r2 - r1)
                buckets[bucket] += (l2 - l1) / 100 * factor

    nr_coords = 500
    coords = [
        c / (nr_coords - 1) * (value_max - value_min) + value_min 
        for c in range(nr_coords)
    ]
    vals = [0.0] * nr_coords
    for bucket, probability_mass in enumerate(buckets):
        bucket_center = value_min + (bucket + 0.5) * bucket_size
        for i in range(nr_coords):
            coord = coords[i]
            vals[i] += stats.norm.pdf(coord, bucket_center, bucket_size) * probability_mass

    # calculate mean
    mean = sum([coord * val for coord, val in zip(coords, vals)]) / sum(vals)

    return {
            "mean": mean,
            "median": [r for l,r in quantiles if l==50][0],
            "min": value_min,
            "max": value_max,

            # y-position (latency values)
            "coords": coords,

            # violin width (probability density) at the y position given in "coords"
            "vals": vals,

    }

def estimate_probability_density_raw(
        quantiles: List[Tuple[int, float]]
) -> Tuple[List[float], List[float]]:
    """
    draws the violins from the "raw" quantiles without gaussian smoothing

     - adjust nr_buckets for sampling frequency
    """
    value_min = min(r for l,r in quantiles)
    value_max = max(r for l,r in quantiles)

    nr_buckets = 500
    bucket_size = (value_max - value_min) / nr_buckets
    buckets = [0.0] * nr_buckets
    for (l1, r1), (l2, r2) in zip(quantiles[:-1], quantiles[1:]):
        for bucket in range(nr_buckets):
            bucket_min = value_min + bucket * bucket_size
            bucket_max = value_min + (bucket + 1) * bucket_size
            if r1 < bucket_max and r2 > bucket_min:
                factor = (min(bucket_max, r2) - max(bucket_min, r1)) / (r2 - r1)
                buckets[bucket] += (l2 - l1) / 100 * factor

    coords = [
        (c + 0.5) / nr_buckets * (value_max - value_min) + value_min 
        for c in range(nr_buckets)
    ]
    return {
            "mean": 0.1,    # todo don't hardcode - use actual mean
            "median": [r for l,r in quantiles if l==50][0],
            "min": value_min,
            "max": value_max,

            # y-position (latency values)
            "coords": coords,

            # violin width (probability density) at the y position given in "coords"
            "vals": buckets,

    }


def plot_latency_comparison_violin(data, output_folder, query):
    latency_query = data["runs"]["main"][0]["results"]["latency"][query]
    stats_by_lod = latency_query["stats_by_lod"]
    stats_full = latency_query["stats"]
    lods = list(stats_by_lod.keys())

    # Prepare data for plotting
    quantiles = {query: [] for query in lods}

    # add full point cloud stats
    quantiles["full-point-cloud"] = [(percentile[0], percentile[1] * 1000) for percentile in stats_full["percentiles"]]

    # add lod stats
    for lod in lods:
        percentiles = stats_by_lod[lod]["percentiles"]
        percentiles = [(percentile[0], percentile[1] * 1000) for percentile in percentiles]
        for percentile in percentiles:
            quantiles[lod].append(percentile)

    # remove queries with no data
    lods = [lod for lod in lods if len(quantiles[lod]) > 0]

    xs = range(len(quantiles))
    vpstats = [estimate_probability_density(quantiles[key]) for key in quantiles.keys()]

    fig, ax = plt.subplots(figsize=[10, 6])
    violin_parts = ax.violin(vpstats, xs, widths=0.8, showmedians=True)

    # Color the full-point-cloud red
    for i, lod in enumerate(quantiles.keys()):
        if lod == "full-point-cloud":
            violin_parts['bodies'][i].set_facecolor('red')
            violin_parts['bodies'][i].set_edgecolor('red')

    # for i, vpstat in enumerate(vpstats):
    #     ax.text(i, vpstat["median"], f'{vpstat["median"]:.2f}', ha='center', va='bottom')

    lods.append("All LODs")
    plt.xticks(xs, lods)
    plt.ylabel("Latency | ms")
    plt.tight_layout()
    plt.savefig(join(output_folder, "latency_comparison_violin.pdf"))


def plot_compression_rate_comparison(data, output_folder):
    fig, axs = plt.subplots(1, 3, figsize=(15, 5))

    dataset_names = {
        "ahn4": "AHN4",
        "kitti": "KITTI",
        "lille": "Lille",
    }

    tool_names = {
        "lidarserv": "Lidarserv",
        "potree_converter": "PotreeConverter",
        "pgpointcloud": "PgPointCloud"
    }

    for i, (dataset, dataset_data) in enumerate(data.items()):
        for tool, tool_data in dataset_data.items():
            if tool == 'input_size':
                continue
            # calculate compression rate
            uncompressed = dataset_data[tool]['uncompressed']
            compressed = dataset_data[tool]['compressed']
            compression_rate = (1 - compressed / uncompressed) * 100
            dataset_data[tool]['compression_rate'] = compression_rate

    for i, (dataset, dataset_data) in enumerate(data.items()):
        ax = axs[i]
        tools = dataset_data.keys()
        tools = [tool for tool in tools if tool != 'input_size']
        compression_rates = [dataset_data[tool]['compression_rate'] for tool in tools]

        x = np.arange(len(compression_rates))

        bars = ax.bar(x, compression_rates)

        tools = [tool_names[tool] for tool in tools]
        ax.set_title(dataset_names[dataset])
        ax.set_xticks(x)
        ax.set_xticklabels(tools)

        for bar, rate in zip(bars, compression_rates):
            ax.text(bar.get_x() + bar.get_width() / 2, bar.get_height(), f'{rate:.2f}%', ha='center', va='bottom')

        axs[0].set_ylabel("Compression Rate | %")
        handles, labels = axs[0].get_legend_handles_labels()
        fig.legend(handles, labels, loc='lower center', bbox_to_anchor=(0.5, -0.05), ncol=3)

        plt.tight_layout()
        plt.savefig(join(output_folder, "compression_rate_comparison.pdf"))

def plot_index_size_comparison(data, output_folder):
    fig, axs = plt.subplots(1, 3, figsize=(12, 4))

    dataset_names = {
        "ahn4": "AHN4",
        "kitti": "KITTI",
        "lille": "Lille",
    }

    tool_names = {
        "lidarserv": "Lidarserv",
        "potree_converter": "PotreeConverter",
        "pgpointcloud": "PgPointCloud"
    }

    for i, (dataset, dataset_data) in enumerate(data.items()):
        ax = axs[i]
        tools = dataset_data.keys()
        tools = [tool for tool in tools if tool != 'input_size']
        uncompressed = [dataset_data[tool]['uncompressed'] for tool in tools]
        compressed = [dataset_data[tool]['compressed'] for tool in tools]

        x = np.arange(len(uncompressed))
        width = 0.35

        ax.bar(x - width / 2, uncompressed, width, label='Uncompressed', color='#F4B400')
        ax.bar(x + width / 2, compressed, width, label='Compressed', color='#4285F4')

        # Add horizontal line for input size
        input_size = dataset_data['input_size']
        ax.axhline(y=input_size, color='r', linestyle='--', label='Input Size')

        tools = [tool_names[tool] for tool in tools]
        ax.set_title(dataset_names[dataset])
        ax.set_xticks(x)
        ax.set_xticklabels(tools)

        ax.yaxis.set_major_formatter(plt.FuncFormatter(lambda y, _: f"{y:.0f}GB"))
        

    axs[0].set_ylabel("Size of Index | Gigabytes")

    plt.legend()
    plt.tight_layout()
    plt.savefig(join(output_folder, "index_size_comparison.pdf"))

def plot_insertion_speed_comparison(data, output_folder):
    dataset_names = {
        "ahn4": "AHN4",
        "kitti": "KITTI",
        "lille": "Lille",
    }

    tool_names = {
        "lidarserv": "Lidarserv",
        "potree_converter": "PotreeConverter",
        "pgpointcloud": "PgPointCloud"
    }

    fig, axs = plt.subplots(1, 3, figsize=(12, 4))
    max_y_value = 0  # To track the maximum y-value across all datasets

    # First pass to determine the maximum y-value
    for dataset, dataset_data in data.items():
        num_points = dataset_data['num_points']
        tools = dataset_data.keys()
        tools = [tool for tool in tools if tool != 'num_points' and tool != 'scanner_speed']
        uncompressed = [num_points / dataset_data[tool]['uncompressed'] for tool in tools]
        compressed = [num_points / dataset_data[tool]['compressed'] for tool in tools]

        # Update the maximum y-value
        max_y_value = max(max_y_value, max(uncompressed + compressed)) * 1.01

    # Second pass to create the plots with a consistent y-scale
    for i, (dataset, dataset_data) in enumerate(data.items()):
        num_points = dataset_data['num_points']
        ax = axs[i]
        tools = dataset_data.keys()
        tools = [tool for tool in tools if tool != 'num_points' and tool != 'scanner_speed']
        uncompressed = [num_points / dataset_data[tool]['uncompressed'] for tool in tools]
        compressed = [num_points / dataset_data[tool]['compressed'] for tool in tools]

        x = np.arange(len(uncompressed))
        width = 0.35

        ax.bar(x - width / 2, uncompressed, width, label='Uncompressed', color='#F4B400')
        ax.bar(x + width / 2, compressed, width, label='Compressed', color='#4285F4')

        tools = [tool_names[tool] for tool in tools]
        ax.set_title(dataset_names[dataset])
        ax.set_xticks(x)
        ax.set_xticklabels(tools)
        ax.set_ylim(0, max_y_value)  # Set the y-axis limit

        # Format y-axis labels with 'M' for millions
        ax.yaxis.set_major_formatter(plt.FuncFormatter(lambda y, _: f"{y / 1e6:.1f}M"))

        try:
            ax.axhline(y=dataset_data["scanner_speed"], color='r', linestyle='--', label='Scanner Speed')
        except:
            continue

    # Add y-axis label to the leftmost plot
    axs[0].set_ylabel("Insertion Rate | Points/s")

    plt.legend()
    plt.tight_layout()
    plt.savefig(join(output_folder, "insertion_speed_comparison.pdf"))



def get_json_files(directory):
    json_files = []
    for root, dirs, files in os.walk(directory):
        for file in files:
            if file.endswith(".json"):
                json_files.append(os.path.join(root, file))
    return json_files

def get_timeout_runs(test_runs, nr_points):
    timeouted = []
    for name, run in test_runs.items():
        nr_points_run = run["results"]["insertion_rate"]["nr_points"]
        if nr_points_run != nr_points:
            timeouted.append(True)
        else:
                timeouted.append(False)
    return timeouted

def make_y_insertion_rate(ax, test_runs):
    ys = [i["results"]["insertion_rate"]["insertion_rate_points_per_sec"] for i in test_runs]
    ax.set_ylabel("Insertion rate | Points/s")
    ymax = max(ys) * 1.1
    bottom, top = ax.get_ylim()
    if bottom != 0 or top < ymax:
        ax.set_ylim(bottom=0, top=ymax)
    return ys


def make_y_duration_cleanup(ax, test_runs):
    ys = [i["results"]["insertion_rate"]["duration_cleanup_seconds"] for i in test_runs]
    ax.set_ylabel("Cleanup time | Points/s")
    return ys


def make_x_nr_threads(ax, test_runs):
    ax.set_xlabel("Number of threads")
    xs = [int(i["index"]["num_threads"]) for i in test_runs]
    ax.set_xlim(left=0, right=max(xs) + 1.0)
    return xs


def make_x_cache_size(ax, test_runs):
    ax.set_xlabel("Cache Size | nr pages")
    # ax.set_xscale("log")
    return [int(i["index"]["cache_size"]) for i in test_runs]


def make_x_nr_bogus_points(ax, test_runs):
    ax.set_xlabel("Bogus points | max nr bogus points per node")
    return [int(i["index"]["nr_bogus_points"][0]) for i in test_runs]


def make_x_priority_function(ax, test_runs):
    labels = [rename_tpf(i["index"]["priority_function"]) for i in test_runs]
    xs = list(range(len(labels)))
    ax.set_xticks(xs, labels)
    ax.set_xticklabels(labels, rotation=90)
    ax.set_xlim(left=-.5, right=len(labels) - .5)
    return xs


def plot_insertion_rate_by_nr_threads(test_runs, filename, title=None):
    fig: plt.Figure = plt.figure()
    ax: plt.Axes = fig.subplots()
    compression = [i["index"]["compression"] for i in test_runs]
    colors = ['red' if c else 'blue' for c in compression]
    xs = make_x_nr_threads(ax, test_runs)
    ys = make_y_insertion_rate(ax, test_runs)
    ax.scatter(xs, ys, c=colors)
    ax.legend(handles=[Line2D([0], [0], marker='o', color='w', markerfacecolor='red', markersize=10, label='Compression'),
                       Line2D([0], [0], marker='o', color='w', markerfacecolor='blue', markersize=10, label='No Compression')])

    if title is not None:
        ax.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
    plt.close(fig)


def plot_insertion_rate_by_cache_size(test_runs, filename, title=None):
    fig: plt.Figure = plt.figure()
    ax: plt.Axes = fig.subplots()
    xs = make_x_cache_size(ax, test_runs)
    ys = make_y_insertion_rate(ax, test_runs)
    ax.plot(xs, ys, marker=".")
    if title is not None:
        ax.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
    plt.close(fig)


def plot_insertion_rate_by_priority_function(test_runs, filename, title=None):
    fig: plt.Figure = plt.figure()
    ax: plt.Axes = fig.subplots()
    xs = make_x_priority_function(ax, test_runs)
    ys = make_y_insertion_rate(ax, test_runs)
    ax.bar(xs, ys, 0.7)
    if title is not None:
        ax.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
    plt.close(fig)


def plot_insertion_rate_by_priority_function_bogus(test_runs, filename, title=None):
    fig: plt.Figure = plt.figure()
    ax: plt.Axes = fig.subplots()
    prio_fns = sorted(set(rename_tpf(i["index"]["priority_function"]) for i in test_runs))
    for prio_fn in prio_fns:
        this_runs = [t for t in test_runs if rename_tpf(t["index"]["priority_function"]) == prio_fn]
        xs = make_x_nr_bogus_points(ax, this_runs)
        ys = make_y_insertion_rate(ax, this_runs)
        ax.plot(xs, ys, label=prio_fn, marker=".")
    ax.legend()
    if title is not None:
        ax.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
    plt.close(fig)


def plot_insertion_rate_by_priority_function_cache(test_runs, filename, title=None):
    fig: plt.Figure = plt.figure()
    ax: plt.Axes = fig.subplots()
    prio_fns = sorted(set(rename_tpf(i["index"]["priority_function"]) for i in test_runs))
    for prio_fn in prio_fns:
        this_runs = [t for t in test_runs if rename_tpf(t["index"]["priority_function"]) == prio_fn]
        xs = make_x_cache_size(ax, this_runs)
        ys = make_y_insertion_rate(ax, this_runs)
        ax.plot(xs, ys, label=prio_fn, marker=".")
    ax.legend()
    if title is not None:
        ax.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
    plt.close(fig)


def plot_duration_cleanup_by_priority_function_bogus(test_runs, filename, title=None):
    fig: plt.Figure = plt.figure()
    ax: plt.Axes = fig.subplots()
    prio_fns = sorted(set(rename_tpf(i["index"]["priority_function"]) for i in test_runs))
    for prio_fn in prio_fns:
        this_runs = [t for t in test_runs if rename_tpf(t["index"]["priority_function"]) == prio_fn]
        xs = make_x_nr_bogus_points(ax, this_runs)
        ys = make_y_duration_cleanup(ax, this_runs)
        ax.plot(xs, ys, label=prio_fn, marker=".")
    ax.legend()
    if title is not None:
        ax.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
    plt.close(fig)

def calculate_average_querying_speed(data, queries, filename, title=None):
    test_runs = data["runs"]["main"]

    times = {
        "no_compression": [],
        "compression": [],
        "no_compression_attribute_index": [],
        "compression_attribute_index": []
    }

    times_no_compression = []
    times_compression = []
    num_points_no_compression = []
    num_points_compression = []

    for test_run in test_runs:
        compression = test_run["index"]["compression"]
        attribute_index = test_run["index"]["enable_attribute_index"]
        try:
            point_filtering = test_run["index"]["enable_point_filtering"]
        except:
            point_filtering = True

        for query in queries:
            query_time = test_run["results"]["query_performance"][query]["query_time_seconds"]
            num_points = test_run["results"]["query_performance"][query]["nr_points"]
            if compression and attribute_index and point_filtering:
                times_compression.append(query_time)
                num_points_compression.append(num_points)
            elif (not compression) and attribute_index and point_filtering:
                times_no_compression.append(query_time)
                num_points_no_compression.append(num_points)

    pps_no_compression = sum(num_points_no_compression) / sum(times_no_compression)
    pps_compression = sum(num_points_compression) / sum(times_compression)
    print("Dataset: ", title)
    print(f"Average querying speed without compression: {pps_no_compression:.2f} points/s")
    print(f"Average querying speed with compression: {pps_compression:.2f} points/s")



def plot_query_by_time(data, filename, queries, labels, title=None):
    test_runs = data["runs"]["main"]

    times = {
        "no_compression": [],
        "compression": [],
        "no_compression_attribute_index": [],
        "compression_attribute_index": []
    }

    for test_run in test_runs:
        compression = test_run["index"]["compression"]
        attribute_index = test_run["index"]["enable_attribute_index"]
        point_filtering = test_run["index"]["enable_point_filtering"]

        for query in queries:
            if not point_filtering:
                continue
            query_time = test_run["results"]["query_performance"][query]["query_time_seconds"]
            if compression and attribute_index:
                times["compression_attribute_index"].append(query_time)
            elif compression:
                times["compression"].append(query_time)
            elif attribute_index:
                times["no_compression_attribute_index"].append(query_time)
            else:
                times["no_compression"].append(query_time)

    fig, ax = plt.subplots(figsize=[10, 6])
    index = range(len(queries))

    colors = ['#DB4437', '#F4B400', '#0F9D58', '#4285F4', '#bb0089']
    bar_width = 0.3

    for p in range(len(queries)):
        plt.bar([p + bar_width * 0.5], times["no_compression"][p], bar_width, color=colors[0])
        plt.bar([p + bar_width * 1.5], times["no_compression_attribute_index"][p], bar_width, color=colors[1])

    plt.ylabel('Execution Time | Seconds')
    plt.xticks([p + bar_width * 1 for p in index], labels, rotation=90)

    custom_legend_labels = [
        'No Attribute Index',
        'Attribute Index'
    ]
    custom_legend_colors = colors[:len(custom_legend_labels)]
    custom_legend_handles = [Line2D([0], [0], color=color, label=label, linewidth=8) for color, label in
                                zip(custom_legend_colors, custom_legend_labels)]
    ax.legend(handles=custom_legend_handles, loc='upper left', bbox_to_anchor=(0, -0.4), title='Subqueries')

    plt.tight_layout()

    if title is not None:
        ax.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
    plt.close(fig)


def plot_query_by_num_points(data, filename, queries, labels, title=None):
    test_runs = data["runs"]["main"]

    points = {
        "point_filtering_attribute_index": [],
        "no_point_filtering_attribute_index": [],
        "point_filtering_no_attribute_index": [],
        "no_point_filtering_no_attribute_index": []
    }

    for test_run in test_runs:
        point_filtering = test_run["index"]["enable_point_filtering"]
        attribute_index = test_run["index"]["enable_attribute_index"]

        for query in queries:
            num_points = test_run["results"]["query_performance"][query]["nr_points"]
            if point_filtering and attribute_index:
                points["point_filtering_attribute_index"].append(num_points)
            elif point_filtering and not attribute_index:
                points["point_filtering_no_attribute_index"].append(num_points)
            elif not point_filtering and attribute_index:
                points["no_point_filtering_attribute_index"].append(num_points)
            else:
                points["no_point_filtering_no_attribute_index"].append(num_points)

    fig, ax = plt.subplots(figsize=[10, 6])
    index = range(len(queries))

    colors = ['#DB4437', '#F4B400', '#0F9D58', '#4285F4', '#bb0089']
    bar_width = 0.3

    for p in range(len(queries)):
        div = 1000000
        plt.bar([p + bar_width * 0.0], points["no_point_filtering_no_attribute_index"][p]/div, bar_width, color=colors[0])
        plt.bar([p + bar_width * 1.0], points["no_point_filtering_attribute_index"][p]/div, bar_width, color=colors[1])
        plt.bar([p + bar_width * 2.0], points["point_filtering_attribute_index"][p]/div, bar_width, color=colors[2])
    
    plt.ylabel('Number of Points')
    plt.xticks([p + bar_width * 1 for p in index], labels, rotation=90)
    ax.yaxis.set_major_formatter(plt.FuncFormatter(lambda y, _: f"{y:.0f}M"))

    custom_legend_labels = [
        'Spatial Index',
        'Spatial Index + Attribute Index',
        'Spatial Index + Attribute Index + Point Filtering'
    ]
    custom_legend_colors = colors[:len(custom_legend_labels)]
    custom_legend_handles = [Line2D([0], [0], color=color, label=label, linewidth=8) for color, label in
                                zip(custom_legend_colors, custom_legend_labels)]
    ax.legend(handles=custom_legend_handles, loc='upper left', bbox_to_anchor=(0, -0.4), title='Subqueries')

    plt.tight_layout()

    if title is not None:
        ax.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
    plt.close(fig)


def plot_query_by_num_nodes(data, filename, queries, labels, title=None):
    test_runs = data["runs"]["main"]

    nodes = {
        "point_filtering_attribute_index": [],
        "no_point_filtering_attribute_index": [],
        "point_filtering_no_attribute_index": [],
        "no_point_filtering_no_attribute_index": []
    }

    for test_run in test_runs:
        point_filtering = test_run["index"]["enable_point_filtering"]
        attribute_index = test_run["index"]["enable_attribute_index"]

        for query in queries:
            num_nodes = test_run["results"]["query_performance"][query]["nr_non_empty_nodes"]
            if point_filtering and attribute_index:
                nodes["point_filtering_attribute_index"].append(num_nodes)
            elif point_filtering and not attribute_index:
                nodes["point_filtering_no_attribute_index"].append(num_nodes)
            elif not point_filtering and attribute_index:
                nodes["no_point_filtering_attribute_index"].append(num_nodes)
            else:
                nodes["no_point_filtering_no_attribute_index"].append(num_nodes)

    fig, ax = plt.subplots(figsize=[10, 6])
    index = range(len(queries))

    colors = ['#DB4437', '#F4B400', '#0F9D58', '#4285F4', '#bb0089']
    bar_width = 0.3

    for p in range(len(queries)):
        div = 1000
        plt.bar([p + bar_width * 0.5], nodes["no_point_filtering_no_attribute_index"][p]/div, bar_width, color=colors[0])
        plt.bar([p + bar_width * 1.5], nodes["no_point_filtering_attribute_index"][p]/div, bar_width, color=colors[1])
    
    plt.ylabel('Number of Nodes')
    plt.xticks([p + bar_width * 1 for p in index], labels, rotation=90)
    ax.yaxis.set_major_formatter(plt.FuncFormatter(lambda y, _: f"{y:.0f}K"))

    custom_legend_labels = [
        'Spatial Index',
        'Spatial Index + Attribute Index',
    ]
    custom_legend_colors = colors[:len(custom_legend_labels)]
    custom_legend_handles = [Line2D([0], [0], color=color, label=label, linewidth=8) for color, label in
                                zip(custom_legend_colors, custom_legend_labels)]
    ax.legend(handles=custom_legend_handles, loc='upper left', bbox_to_anchor=(0, -0.4), title='Subqueries')

    plt.tight_layout()

    if title is not None:
        ax.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
    plt.close(fig)


# Plots the number of nodes per level for each run
def plot_query_lod_nodes_by_runs(test_runs, filename, title=None):
    fig, ax = plt.subplots(figsize=[20, 12])

    # preprocessing data (get all lod lists from all runs)
    lod_lists = []
    for name, run in test_runs.items():
        for multi_run in run:
            node_hierarchy = multi_run["index"]["node_hierarchy"]
            point_hierarchy = multi_run["index"]["point_hierarchy"]
            run_name = "N" + str(node_hierarchy) + "P" + str(point_hierarchy)
            lod_lists.append((run_name, multi_run["results"]["index_info"]["directory_info"]["num_nodes_per_level"]))

    x = list(range(len(lod_lists[0][1])))  # Assuming both sublists have the same length

    # plot data
    for lod_list in lod_lists:
        plt.plot(x, lod_list[1], label=lod_list[0])
    plt.grid(True)
    #labelLines(ax.get_lines(), zorder=2.5)
    plt.xlabel('LOD')
    plt.ylabel('Number of Nodes')
    # plt.title('Number of Nodes per LOD')

    if title is not None:
        ax.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
    plt.close(fig)


# Plots Insertion Speed, Query Time Speedup and Query Point Reduction according to the node and point hierarchy
# IMPORTANT: Always use the same number of points for all runs (no timeout)
# Else the query time speedup is not comparable (rest is probably fine)
def plot_overall_performance_by_bogus(test_runs, filename, nr_points, title=None, insertion_color_threshold=150000,
                                      query_run="only_node_acc"):
    fig, ax1 = plt.subplots(figsize=[10, 6])
    ax2 = ax1.twinx()  # Create a twin Axes sharing the xaxis

    names = []
    sizes_of_roots = []
    insertion_speeds = []
    query_speeds = []
    point_reductions = []
    timeouted = []

    for name, run in test_runs.items():
        for multi_run in run:
            # name calculation
            bogus_points = multi_run["index"]["nr_bogus_points"]
            run_name = str(bogus_points[0])
            names.append(run_name)

            # data calculation
            sizes_of_roots.append(multi_run["results"]["index_info"]["root_cell_size"][0])
            insertion_speeds.append(multi_run["results"]["insertion_rate"]["insertion_rate_points_per_sec"])
            query_speeds.append(calculate_average_query_time_single_run(multi_run))
            point_reductions.append(calculate_average_point_reduction_single_run(multi_run, query_run))

            # check if timeouted
            nr_points_run = multi_run["results"]["insertion_rate"]["nr_points"]
            if nr_points_run != nr_points:
                timeouted.append(True)
            else:
                timeouted.append(False)

    # Convert timeouted to color list
    # Also check if insertion speed is below threshold and color it orange
    colors = []
    for i in range(len(names)):
        if timeouted[i]:
            colors.append("red")
        elif insertion_speeds[i] < insertion_color_threshold:
            colors.append("orange")
        else:
            colors.append("green")

    # Plotting logic for Insertion Speed
    # plot insertion speed as bar plot with different colors for timeouted runs
    # ax1.bar(names, insertion_speeds, label='Insertion Speed (Points per Second)', color=colors)
    ax1.plot(names, insertion_speeds, marker='o', label='Insertion Rate | Points/s', color='tab:blue')
    ax1.set_xlabel('Number of Bogus Points')
    ax1.set_ylabel('Insertion Rate (Points/s)', color='tab:blue')

    # Plotting logic for Query Speedup
    ax2.plot(names, query_speeds, marker='o', label='Average Query Time | Seconds', color='tab:green')
    ax2.set_ylabel('Average Query Time | Seconds', color='tab:green')

    # Creating a third y-axis for Point Reduction
    ax3 = ax1.twinx()
    ax3.spines['right'].set_position(('outward', 60))  # Adjust the position of the third y-axis

    ax3.plot(names, point_reductions, marker='x', label='Average Point Reduction | Percentage', color='tab:red')
    ax3.set_ylabel('Average Point Reduction | Percentage', color='tab:red')

    # plt.axhline(y = 400000, color = 'b', linestyle = '-')
    ax1.axhline(y=400000, color='grey', linestyle=':', label='Insertion Rate Goal (400000 Points/s)')

    # Combine legends from all axes
    lines, labels = ax1.get_legend_handles_labels()
    lines2, labels2 = ax2.get_legend_handles_labels()
    lines3, labels3 = ax3.get_legend_handles_labels()

    ax3.legend(lines + lines2 + lines3, labels + labels2 + labels3, loc='upper center', bbox_to_anchor=(0.5, -0.1))
    # legend position below diagram:
    # ax3.legend(lines + lines2 + lines3, labels + labels2 + labels3, loc='upper center', bbox_to_anchor=(0.5, -0.2))

    ax1.tick_params(axis='y', labelcolor='tab:blue')
    ax2.tick_params(axis='y', labelcolor='tab:green')
    ax3.tick_params(axis='y', labelcolor='tab:red')

    plt.xticks(rotation=45, ha='right')
    # plt.title(title if title else 'Overall Performance by Run')
    plt.tight_layout()

    if title is not None:
        ax1.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
    plt.close(fig)


# Plots Insertion Speed, Query Time Speedup and Query Point Reduction according to the node and point hierarchy
# IMPORTANT: Always use the same number of points for all runs (no timeout)
# Else the query time speedup is not comparable (rest is probably fine)
def plot_overall_performance_by_sizes(
        test_runs,
        filename,
        nr_points,
        title=None,
        insertion_color_threshold=150000,
        plot_point_reduction=False,
        query_run="only_node_acc"):
    fig, ax1 = plt.subplots(figsize=[10, 6])
    ax2 = ax1.twinx()  # Create a twin Axes sharing the xaxis

    names = []
    node_values = []
    insertion_speeds = []
    query_speeds = []
    point_reductions = []
    timeouted = []

    for name, run in test_runs.items():
        for multi_run in run:
            # name calculation
            node_hierarchy = multi_run["index"]["node_hierarchy"]
            point_hierarchy = multi_run["index"]["point_hierarchy"]
            run_name = "N" + str(node_hierarchy) + "P" + str(point_hierarchy)
            names.append(run_name)
            node_values.append(node_hierarchy)

            # data calculation
            insertion_speeds.append(multi_run["results"]["insertion_rate"]["insertion_rate_points_per_sec"])
            query_speeds.append(calculate_average_query_time_single_run(multi_run))
            if plot_point_reduction:
                point_reductions.append(calculate_average_point_reduction_single_run(multi_run, query_run))

            # check if timeouted
            nr_points_run = multi_run["results"]["insertion_rate"]["nr_points"]
            if nr_points_run != nr_points:
                timeouted.append(True)
            else:
                timeouted.append(False)

    # Convert timeouted to color list
    # Also check if insertion speed is below threshold and color it orange
    colors = []
    for i in range(len(names)):
        if timeouted[i]:
            colors.append("red")
        elif insertion_speeds[i] < insertion_color_threshold:
            colors.append("orange")
        else:
            colors.append("green")

    # flip all lists
    # names = names[::-1]
    # insertion_speeds = insertion_speeds[::-1]
    # query_speeds = query_speeds[::-1]
    # point_reductions = point_reductions[::-1]
    # colors = colors[::-1]

    # Plotting logic for Insertion Speed
    # plot insertion speed as bar plot with different colors for timeouted runs
    # ax1.bar(names, insertion_speeds, label='Insertion Rate | points/s', color=colors)
    ax1.plot(names, insertion_speeds, marker='o', label='Insertion Rate | Points/s', color='tab:blue')
    ax1.set_xticklabels(names, rotation=90)
    ax1.set_xlabel('Node Sizes')
    ax1.set_ylabel('Insertion Rate | Points/s', color='tab:blue')

    # Plotting logic for Query Speedup
    ax2.plot(names, query_speeds, marker='o', label='Average Query Time | Seconds', color='tab:green')
    ax2.set_ylabel('Average Query Time | Seconds', color='tab:green')

    # Creating a third y-axis for Point Reduction
    ax3 = ax1.twinx()
    ax3.spines['right'].set_position(('outward', 60))  # Adjust the position of the third y-axis

    if plot_point_reduction:
        ax3.plot(names, point_reductions, marker='x', label='Average Point Reduction | Percentage', color='tab:red')
        ax3.set_ylabel('Average Point Reduction | Percentage', color='tab:red')

    # ax1.axhline(y=400000, color='grey', linestyle=':', label='Insertion Rate Goal (400000 Points/s)')

    # Combine legends from all axes
    lines, labels = ax1.get_legend_handles_labels()
    lines2, labels2 = ax2.get_legend_handles_labels()
    if plot_point_reduction:
        lines3, labels3 = ax3.get_legend_handles_labels()
        ax3.legend(lines + lines2 + lines3, labels + labels2 + labels3, loc='upper center', bbox_to_anchor=(0.5, -0.1))
    # legend position below diagram:
    # ax3.legend(lines + lines2 + lines3, labels + labels2 + labels3, loc='upper center', bbox_to_anchor=(0.5, -0.2))

    ax1.tick_params(axis='y', labelcolor='tab:blue')
    ax2.tick_params(axis='y', labelcolor='tab:green')
    if plot_point_reduction:
        ax3.tick_params(axis='y', labelcolor='tab:red')

    plt.xticks(rotation=90)
    # plt.title(title if title else 'Overall Performance by Run')
    plt.tight_layout()

    if title is not None:
        ax1.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
    plt.close(fig)


# Plots overall performance by node and point hierarchy sizes (3D)
# IMPORTANT: Always use the same number of points for all runs (no timeout)
# Else the query time speedup is not comparable (rest is probably fine)
def plot_overall_performance_by_sizes_3d(data, filename, nr_points, title=None, query_run="only_node_acc"):
    # Create a 3D plot
    num_runs = len(data.keys())
    height = num_runs * 10
    fig, axs = plt.subplots(num_runs, 2, figsize=(20, height))

    # calculate global min and max for all runs
    min_insertion_speed = 1000000
    max_insertion_speed = 0
    min_point_reduction = 1000000
    max_point_reduction = 0
    for key in data.keys():
        run = data[key]
        for multi_run in run:
            # data calculation
            insertion_speed = multi_run["results"]["insertion_rate"]["insertion_rate_points_per_sec"]
            point_reduction = calculate_average_point_reduction_single_run(multi_run)

            if insertion_speed < min_insertion_speed:
                min_insertion_speed = insertion_speed

            if insertion_speed > max_insertion_speed:
                max_insertion_speed = insertion_speed

            if point_reduction < min_point_reduction:
                min_point_reduction = point_reduction

            if point_reduction > max_point_reduction:
                max_point_reduction = point_reduction

    i = 1
    for key in data.keys():
        run = data[key]

        node_hierarchies = []
        point_hierarchies = []
        query_times = []
        point_reductions = []
        insertion_speeds = []
        timeouted = []

        for multi_run in run:
            # data calculation
            node_hierarchies.append(multi_run["index"]["node_hierarchy"])
            point_hierarchies.append(multi_run["index"]["point_hierarchy"])
            insertion_speeds.append(multi_run["results"]["insertion_rate"]["insertion_rate_points_per_sec"])
            query_times.append(calculate_average_query_time_single_run(multi_run))
            point_reductions.append(calculate_average_point_reduction_single_run(multi_run, query_run))

            # check if timeouted
            nr_points_run = multi_run["results"]["insertion_rate"]["nr_points"]
            if nr_points_run != nr_points:
                timeouted.append(True)
            else:
                timeouted.append(False)

        ax1 = plt.subplot(num_runs, 2, i, projection='3d')
        ax2 = plt.subplot(num_runs, 2, i + 1, projection='3d')

        # Calculate colors of bars
        cmap = cm.get_cmap('jet')
        colors_indexing_speed = [cmap((x - min_insertion_speed) / (max_insertion_speed - min_insertion_speed)) for x in
                                 insertion_speeds]
        colors_query_point_reduction = [cmap((x - min_point_reduction) / (max_point_reduction - min_point_reduction))
                                        for x in point_reductions]

        # map timeouted runs to gray
        for j in range(len(timeouted)):
            if timeouted[j]:
                colors_indexing_speed[j] = (0.5, 0.5, 0.5, 1)
                colors_query_point_reduction[j] = (0.5, 0.5, 0.5, 1)

        # Plot the data
        x = node_hierarchies
        y = point_hierarchies
        bottom = np.zeros_like(insertion_speeds)
        width = depth = 0.3
        ax1.bar3d(x, y, bottom, width, depth, insertion_speeds, shade=True, color=colors_indexing_speed)
        ax2.bar3d(x, y, bottom, width, depth, point_reductions, shade=True, color=colors_query_point_reduction)

        # Set ticks
        ax1.set_xticks(node_hierarchies)
        ax1.set_yticks(point_hierarchies)
        ax2.set_xticks(node_hierarchies)
        ax2.set_yticks(point_hierarchies)

        # set scale of z axis by min and max values
        ax1.set_zlim(min_insertion_speed, max_insertion_speed)
        ax2.set_zlim(min_point_reduction, max_point_reduction)

        # Set labels and title
        ax1.set_xlabel('Node Hierarchy Size')
        ax1.set_ylabel('Point Hierarchy Size')
        ax1.set_zlabel('Insertion Rate | Points/s')
        ax1.set_title('Insertion Speed ' + str(key))

        ax2.set_xlabel('Node Hierarchy Size')
        ax2.set_ylabel('Point Hierarchy Size')
        ax2.set_zlabel('Average Point Reduction | Percentage')
        ax2.set_title('Average Point Reduction | Percentage ' + str(key))

        ax1.view_init(45, -45)
        ax2.view_init(45, -45)

        i += 2

    # Output the final plot
    plt.tight_layout()
    if title is not None:
        ax1.set_title(title)
        ax2.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
    plt.close(fig)


# Calculates the average time speedup over all queries in a single run
# Speedup is calculated between raw_point_filtering and point_filtering_with_full_acc
def calculate_average_query_time_single_run(run):
    queries = run["results"]["query_performance"]

    query_names = \
        [
            'lod0',
            'lod1',
        ]

    speedup_sum = 0
    for query in query_names:
        speedup_sum += queries[query]["query_time_seconds"]
    if len(queries) > 0:
        return speedup_sum / len(queries)
    return -1


# Calculates the average point reduction over all queries in a single run
# Point reduction is calculated between total number of points and only_full_acc in percent!
def calculate_average_point_reduction_single_run(run, query_run="only_node_acc"):
    queries = run["results"]["query_performance"]

    query_names = \
        [
            'time_range',
            'ground_classification',
            'building_classification',
            'powerline_classification',
            'vegetation_classification',
            'normal_x_vertical',
            'high_intensity',
            'low_intensity',
            'one_return',
            'mixed_ground_and_time',
        ]

    nr_points = run["results"]["insertion_rate"]["nr_points"]
    max_possible_reduction_sum = 0
    real_reduction_sum = 0
    for query in query_names:
        point_filtered_points = queries[query]["raw_point_filtering"]["nr_points"]
        max_possible_reduction = nr_points - point_filtered_points
        accelerated_points = queries[query][query_run]["nr_points"]
        real_reduction = nr_points - accelerated_points
        max_possible_reduction_sum += max_possible_reduction
        real_reduction_sum += real_reduction
    if len(queries) > 0:
        return real_reduction_sum / max_possible_reduction_sum * 100
    return 0

def rename_tpf(tpf):
    replacements = {
        "Lod": "TreeLevel",
        "NrPointsWeighted1": "NrPointsTaskAge",
        "NrPointsWeightedByTaskAge": "NrPointsTaskAge",
        "NrPointsWeighted2": None,
        "NrPointsWeighted3": None,
    }
    if tpf in replacements:
        return replacements[tpf]
    return tpf

if __name__ == '__main__':
    main()
