import os.path
from os.path import join, dirname
import json
import matplotlib.pyplot as plt
import matplotlib as mpl
import numpy as np
from matplotlib.lines import Line2D
from labellines import labelLine, labelLines
from mpl_toolkits.mplot3d import Axes3D
from mpl_toolkits import mplot3d
import matplotlib.cm as cm
from matplotlib.gridspec import SubplotSpec

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

    files = ["article_measurements/lidarserv/kitti_2024-12-05_3.json", "article_measurements/lidarserv/lille_2024-12-05_2.json"]
    for file in files:
        path = join(PROJECT_ROOT, file)
        with open(path, "r") as f:
            data = json.load(f)
            output_folder = f"{path}.diagrams"
            os.makedirs(output_folder, exist_ok=True)

            # plot_query_by_time(
            #     test_runs=data["runs"]["main"],
            #     filename=join(output_folder, "query-by-time.pdf"),
            #     title="Query Time"
            # )

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

    files = "article_measurements/lidarserv/kitti_2024-12-26_1.json", "article_measurements/lidarserv/lille_2024-12-26_3.json"
    for path in files:
        with open(path, "r") as f:
            data = json.load(f)
            output_folder = f"{path}.diagrams"
            os.makedirs(output_folder, exist_ok=True)

            plot_latency_comparison_violin(data, output_folder)

def plot_latency_comparison_violin(data, output_folder):
    latency_query = data["runs"]["main"][0]["results"]["latency"]["full-point-cloud"]["stats_by_lod"]
    lods = list(latency_query.keys())

    # Prepare data for plotting
    percentile_data = {query: [] for query in lods}
    for lod in lods:
        percentiles = latency_query[lod]["percentiles"]
        for percentile in percentiles:
            percentile_data[lod].append(percentile)

    # remove queries with no data
    lods = [lod for lod in lods if len(percentile_data[lod]) > 0]

    # create arbitrary data to be able to plot a violin plot
    simulation_data = []
    for lod in lods:
        data = []
        for p in percentile_data[lod]:
            percentile = p[0]
            value = p[1]
            data.extend([value*1000] * int(percentile))
        simulation_data.append(data)

    # plot the violin plot
    fig, ax = plt.subplots(figsize=[10, 6])
    parts = ax.violinplot(simulation_data, showmeans=False, showmedians=True, widths=0.9)
    ax.set_xticks(np.arange(1, len(lods) + 1))
    ax.set_xticklabels(lods)
    plt.xticks(rotation=90)
    ax.set_ylabel("Latency | ms")


    plt.tight_layout()
    plt.savefig(join(output_folder, "latency_comparison_violin.pdf"))
    plt.close(fig)


def plot_index_size_comparison(data, output_folder):
    fig, axs = plt.subplots(1, 3, figsize=(15, 5))

    dataset_names = {
        "kitti": "KITTI",
        "lille": "Lille",
        "ahn4": "AHN4"
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
        "kitti": "KITTI",
        "lille": "Lille",
        "ahn4": "AHN4"
    }

    tool_names = {
        "lidarserv": "Lidarserv",
        "potree_converter": "PotreeConverter",
        "pgpointcloud": "PgPointCloud"
    }

    fig, axs = plt.subplots(1, 3, figsize=(15, 5))
    max_y_value = 0  # To track the maximum y-value across all datasets

    # First pass to determine the maximum y-value
    for dataset, dataset_data in data.items():
        num_points = dataset_data['num_points']
        tools = dataset_data.keys()
        tools = [tool for tool in tools if tool != 'num_points']
        uncompressed = [num_points / dataset_data[tool]['uncompressed'] for tool in tools]
        compressed = [num_points / dataset_data[tool]['compressed'] for tool in tools]

        # Update the maximum y-value
        max_y_value = max(max_y_value, max(uncompressed + compressed)) * 1.01

    # Second pass to create the plots with a consistent y-scale
    for i, (dataset, dataset_data) in enumerate(data.items()):
        num_points = dataset_data['num_points']
        ax = axs[i]
        tools = dataset_data.keys()
        tools = [tool for tool in tools if tool != 'num_points']
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


def plot_query_by_time(test_runs, filename, title=None, queries=None, labels=None):
    fig, ax = plt.subplots(figsize=[10, 6])

    if queries is None:
        queries = query_names()
    if labels is None:
        labels = query_pretty_names()
    # subqueries = ["raw_spatial", "raw_point_filtering", "point_filtering_with_node_acc",
    #               "point_filtering_with_full_acc", "only_node_acc", "only_full_acc"]
    # subqueries = ["raw_spatial", "raw_point_filtering", "point_filtering_with_node_acc",
    #               "point_filtering_with_full_acc"]


    bar_width = 1 / (len(subqueries) + 1)
    index = range(len(queries))

    colors = ['#9637DB', '#DB4437', '#F4B400', '#0F9D58', '#4285F4', '#bb0089']

    for run in test_runs:
        for p in range(len(queries)):

            # number of points per subquery
            for i, subquery in enumerate(subqueries):
                # try catch, because raw_spatial_query is not always available
                try:
                    nr_points_subquery = [
                        run["results"]["query_performance"][queries[p]][subquery]["query_time_seconds"]]
                    plt.bar([p + i * bar_width], nr_points_subquery, bar_width, label=subquery, color=colors[i])
                except:
                    pass

        # plt.xlabel('Queries')
        plt.ylabel('Execution Time | Seconds')
        # plt.title(title)
        plt.xticks([p + bar_width * 2 for p in index], labels, rotation=90)

        custom_legend_labels = [
            'Spatial Query',
            'Spatial Query + Point Filter',
            'Spatial Query + Range Filter + Point Filter',
            'Spatial Query + Range Filter + Histogram Filter + Point Filter',
            # 'Bounds Filter',
            # 'Bounds Filter\nHistogram Filter',
        ]  # Custom legend labels
        custom_legend_colors = colors[:len(custom_legend_labels)]  # Use the same colors for custom legend
        custom_legend_handles = [Line2D([0], [0], color=color, label=label, linewidth=8) for color, label in
                                 zip(custom_legend_colors, custom_legend_labels)]
        ax.legend(handles=custom_legend_handles, loc='upper left', bbox_to_anchor=(0, -0.4), title='Subqueries')

        plt.tight_layout()

        if title is not None:
            ax.set_title(title)
        fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
        plt.close(fig)


def plot_query_by_num_points(test_runs, nr_points, filename, queries=None, labels=None, title=None):
    fig, ax = plt.subplots(figsize=[8, 4.8])
    subqueries = ["only_node_acc", "only_full_acc", "raw_point_filtering"]

    bar_width = 0.15
    index = range(len(queries))

    colors = ['#DB4437', '#F4B400', '#0F9D58', '#4285F4']

    for run in test_runs:
        insertion_rate_block = run["results"]["insertion_rate"]
        if insertion_rate_block is not None:
            nr_points = insertion_rate_block["nr_points"]

        for p in range(len(queries)):

            # number of points per subquery
            plt.bar(p, nr_points, bar_width, label="nr_points", color="#DB4437")
            for i, subquery in enumerate(subqueries):
                nr_points_subquery = [run["results"]["query_performance"][queries[p]][subquery]["nr_points"]]
                plt.bar([p + (i + 1) * bar_width], nr_points_subquery, bar_width, label=subquery, color=colors[i + 1])

        # plt.xlabel('Queries')
        plt.ylabel('Number of Points')
        # plt.title(title)
        plt.xticks([p + bar_width * 2 for p in index], labels, rotation=90, ha='right')

        custom_legend_labels = ['All Points', 'Range Filter', 'Range Filter + Histogram Filter',
                                'Point Filter']  # Custom legend labels
        custom_legend_colors = colors[:len(custom_legend_labels)]  # Use the same colors for custom legend
        custom_legend_handles = [Line2D([0], [0], color=color, label=label, linewidth=8) for color, label in
                                 zip(custom_legend_colors, custom_legend_labels)]
        ax.legend(handles=custom_legend_handles, loc='upper left', bbox_to_anchor=(0, -0.8), title='Subqueries')

        plt.tight_layout()

        if title is not None:
            ax.set_title(title)
        fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
        plt.close(fig)


def plot_query_by_num_nodes(test_runs, nr_nodes, filename, queries=None, labels=None, title=None):
    fig, ax = plt.subplots(figsize=[10, 6])
    subqueries = ["only_node_acc", "only_full_acc"]

    bar_width = 0.15
    index = range(len(queries))

    colors = ['#DB4437', '#F4B400', '#0F9D58', '#4285F4']

    for run in test_runs:
        nr_nodes = run["results"]["index_info"]["directory_info"]["num_nodes"]

        for p in range(len(queries)):

            # number of nodes per subquery
            plt.bar(p, nr_nodes, bar_width, label="nr_nodes", color="#DB4437")
            for i, subquery in enumerate(subqueries):
                nr_nodes_subquery = [run["results"]["query_performance"][queries[p]][subquery]["nr_nodes"]]
                plt.bar([p + (i + 1) * bar_width], nr_nodes_subquery, bar_width, label=subquery, color=colors[i + 1])

            # check if nr_non_empty_nodes exist in json
            if "nr_non_empty_nodes" in run["results"]["query_performance"][queries[p]]["point_filtering_with_full_acc"]:
                nr_non_empty_nodes = [run["results"]["query_performance"][queries[p]]["point_filtering_with_full_acc"][
                                          "nr_non_empty_nodes"]]
                plt.bar([p + 3 * bar_width], nr_non_empty_nodes, bar_width, label="nr_non_empty_nodes",
                        color=colors[i + 2])

        # plt.xlabel('Queries')
        plt.ylabel('Number of Nodes')
        # plt.title(title)
        plt.xticks([p + bar_width * 2 for p in index], labels, rotation=90, ha='right')

        custom_legend_labels = ['All Nodes', 'Range Filter', 'Range Filter + Histogram Filter',
                                'Nodes Containing Searched Points']  # Custom legend labels
        custom_legend_colors = colors[:len(custom_legend_labels)]  # Use the same colors for custom legend
        custom_legend_handles = [Line2D([0], [0], color=color, label=label, linewidth=8) for color, label in
                                 zip(custom_legend_colors, custom_legend_labels)]
        ax.legend(handles=custom_legend_handles, loc='upper left', bbox_to_anchor=(0, -0.4), title='Subqueries')

        plt.tight_layout()

        if title is not None:
            ax.set_title(title)
        fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
        plt.close(fig)


def plot_query_by_num_points_stacked(test_runs, nr_points, filename, title=None):
    fig, ax = plt.subplots(figsize=[10, 6])

    queries = query_names()
    subqueries = ["raw_point_filtering", "only_full_acc", "only_node_acc"]

    bar_width = 0.6
    index = range(len(queries))

    colors = ['#DB4437', '#F4B400', '#0F9D58', '#4285F4']
    colors = colors[::-1]

    for run in test_runs:
        # plt.axhline(y=nr_points, color='#DB4437', linestyle='-')
        insertion_rate_block = run["results"]["insertion_rate"]
        if insertion_rate_block is not None:
            nr_points = insertion_rate_block["nr_points"]
        for p in range(len(queries)):
            bottom = 0
            for i, subquery in enumerate(subqueries):
                nr_points_subquery = run["results"]["query_performance"][queries[p]][subquery]["nr_points"]

                plt.bar(
                    p,
                    nr_points_subquery - bottom,
                    bar_width,
                    bottom=bottom,
                    label=subquery if i == 0 else "",
                    color=colors[i],
                )
                bottom += nr_points_subquery - bottom
            plt.bar(
                p,
                nr_points - bottom,
                bar_width,
                bottom=bottom,
                label=subquery if i == 0 else "",
                color=colors[i + 1],
            )

        plt.xlabel('Queries')
        plt.ylabel('Number of Points')
        # plt.title(title)
        labels = query_pretty_names()
        plt.xticks([p for p in index], labels, rotation=90, ha='right')

        custom_legend_labels = ['Point Filter', 'Histogram Filter', 'Bounds Filter',
                                'All Points']  # Custom legend labels
        custom_legend_labels = custom_legend_labels[::-1]
        colors = colors[::-1]
        custom_legend_colors = colors[0:4]  # Use the same colors for custom legend
        custom_legend_handles = [Line2D([0], [0], color=color, label=label, linewidth=8) for color, label in
                                 zip(custom_legend_colors, custom_legend_labels)]
        ax.legend(handles=custom_legend_handles, loc='upper left', bbox_to_anchor=(1, 1), title='Subqueries')

        plt.tight_layout()

        if title is not None:
            ax.set_title(title)
        fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
        plt.close(fig)


def plot_query_false_positive_rates(test_runs, filename, queries=None, labels=None, title=None):
    fig, ax = plt.subplots(figsize=[10, 6])
    subqueries = ["raw_spatial", "only_node_acc", "only_full_acc", "point_filtering_with_full_acc"]

    bar_width = 0.15
    index = range(len(queries))

    colors = ['#DB4437', '#F4B400', '#0F9D58', '#4285F4']

    for run in test_runs:
        for p in range(len(queries)):
            # total number of points
            points_total = run["results"]["query_performance"][queries[p]]["raw_spatial"]["nr_points"]
            nodes_total = run["results"]["query_performance"][queries[p]]["raw_spatial"]["nr_nodes"]

            # number of points for node acceleration
            points_node_acc = run["results"]["query_performance"][queries[p]]["only_node_acc"]["nr_points"]
            nodes_node_acc = run["results"]["query_performance"][queries[p]]["only_node_acc"]["nr_nodes"]

            # number of points for hist acceleration
            points_full_acc = run["results"]["query_performance"][queries[p]]["only_full_acc"]["nr_points"]
            nodes_full_acc = run["results"]["query_performance"][queries[p]]["only_full_acc"]["nr_nodes"]

            # ground truth (minimum)
            points_point_filtering = run["results"]["query_performance"][queries[p]]["point_filtering_with_full_acc"][
                "nr_points"]
            nodes_point_filtering = run["results"]["query_performance"][queries[p]]["point_filtering_with_full_acc"][
                "nr_non_empty_nodes"]

            # all not searched points (all negatives)
            false_points = points_total - points_point_filtering
            false_nodes = nodes_total - nodes_point_filtering

            # all not searches points after node acceleration (false positives)
            false_points_node = points_node_acc - points_point_filtering
            false_nodes_node = nodes_node_acc - nodes_point_filtering

            # all not searches points after hist acceleration (false positives)
            false_points_full = points_full_acc - points_point_filtering
            false_nodes_full = nodes_full_acc - nodes_point_filtering

            # false positive percentage node acceleration
            false_points_node_percentage = false_points_node / false_points * 100
            false_nodes_node_percentage = false_nodes_node / false_nodes * 100

            # false positive percentage hist acceleration
            false_points_full_percentage = false_points_full / false_points * 100
            false_nodes_full_percentage = false_nodes_full / false_nodes * 100

            # plot it
            plt.bar([p + 0 * bar_width], false_points_node_percentage, bar_width, color=colors[0])
            plt.bar([p + 1 * bar_width], false_points_full_percentage, bar_width, color=colors[1])
            plt.bar([p + 2.5 * bar_width], false_nodes_node_percentage, bar_width, color=colors[2])
            plt.bar([p + 3.5 * bar_width], false_nodes_full_percentage, bar_width, color=colors[3])

        # plt.xlabel('Queries')
        plt.ylabel('False Positive Rate | Percentage')
        # plt.title(title)
        plt.xticks([p + bar_width * 2 for p in index], labels, rotation=90, ha='right')

        custom_legend_labels = ['False Positive Points - Range Filtering', 'False Positive Points - Histogram Filtering', 'False Positive Nodes - Range Filtering',
                                'False Positive Nodes - Histogram Filtering']  # Custom legend labels
        custom_legend_colors = colors[:len(custom_legend_labels)]  # Use the same colors for custom legend
        custom_legend_handles = [Line2D([0], [0], color=color, label=label, linewidth=8) for color, label in
                                 zip(custom_legend_colors, custom_legend_labels)]
        ax.legend(handles=custom_legend_handles, loc='upper left', bbox_to_anchor=(0, -0.4), title='Subqueries')

        plt.tight_layout()

        if title is not None:
            ax.set_title(title)
        fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
        plt.close(fig)


def plot_query_true_negative_rates(test_runs, filename, queries=None, labels=None, title=None):
    fig, ax = plt.subplots(figsize=[10, 6])
    subqueries = ["raw_spatial", "only_node_acc", "only_full_acc", "point_filtering_with_full_acc"]

    bar_width = 0.15
    index = range(len(queries))

    colors = ['#DB4437', '#F4B400', '#0F9D58', '#4285F4']

    for run in test_runs:
        for p in range(len(queries)):
            # total number of points
            points_total = run["results"]["query_performance"][queries[p]]["raw_spatial"]["nr_points"]
            nodes_total = run["results"]["query_performance"][queries[p]]["raw_spatial"]["nr_nodes"]

            # number of points for node acceleration
            points_node_acc = run["results"]["query_performance"][queries[p]]["only_node_acc"]["nr_points"]
            nodes_node_acc = run["results"]["query_performance"][queries[p]]["only_node_acc"]["nr_nodes"]

            # number of points for hist acceleration
            points_full_acc = run["results"]["query_performance"][queries[p]]["only_full_acc"]["nr_points"]
            nodes_full_acc = run["results"]["query_performance"][queries[p]]["only_full_acc"]["nr_nodes"]

            # ground truth (minimum)
            points_point_filtering = run["results"]["query_performance"][queries[p]]["point_filtering_with_full_acc"][
                "nr_points"]
            nodes_point_filtering = run["results"]["query_performance"][queries[p]]["point_filtering_with_full_acc"][
                "nr_non_empty_nodes"]

            # true negative percentage node acceleration
            true_points_node_percentage = (points_total - points_node_acc) / points_total * 100
            true_nodes_node_percentage = (nodes_total - nodes_node_acc) / nodes_total * 100

            # true negative percentage hist acceleration
            true_points_full_percentage = (points_total - points_full_acc) / points_total * 100
            true_nodes_full_percentage = (nodes_total - nodes_full_acc) / nodes_total * 100

            # plot it
            plt.bar([p + 0 * bar_width], true_points_node_percentage, bar_width, color=colors[0])
            plt.bar([p + 1 * bar_width], true_points_full_percentage, bar_width, color=colors[1])
            plt.bar([p + 2.5 * bar_width], true_nodes_node_percentage, bar_width, color=colors[2])
            plt.bar([p + 3.5 * bar_width], true_nodes_full_percentage, bar_width, color=colors[3])

        # plt.xlabel('Queries')
        plt.ylabel('True Negative Rate | Percentage')
        # plt.title(title)
        plt.xticks([p + bar_width * 2 for p in index], labels, rotation=90, ha='right')

        custom_legend_labels = ['True Negative Points - Range Filtering', 'True Negative Points - Histogram Filtering', 'True Negative Nodes - Range Filtering',
                                'True Negative Nodes - Histogram Filtering']  # Custom legend labels
        custom_legend_colors = colors[:len(custom_legend_labels)]  # Use the same colors for custom legend
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
    labelLines(ax.get_lines(), zorder=2.5)
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


def query_names():
    return\
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

def query_pretty_names():
    return [
        'Time\nSmall Range',
        'Classification\nGround',
        'Classification\nBuilding',
        'Classification\nPowerline',
        'Classification\nVegetation',
        'Normal\nVertical',
        'Intensity\nHigh Value',
        'Intensity\nLow Value',
        'Number of Returns\nOne Return',
        'Mixed\nGround and Time Range',
    ]


if __name__ == '__main__':
    main()
