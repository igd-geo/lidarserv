#!/usr/bin/env python3
from dataclasses import dataclass
from genericpath import exists
from glob import glob
from os.path import join, dirname
import json
from posixpath import basename
import matplotlib.pyplot as plt
import matplotlib as mpl
import numpy as np
from matplotlib.lines import Line2D
from typing import List, Tuple
from scipy import stats
from math import floor

PROJECT_ROOT = dirname(__file__)
OUTPUT_FOLDER = join(PROJECT_ROOT, "figures")

@dataclass
class InputFile:
    name: str
    path: str
    
def main():
    # plot style
    # plt.style.use("seaborn-notebook")

    # font magic to make the output pdf viewable in Evince, and probably other pdf viewers as well...
    # without this pdf rendering of pages with figures is extremely slow, especially when zooming in a lot and
    # regularly crashes the viewer...
    mpl.rcParams['pdf.fonttype'] = 42

    # insertion_speeds
    path = join(PROJECT_ROOT, "measurements/insertion_speeds.json")
    with open(path, "r") as f:
        data = json.load(f)
    plot_insertion_speed_comparison(
        data, 
        filename=join(OUTPUT_FOLDER, "insertion_speeds.pdf")
    )

    # index_sizes
    path = join(PROJECT_ROOT, "measurements/index_sizes.json")
    with open(path, "r") as f:
        data = json.load(f)
    plot_index_size_comparison(
        data,
        filename=join(OUTPUT_FOLDER, "index_sizes.pdf")
    )
    plot_compression_rate_comparison(
        data,
        filename=join(OUTPUT_FOLDER, "index_sizes_compression.pdf")
    )

    # queries
    all_files = [
        InputFile(
            name=basename(path).removesuffix(".json"),
            path=join(PROJECT_ROOT, path)
        )
        for path 
        in glob("measurements/lidarserv/*.json", root_dir=PROJECT_ROOT)
    ]
    queries_and_labels = [
        # ahn
        (
            [
                "classification_bridges",
                "classification_building",
                "classification_ground",
                "classification_vegetation",
                "intensity_high",
                "intensity_low",
                "time_1",
                "time_2",
                "time_3"
            ], [
                "Classification\nBridges",
                "Classification\nBuildings",
                "Classification\nGround",
                "Classification\nVegetation",
                "Intensity\nHigh",
                "Intensity\nLow",
                "Time\nBig Slice",
                "Time\nSmall Slice",
                "Time\nMedium Slice"
            ], 
            "AHN4",
            "ahn4_"
        ), 

        # kitti
        (
            [
                "classification_building",
                "classification_ground",
                "pointsource1",
                "pointsource2",
                "rgb",
                "time1",
                "time2"
            ], 
            [
                "Classification\nBuildings",
                "Classification\nGround",
                "Pointsource1",
                "Pointsource2",
                "RGB",
                "Time\nBig Slice",
                "Time\nMedium Slice"
            ],
            "KITTI",
            "kitti_"
        ),

        # lille
        (
            [
                "intensity_high",
                "intensity_low",
                "time_1",
                "time_2",
                "pointsource1",
                "pointsource2",
                "scananglerank1",
                "scananglerank2",
                "view_frustum1",
            ], 
            [
                "Intensity\nHigh",
                "Intensity\nLow",
                "Time\nBig Slice",
                "Time\nSmall Slice",
                "PointsourceID\n≥10",
                "PointsourceID\n≥5",
                "ScanAngleRank\n≤45°",
                "ScanAngleRank\n≤90°",
                "View Frustum",
            ], 
            "Lille",
            "lille_"
        )
    ]
    for file in all_files:
        with open(file.path, "r") as f:
            data = json.load(f)
            print_querys(
                data,
                filename=join(OUTPUT_FOLDER, f"queries_{file.name}.tex")
            )
            for queries, labels, title, prefix in queries_and_labels:
                if not file.name.startswith(prefix):
                    continue
                plot_query_by_time(
                    data=data,
                    queries=queries,
                    labels=labels,
                    filename=join(OUTPUT_FOLDER, f"query_by_time_{file.name}.pdf"),
                    title=title
                )
                calculate_average_querying_speed(
                    data=data,
                    queries=queries,
                    filename=join(OUTPUT_FOLDER, f"average_querying_speed_{file.name}.txt"),
                    title=title
                )
                plot_query_by_num_points(
                    data=data,
                    queries=queries,
                    labels=labels,
                    filename=join(OUTPUT_FOLDER, f"query_by_points_{file.name}.pdf"),
                    title=title
                )
                plot_query_by_num_nodes(
                    data=data,
                    queries=queries,
                    labels=labels,
                    filename=join(OUTPUT_FOLDER, f"query_by_nodes_{file.name}.pdf"),
                    title=title
                )

    # latency
    for file in all_files:
        with open(file.path, "r") as f:
            data = json.load(f)
        filename=join(OUTPUT_FOLDER, f"latency_comparison_violin_{file.name}.pdf")
        if not exists(filename):    # only regenerate the latency plots, if they don't exist, because computing the viiolin shapes takes quite long.
            plot_latency_comparison_violin(
                data, 
                query="full-point-cloud",
                filename=filename, 
            )

    # insertion rate over time
    for file in all_files:
        with open(file.path, "r") as f:
            data = json.load(f)
        title = None
        if file.name.startswith("kitti"):
            title = "KITTI"
        if file.name.startswith("ahn"):
            title = "AHN4"
        if file.name.startswith("lille"):
            title = "Lille"
        
        plot_insertion_rate_progression(
            data,
            title=title,
            filename=join(OUTPUT_FOLDER, f"insertion_rate_progression_{file.name}.pdf"), 
        )

def print_querys(data, filename):
    if "main" not in data["runs"]:
        return
    if len(data["runs"]["main"]) == 0:
        return
    if "query_performance" not in data["runs"]["main"][0]["results"]:
        return
    if data["runs"]["main"][0]["results"]["query_performance"] is None:
        return
    points_file = data["settings"]["points_file"].split("/")[-1]
    total_points = data["env"]["nr_points"]
    queries = data["runs"]["main"][0]["results"]["query_performance"]
    query_statements = data["settings"]["queries"]

    with open(filename, "wt") as f:
        for query in queries:
            query_statement = query_statements[query]
            try:
                nr_points = queries[query]["nr_points"]
                selectivity = nr_points / total_points * 100
                f.write(f"{points_file} & ${query_statement}$ & {selectivity:.2f}\\% & {nr_points:.0f} \\\\ \\hline\n")
            except:
                continue

def estimate_probability_density(
        quantiles: List[Tuple[int, float]]
) -> dict[str, float | list[float]]:
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
            "quantiles": [r for l,r in quantiles if l==95],
            "min": value_min,
            "max": value_max,

            # y-position (latency values)
            "coords": coords,

            # violin width (probability density) at the y position given in "coords"
            "vals": vals,

    }

def plot_latency_comparison_violin(data, filename, query):
    if "main" not in data["runs"]:
        return
    if len(data["runs"]["main"]) == 0:
        return
    if "latency" not in data["runs"]["main"][0]["results"]:
        return
    if data["runs"]["main"][0]["results"]["latency"] is None:
        return
    if query not in data["runs"]["main"][0]["results"]["latency"]:
        return
    latency_query = data["runs"]["main"][0]["results"]["latency"][query]
    if "stats_by_lod" not in latency_query:
        return
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
    violin_parts = ax.violin(vpstats, xs, widths=0.8, showmedians=False)

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
    plt.savefig(filename, metadata={"CreationDate": None})


def plot_compression_rate_comparison(data, filename):
    fig, axs = plt.subplots(1, 3, figsize=(15, 5))

    dataset_names = {
        "ahn4": "AHN4",
        "kitti": "KITTI",
        "lille": "Lille",
    }

    tool_names = {
        "lidarserv": "Lidarserv",
        "potree_converter": "PotreeConverter 2.0",
        "pgpointcloud": "PgPointCloud",
        "laz": "Las/Laz"
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

        tools = [tool_names.get(tool, tool) for tool in tools]
        ax.set_title(dataset_names[dataset])
        ax.set_xticks(x)
        ax.set_xticklabels(tools)

        for bar, rate in zip(bars, compression_rates):
            ax.text(bar.get_x() + bar.get_width() / 2, bar.get_height(), f'{rate:.2f}%', ha='center', va='bottom')

    axs[0].set_ylabel("Compression Rate | %")
    handles, labels = axs[0].get_legend_handles_labels()
    fig.legend(handles, labels, loc='lower center', bbox_to_anchor=(0.5, -0.05), ncol=3)

    plt.tight_layout()
    plt.savefig(filename, metadata={"CreationDate": None})

def plot_index_size_comparison(data, filename):
    fig, axs = plt.subplots(1, 3, figsize=(12, 4))

    dataset_names = {
        "ahn4": "AHN4",
        "kitti": "KITTI",
        "lille": "Lille",
    }

    tool_names = {
        "lidarserv": "Lidarserv",
        "potree_converter": "PotreeConverter 2.0",
        "pgpointcloud": "PgPointCloud",
        "laz": "Las/Laz"
    }

    for i, (dataset, dataset_data) in enumerate(data.items()):
        ax = axs[i]
        tools = dataset_data.keys()
        tools = [tool for tool in tools if tool != 'input_size']
        uncompressed = [dataset_data[tool]['uncompressed'] / (1024 ** 3) for tool in tools]
        compressed = [dataset_data[tool]['compressed'] / (1024 ** 3) for tool in tools]

        x = np.arange(len(uncompressed))
        width = 0.35

        bars1 = ax.bar(x - width / 2, uncompressed, width, label='Uncompressed', color='#F4B400')
        bars2 = ax.bar(x + width / 2, compressed, width, label='Compressed', color='#4285F4')

        # Add horizontal line for input size
        input_size = dataset_data['input_size'] / (1024 ** 3)
        line = ax.axhline(y=input_size, color='r', linestyle='--', label='Input Size')

        tools = [tool_names[tool] for tool in tools]
        ax.set_title(dataset_names[dataset])
        ax.set_xticks(x)
        ax.set_xticklabels(tools)

        if input_size > 10:
            ax.yaxis.set_major_formatter(plt.FuncFormatter(lambda y, _: f"{y:.0f}GB"))
        else:
            ax.yaxis.set_major_formatter(plt.FuncFormatter(lambda y, _: f"{y:.1f}GB"))

    axs[0].set_ylabel("Size of Index | Gigabytes")

    ax.legend()
    fig.tight_layout()
    plt.savefig(filename, metadata={"CreationDate": None})

def plot_insertion_speed_comparison(data, filename):
    dataset_names = {
        "ahn4": "AHN4",
        "kitti": "KITTI",
        "lille": "Lille",
    }

    tool_names = {
        "lidarserv": "Lidarserv",
        "potree_converter": "PotreeConverter 2.0",
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
    plt.savefig(filename, metadata={"CreationDate": None})

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
        if "query_performance" not in test_run["results"]:
            return
        if test_run["results"]["query_performance"] is None:
            return
        compression = test_run["index"]["compression"]
        attribute_index = test_run["index"]["enable_attribute_index"]
        try:
            point_filtering = test_run["index"]["enable_point_filtering"]
        except:
            point_filtering = True

        for query in queries:
            if query not in test_run["results"]["query_performance"]:
                return
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
    with open(filename, "wt") as f:
        f.write(f"Dataset: {title}\n")
        f.write(f"Average querying speed without compression: {pps_no_compression:.2f} points/s\n")
        f.write(f"Average querying speed with compression: {pps_compression:.2f} points/s\n")



def plot_query_by_time(data, filename, queries, labels, title=None):
    test_runs = data["runs"]["main"]

    times = {
        "no_compression": [],
        "compression": [],
        "no_compression_attribute_index": [],
        "compression_attribute_index": []
    }

    for test_run in test_runs:
        if "enable_point_filtering" not in test_run["index"]:
            return
        compression = test_run["index"]["compression"]
        attribute_index = test_run["index"]["enable_attribute_index"]
        point_filtering = test_run["index"]["enable_point_filtering"]
        if not point_filtering:
            continue

        for query in queries:
            if "query_performance" not in test_run["results"]:
                return
            if query not in test_run["results"]["query_performance"]:
                return
            query_time = test_run["results"]["query_performance"][query]["query_time_seconds"]
            if compression and attribute_index:
                times["compression_attribute_index"].append(query_time)
            elif compression:
                times["compression"].append(query_time)
            elif attribute_index:
                times["no_compression_attribute_index"].append(query_time)
            else:
                times["no_compression"].append(query_time)

    fig, ax = plt.subplots(figsize=[10, 5.5])
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


def plot_insertion_rate_progression(data, filename: str, title=None):
    test_runs = data["runs"]["main"]

    filter_run = {
        "compression": False,
        "enable_attribute_index": True,
        "enable_point_filtering": False,
    }
    test_runs = [
        run
        for run in test_runs
        if all(k in run["index"] and run["index"][k] == v for k, v in filter_run.items())
    ]
    
    if len(test_runs) == 1:
        file_names = [filename]
    else:
        if filename.endswith(".pdf"):
            base = filename[:-4]
            suffix = ".pdf"
        else:
            base = filename
            suffix = ""
        file_names = [base + "-" + str(i) + suffix for i in range(len(test_runs))]

    for filename, run in zip(file_names, test_runs):
        if "progress_over_time" not in run["results"]["insertion_rate"]:
            return

        data = run["results"]["insertion_rate"]["progress_over_time"]
        elapsed_seconds = [it["elapsed_seconds"] for it in data]
        nr_points_done = [it["nr_points_done"] for it in data]
        gps_time = [it["gps_time"] for it in data]
        nr_points_read = [it["nr_points_read"] for it in data]
        #nr_pending_tasks = [it["nr_pending_tasks"] for it in data]
        #nr_pending_points = [it["nr_pending_points"] for it in data]
        #nr_cached_nodes = [it["nr_cached_nodes"] for it in data]

        fig, ax = plt.subplots()
        fig.set_size_inches(8, 4)

        #pps_insert = [0] + [
        #    (y2 - y1) / (x2- x1)
        #    for x1, x2, y1, y2 in zip(
        #        elapsed_seconds[1:], 
        #        elapsed_seconds[:-1], 
        #        nr_points_done[1:], 
        #        nr_points_done[:-1]
        #    )
        #]
        length = floor(elapsed_seconds[-1])
        indexes = [
            max(
                (i for i,v in enumerate(elapsed_seconds) if v <= t),
                default=0
            ) 
            for t in range(length)
        ]
        fs = [
            (t - elapsed_seconds[i]) / (elapsed_seconds[i+1] - elapsed_seconds[i])
            for t, i in enumerate(indexes)
        ]
        nr_points_done_resample = [
            nr_points_done[i] * (1-f) + nr_points_done[i+1] * f
            for i,f in zip(indexes, fs)
        ]
        nr_points_read_resample = [
            nr_points_read[i] * (1-f) + nr_points_read[i+1] * f
            for i,f in zip(indexes, fs)
        ]
        if length > 100:
            delta_t = floor(length / 100)
        else:
            delta_t = 1
        pps_insert = [
            (v2 - v1) / delta_t 
            for v1, v2 in zip(
                [0] * delta_t + nr_points_done_resample[:-delta_t],
                nr_points_done_resample
            )
        ]
        ax.plot(
            nr_points_read_resample, pps_insert, 
            label=f"Insertion Rate (moving average, Δt={delta_t}s)", color='#4285F4'
        )

        if all(t is not None for t in gps_time):
            pps_sensor = [0] + [
                (y2 - y1) / (x2- x1)
                for x1, x2, y1, y2 in zip(
                    gps_time[1:], 
                    gps_time[:-1], 
                    nr_points_read[1:], 
                    nr_points_read[:-1]
                )
            ]
            if any(pps > 10 for pps in pps_sensor):
                ax.plot(nr_points_read, pps_sensor, label="Scanner Speed", color='r', linestyle='--')
        ax.set_xlabel("Read position | Points")
        ax.set_ylabel("Points/s")
        ax.xaxis.set_major_formatter(plt.FuncFormatter(lambda y, _: f"{y / 1e6:.0f}M"))
        ax.yaxis.set_major_formatter(plt.FuncFormatter(lambda y, _: f"{y / 1e6:.1f}M"))
        ax.legend()
        #fig.tight_layout()
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
        if "enable_point_filtering" not in test_run["index"]:
            return
        point_filtering = test_run["index"]["enable_point_filtering"]
        attribute_index = test_run["index"]["enable_attribute_index"]

        for query in queries:
            if "query_performance" not in test_run["results"]:
                return
            if query not in test_run["results"]["query_performance"]:
                return
            num_points = test_run["results"]["query_performance"][query]["nr_points"]
            if point_filtering and attribute_index:
                points["point_filtering_attribute_index"].append(num_points)
            elif point_filtering and not attribute_index:
                points["point_filtering_no_attribute_index"].append(num_points)
            elif not point_filtering and attribute_index:
                points["no_point_filtering_attribute_index"].append(num_points)
            else:
                points["no_point_filtering_no_attribute_index"].append(num_points)

    fig, ax = plt.subplots(figsize=[10, 5.5])
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
        'No Attribute Index',
        'Attribute Index',
        'Attribute Index + Sequential Filtering'
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
        if "enable_point_filtering" not in test_run["index"]:
            return
        point_filtering = test_run["index"]["enable_point_filtering"]
        attribute_index = test_run["index"]["enable_attribute_index"]

        for query in queries:
            if "query_performance" not in test_run["results"]:
                return
            if query not in test_run["results"]["query_performance"]:
                return
            num_nodes = test_run["results"]["query_performance"][query]["nr_non_empty_nodes"]
            if point_filtering and attribute_index:
                nodes["point_filtering_attribute_index"].append(num_nodes)
            elif point_filtering and not attribute_index:
                nodes["point_filtering_no_attribute_index"].append(num_nodes)
            elif not point_filtering and attribute_index:
                nodes["no_point_filtering_attribute_index"].append(num_nodes)
            else:
                nodes["no_point_filtering_no_attribute_index"].append(num_nodes)

    fig, ax = plt.subplots(figsize=[10, 5.5])
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
        'No Attribute Index',
        'Attribute Index',
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


if __name__ == '__main__':
    main()
