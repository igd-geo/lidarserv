#!/usr/bin/env python3
from dataclasses import dataclass
from genericpath import exists
from glob import glob
from os.path import join, dirname
import json
from posixpath import basename
from pprint import pprint
import matplotlib.pyplot as plt
import matplotlib as mpl
from matplotlib.ticker import ScalarFormatter
import numpy as np
from matplotlib.lines import Line2D
from typing import List, Tuple
from scipy import stats
from math import floor
from collections import deque

PROJECT_ROOT = dirname(__file__)
OUTPUT_FOLDER = join(PROJECT_ROOT, "figures")

QUERIES_AND_LABELS = [
    # ahn
    (
        # queries
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
        ], 
        
        # labels
        [
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

        # figure title
        "AHN4",

        # file name prefix
        "ahn4_"
    ), 

    # kitti
    (
        # queries
        [
            "classification_building",
            "classification_ground",
            "pointsource1",
            "pointsource2",
            "rgb",
            "time1",
            "time2"
        ], 

        # labels
        [
            "Classification\nBuildings",
            "Classification\nGround",
            "Pointsource1",
            "Pointsource2",
            "RGB",
            "Time\nBig Slice",
            "Time\nMedium Slice"
        ],

        # figure title
        "KITTI",

        # file name prefix
        "kitti_"
    ),

    # lille
    (
        # queries
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

        # labels
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

        # figure title
        "Lille",

        # file name prefix
        "lille_"
    ),


    # kitti - attribute index tests
    (
        # queries
        [f"semantic_{i}" for i in range(6, 45) if i not in [18, 23]],  # classes 18 and 23 are empty (do not exist in our dataset)

        # labels
        [f"Class {i}" for i in range(6, 45) if i not in [18, 23]], 

        # figure title
        "KITTI",

        # file name prefix
        "kitti-attr-idx_"
    ),

    # ahn - attribute index tests
    (
        [
            "classification_1",
            "classification_2",
            "classification_6",
            "classification_9",
            "classification_14",
            "classification_26",
            "time_slice_1",
            "time_slice_2",
            "time_slice_3",
        ],
        [
            "Classification 1",
            "Classification 2",
            "Classification 6",
            "Classification 9",
            "Classification 14",
            "Classification 26",
            "GpsTime 60s slice",
            "GpsTime 5m slice\n(1 flight line)",
            "GpsTime 2h slice",
        ],
        "AHN4",
        "ahn4-attr-idx_",
    )
]


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
    for file in all_files:
        with open(file.path, "r") as f:
            data = json.load(f)
            if "querying" in data["runs"]:
                run = "querying"
            elif "main" in data["runs"]:
                run = "main"
            else:
                continue
            print_querys(
                data, 
                run,
                filename=join(OUTPUT_FOLDER, f"queries_{file.name}.tex")
            )
            queries, labels, title = next((
                    (queries, labels, title)
                    for queries, labels, title, prefix in QUERIES_AND_LABELS
                    if file.name.startswith(prefix)
                ), 
                (None, None, None)
            )
            if queries is None:
                continue
            plot_query_by_time(
                data=data,
                run=run,
                queries=queries,
                labels=labels,
                filename=join(OUTPUT_FOLDER, f"query_by_time_{file.name}.pdf"),
                title=title
            )
            calculate_average_querying_speed(
                data=data,
                run=run,
                queries=queries,
                filename=join(OUTPUT_FOLDER, f"average_querying_speed_{file.name}.txt"),
                title=title
            )
            plot_query_by_num_points(
                data=data,
                run=run,
                queries=queries,
                labels=labels,
                filename=join(OUTPUT_FOLDER, f"query_by_points_{file.name}.pdf"),
                title=title
            )
            plot_query_by_num_nodes(
                data=data,
                run=run,
                queries=queries,
                labels=labels,
                filename=join(OUTPUT_FOLDER, f"query_by_nodes_{file.name}.pdf"),
                title=title
            )

    # latency
    for file in all_files:
        with open(file.path, "r") as f:
            data = json.load(f)
        if "latency" in data["runs"]:
            run = "latency"
        elif "main" in data["runs"]:
            run = "main"
        else:
            continue
        filename=join(OUTPUT_FOLDER, f"latency_comparison_violin_lod_{file.name}.pdf")
        if not exists(filename):    # only regenerate the latency plots, if they don't exist, because computing the viiolin shapes takes quite long.
            plot_latency_comparison_violin_lod(
                data, 
                run=run,
                query="full-point-cloud",
                filename=filename, 
            )

        queries, labels, title = next((
                (queries, labels, title)
                for queries, labels, title, prefix in QUERIES_AND_LABELS
                if file.name.startswith(prefix)
            ), 
            (None, None, None)
        )
        if queries is None:
            continue
        filename=join(OUTPUT_FOLDER, f"latency_comparison_violin_queries_{file.name}.pdf")
        if not exists(filename):
            plot_latency_comparison_violin_queries(
                data, 
                run=run,
                queries=queries,
                labels=labels,
                filename=filename, 
            )

    # insertion rate over time
    for file in all_files:
        with open(file.path, "r") as f:
            data = json.load(f)
        if "insertion-speed" in data["runs"]:
            run = "insertion-speed"
        else:
            continue

        queries, labels, title = next((
                    (queries, labels, title)
                    for queries, labels, title, prefix in QUERIES_AND_LABELS
                    if file.name.startswith(prefix)
                ), 
                (None, None, None)
            )
        
        plot_insertion_rate_progression(
            data,
            run=run,
            title=title,
            filename=join(OUTPUT_FOLDER, f"insertion_rate_progression_{file.name}.pdf"), 
        )

    # attribute index comparison
    for file in all_files:
        with open(file.path, "r") as f:
            data = json.load(f)
        if "attr-index" in data["runs"]:
            run = "attr-index"
        else:
            continue
        queries, labels, title = next((
                (queries, labels, title)
                for queries, labels, title, prefix in QUERIES_AND_LABELS
                if file.name.startswith(prefix)
            ), 
            (None, None, None)
        )
        if queries is None:
            continue
        
        plot_attribute_indexes_query_comparisson(
            data,
            run=run,
            queries=queries,
            labels=labels,
            title=title,
            filename=join(OUTPUT_FOLDER, f"attribute_indexes_query_time_{file.name}.pdf"),
            y_label="Execution Time | Seconds",
            plot_attribute="query_time_seconds" 
        )
        plot_attribute_indexes_query_comparisson(
            data,
            run=run,
            queries=queries,
            labels=labels,
            title=title,
            filename=join(OUTPUT_FOLDER, f"attribute_indexes_nodes_{file.name}.pdf"),
            y_label="Number of Nodes",
            plot_attribute="nr_nodes" 
        )
        plot_attribute_indexes_index_comparisson(
            data,
            run=run,
            title=title,
            filename=join(OUTPUT_FOLDER, f"attribute_indexes_point_rate_{file.name}.pdf"),
        )
    

def print_querys(data, run, filename):
    points_file = data["settings"]["points_file"].split("/")[-1]
    total_points = data["env"]["nr_points"]
    query_statements = data["settings"]["queries"]
    data_run = data["runs"][run]
    data_run = [
        d for d in data_run 
        if "enable_point_filtering" not in d["index"]
        or d["index"]["enable_point_filtering"] in [True, "PointFiltering"]
    ]
    if len(data_run) == 0:
        return
    data_results = data_run[0]["results"]
    if "query_performance" not in data_results:
        return
    if data_results["query_performance"] is None:
        return
    queries = data_results["query_performance"]

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
                if r2 == r1:
                    factor = 1
                else:
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

def plot_latency_comparison_violin_lod(data, run, filename, query):
    data_run = data["runs"][run]
    if len(data_run) == 0:
        return
    if "latency" not in data_run[0]["results"]:
        return
    if data_run[0]["results"]["latency"] is None:
        return
    if query not in data_run[0]["results"]["latency"]:
        return
    latency_query = data_run[0]["results"]["latency"][query]
    if "stats_by_lod" not in latency_query:
        return
    stats_by_lod = latency_query["stats_by_lod"]
    stats_full = latency_query["stats"]

    # order lods
    lod_order = [f"LOD{i}" for i in range(0, 50)]
    lods = [lod for lod in lod_order if lod in stats_by_lod.keys()]

    # Prepare data for plotting
    quantiles = {query: [] for query in lods}

    ymax1 = max(percentile[1] for percentile in stats_full["percentiles"])
    ymax2 = max(percentile[1] for stats_lod in stats_by_lod.values() for percentile in stats_lod["percentiles"])
    ymax = max(ymax1, ymax2)
    if ymax < 10:
        unit = "ms"
        unit_factor = 1_000
    elif ymax < 10 * 60:
        unit = "s"
        unit_factor = 1
    elif ymax < 10 * 60 * 60:
        unit = "min"
        unit_factor = 1/60
    else:
        unit = "h"
        unit_factor = 1/60/60

    # add full point cloud stats
    quantiles["full-point-cloud"] = [(percentile[0], percentile[1] * unit_factor) for percentile in stats_full["percentiles"]]

    # add lod stats
    for lod in lods:
        percentiles = stats_by_lod[lod]["percentiles"]
        percentiles = [(percentile[0], percentile[1] * unit_factor) for percentile in percentiles]
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
    ax.set_xticks(xs)
    ax.set_xticklabels(lods)
    ax.set_ylabel(f"Latency | {unit}")
    formatter = ScalarFormatter()
    formatter.set_scientific(False)
    ax.yaxis.set_major_formatter(formatter)
    fig.tight_layout()
    fig.savefig(filename, metadata={"CreationDate": None})
    plt.close(fig)

def plot_latency_comparison_violin_queries(data, run, queries, labels, filename):
    data_run = data["runs"][run]
    if len(data_run) == 0:
        return
    
    # Prepare data for plotting
    quantiles = {query: [] for query in queries}

    if data_run[0]["results"] is None:
        return
    if "latency" not in data_run[0]["results"]:
        return
    if data_run[0]["results"]["latency"] is None:
        return

    for query in queries:
        if query not in data_run[0]["results"]["latency"]:
            return
        latency_query = data_run[0]["results"]["latency"][query]
        if "stats_by_lod" not in latency_query:
            return
        stats_full = latency_query["stats"]
        if stats_full is None:
            return
        if "percentiles" not in stats_full:
            return

        quantiles[query] = [(percentile[0], percentile[1]) for percentile in stats_full["percentiles"]]

    if len(quantiles) == 0:
        return
    
    ymax = max(percentile[1] for stats_query in quantiles.values() for percentile in stats_query)
    if ymax < 10:
        unit = "ms"
        unit_factor = 1_000
    elif ymax < 10 * 60:
        unit = "s"
        unit_factor = 1
    elif ymax < 10 * 60 * 60:
        unit = "min"
        unit_factor = 1/60
    else:
        unit = "h"
        unit_factor = 1/60/60
    quantiles = {
        query: [
            (percent, latency * unit_factor) 
            for percent, latency in percentiles
        ] 
        for query, percentiles in quantiles.items()
    }

    
    xs = range(len(quantiles))
    vpstats = [estimate_probability_density(quantiles[key]) for key in quantiles.keys()]

    fig, ax = plt.subplots(figsize=[12, 6])
    violin_parts = ax.violin(vpstats, xs, widths=0.8, showmedians=False)

    # for i, vpstat in enumerate(vpstats):
    #     ax.text(i, vpstat["median"], f'{vpstat["median"]:.2f}', ha='center', va='bottom')

    ax.set_xticks(xs)
    ax.set_xticklabels(labels)
    ax.set_ylabel(f"Latency | {unit}")
    formatter = ScalarFormatter()
    formatter.set_scientific(False)
    ax.yaxis.set_major_formatter(formatter)
    fig.tight_layout()
    fig.savefig(filename, metadata={"CreationDate": None})
    plt.close(fig)


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
    plt.close(fig)

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
    plt.close(fig)

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
    plt.close(fig)

def calculate_average_querying_speed(data, run, queries, filename, title=None):
    test_runs = data["runs"][run]

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
        if "enable_point_filtering" in test_run["index"]:
            attribute_index = test_run["index"]["enable_attribute_index"] in [True, "All"] and test_run["index"]["enable_point_filtering"] != "NodeFilteringWithoutAttributeIndex"
            point_filtering = test_run["index"]["enable_point_filtering"] in [True, "PointFiltering"]
        else:
            attribute_index = test_run["index"]["enable_attribute_index"]
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

    with open(filename, "wt") as f:
        f.write(f"Dataset: {title}\n")
        if sum(times_no_compression) > 0:
            pps_no_compression = sum(num_points_no_compression) / sum(times_no_compression)
            f.write(f"Average querying speed without compression: {pps_no_compression:.2f} points/s\n")
        if sum(times_compression) > 0:
            pps_compression = sum(num_points_compression) / sum(times_compression)
            f.write(f"Average querying speed with compression: {pps_compression:.2f} points/s\n")



def plot_query_by_time(data, filename, run, queries, labels, title=None):
    test_runs = data["runs"][run]

    times = {
        "no_compression": [],
        "compression": [],
        "no_compression_attribute_index": [],
        "compression_attribute_index": []
    }

    for test_run in test_runs:
        compression = test_run["index"]["compression"]
        if "enable_point_filtering" in test_run["index"] and test_run["index"]["enable_point_filtering"] in [False, "NodeFiltering"]:
            continue
        if "enable_point_filtering" in test_run["index"]:
            attribute_index = test_run["index"]["enable_attribute_index"] in [True, "All"] and test_run["index"]["enable_point_filtering"] != "NodeFilteringWithoutAttributeIndex"
        else:
            attribute_index = test_run["index"]["enable_attribute_index"] in [True, "All"]

        for query in queries:
            if "query_performance" not in test_run["results"]:
                return
            if test_run["results"]["query_performance"] is None:
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

    if len(times["no_compression"]) != len(queries) \
            or len(times["no_compression_attribute_index"]) != len(queries):
        return
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


def plot_attribute_indexes_index_comparisson(data, filename, run, title):
    test_runs = data["runs"][run]
    values = dict()
    for test_run in test_runs:
        enable_attribute_index = test_run["index"]["enable_attribute_index"]
        this_run_value = test_run["results"]["insertion_rate"]["insertion_rate_points_per_sec"]
        if enable_attribute_index in values:
            print(f"received multiple test results for {enable_attribute_index} (output file: {basename(filename)})")
            return
        values[enable_attribute_index] = this_run_value
    
    if any(key not in values for key in ["All", "RangeIndexOnly", "SfcIndexOnly", "None"]):
        print(f"incomplete data (output file: {basename(filename)})")
        return

    fig, ax = plt.subplots()
    colors = ['#DB4437', '#F4B400', '#0F9D58', '#4285F4', '#bb0089']
    bar_width = 0.7
    plt.bar(
        range(4),
        [
            values["None"],
            values["RangeIndexOnly"],
            values["SfcIndexOnly"],
            values["All"],
        ],
        bar_width,
        color=colors[:4],
    )
    plt.ylabel("Insertion Speed | Points/s")
    plt.xticks(range(4), [
        "No Attribute Index",
        "Value Range Index",
        "Bin List Index",
        "Value Range Index\n+\nBin List Index"
    ])
    plt.tight_layout()
    if title is not None:
        ax.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
    plt.close(fig)


def plot_attribute_indexes_query_comparisson(data, filename, run, queries: List[str], labels, title, plot_attribute, y_label):
    test_runs = data["runs"][run]

    # exclude purely spatial queries, as they are not influenced by the attribute index
    use_queries = [not q.startswith("view_frustum") for q in queries]
    queries = [query for query, use in zip(queries, use_queries) if use]
    labels = [label for label, use in zip(labels, use_queries) if use]

    values = dict()

    for test_run in test_runs:
        enable_attribute_index = test_run["index"]["enable_attribute_index"]
        this_run_values = [
            test_run["results"]["query_performance"][query][plot_attribute] 
            for query in queries
        ]
        if enable_attribute_index in values:
            print(f"received multiple test results for {enable_attribute_index} (output file: {basename(filename)})")
            return
        values[enable_attribute_index] = this_run_values

    if any(key not in values for key in ["All", "RangeIndexOnly", "SfcIndexOnly", "None"]):
        print(f"incomplete data (output file: {basename(filename)})")
        return

    fig, ax = plt.subplots(figsize=[9, 4])

    colors = ['#DB4437', '#F4B400', '#0F9D58', '#4285F4', '#bb0089']

    bar_width = 1.0 / 4.5
    plt.bar(
        range(len(queries)),
        values["None"],
        bar_width,
        color=colors[0],
        label="No Attribute Index"
    )
    plt.bar(
        [x + bar_width for x in range(len(queries))],
        values["RangeIndexOnly"],
        bar_width,
        color=colors[1],
        label="Value Range Index"
    )
    plt.bar(
        [x + 2 * bar_width for x in range(len(queries))],
        values["SfcIndexOnly"],
        bar_width,
        color=colors[2],
        label="Bin List Index"
    )
    plt.bar(
        [x + 3 * bar_width for x in range(len(queries))],
        values["All"],
        bar_width,
        color=colors[3],
        label="Value Range Index + Bin List Index"
    )
    
    plt.ylabel(y_label)
    plt.xticks([x + 1.5 * bar_width for x in range(len(queries))], labels, rotation=90)

    ax.legend()

    plt.tight_layout()

    if title is not None:
        ax.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
    plt.close(fig)


def plot_insertion_rate_progression(data, run:str, filename: str, title=None):
    test_runs = data["runs"][run]
    filter_by_compression = len(
        set(test_run["index"]["compression"] for test_run in test_runs)
    ) > 1
    if filter_by_compression:
        test_runs = [test_run for test_run in test_runs if not test_run["index"]["compression"]]

    test_runs = [run for run in test_runs if run["index"]["compression"] == False]
    
    if len(test_runs) == 1:
        run = test_runs[0] 
    else:
        return

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
            (y2 - y1) / (x2 - x1) if (x2 - x1) != 0 else 0
            for x1, x2, y1, y2 in zip(
                gps_time[1:], 
                gps_time[:-1], 
                nr_points_read[1:], 
                nr_points_read[:-1]
            )
        ]

        # Moving average calculation for pps_sensor
        window = deque(maxlen=delta_t)
        pps_sensor_smoothed = []
        for value in pps_sensor:
            window.append(value)
            moving_avg = sum(window) / len(window)
            pps_sensor_smoothed.append(moving_avg)
        pps_sensor = pps_sensor_smoothed

        if any(pps > 10 for pps in pps_sensor):
            ax.plot(nr_points_read, pps_sensor, label=f"Scanner Speed (moving average, Δt={delta_t}s)", color='r', linestyle='--')
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


def plot_query_by_num_points(data, run, filename, queries, labels, title=None):
    test_runs = data["runs"][run]

    points = {
        "point_filtering_attribute_index": [],
        "no_point_filtering_attribute_index": [],
        "point_filtering_no_attribute_index": [],
        "no_point_filtering_no_attribute_index": []
    }
    
    filter_by_compression = len(
        set(test_run["index"]["compression"] for test_run in test_runs)
    ) > 1
    if filter_by_compression:
        test_runs = [test_run for test_run in test_runs if not test_run["index"]["compression"]]

    for test_run in test_runs:
        if "enable_point_filtering" in test_run["index"]:
            attribute_index = test_run["index"]["enable_attribute_index"] in [True, "All"] and test_run["index"]["enable_point_filtering"] != "NodeFilteringWithoutAttributeIndex"
            point_filtering = test_run["index"]["enable_point_filtering"] in [True, "PointFiltering"]
        else:
            attribute_index = test_run["index"]["enable_attribute_index"]
            point_filtering = True

        for query in queries:
            if "query_performance" not in test_run["results"]:
                return
            if test_run["results"]["query_performance"] is None:
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

    if len(points["no_point_filtering_no_attribute_index"]) != len(queries) \
            or len(points["no_point_filtering_attribute_index"]) != len(queries) \
            or len(points["point_filtering_attribute_index"]) != len(queries):
        return

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


def plot_query_by_num_nodes(data, run, filename, queries, labels, title=None):
    test_runs = data["runs"][run]

    nodes = {
        "point_filtering_attribute_index": [],
        "no_point_filtering_attribute_index": [],
        "point_filtering_no_attribute_index": [],
        "no_point_filtering_no_attribute_index": []
    }
    filter_by_compression = len(
        set(test_run["index"]["compression"] for test_run in test_runs)
    ) > 1
    if filter_by_compression:
        test_runs = [test_run for test_run in test_runs if not test_run["index"]["compression"]]

    for test_run in test_runs:
        if "enable_point_filtering" in test_run["index"]:
            attribute_index = test_run["index"]["enable_attribute_index"] in [True, "All"] and test_run["index"]["enable_point_filtering"] != "NodeFilteringWithoutAttributeIndex"
            point_filtering = test_run["index"]["enable_point_filtering"] in [True, "PointFiltering"]
        else:
            attribute_index = test_run["index"]["enable_attribute_index"]
            point_filtering = True

        for query in queries:
            if "query_performance" not in test_run["results"]:
                return
            if test_run["results"]["query_performance"] is None:
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

    if len(nodes["no_point_filtering_no_attribute_index"]) != len(queries) \
            or len(nodes["no_point_filtering_attribute_index"]) != len(queries):
        return

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
