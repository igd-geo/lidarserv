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

PROJECT_ROOT = join(dirname(__file__), "..")

INPUT_FILES_PARAMETER_OVERVIEW_V1 = [
    # "2023-12-14_parameter_overview/parameter_overview_v1_2024-01-08_1.json",
]

INPUT_FILES_PARAMETER_OVERVIEW_V2 = [
    # "2023-12-14_parameter_overview/parameter_overview_v2_2024-01-08_1.json",
    "2023-12-14_parameter_overview/parameter_overview_v2_2024-01-29_1.json",
]

INPUT_FILES_QUERY_PERFORMANCE = [
    # "2024-01-09_query_performance/query_performance_v1_2024-01-09_1.json",
    # "2024-01-09_query_performance/query_performance_v1_2024-01-09_2.json",
    "2024-01-09_query_performance/query_performance_v1_2024-01-25_2.json",
]
def main():
    # plot style
    # plt.style.use("seaborn-notebook")

    # font magic to make the output pdf viewable in Evince, and probably other pdf viewers as well...
    # without this pdf rendering of pages with figures is extremely slow, especially when zooming in a lot and
    # regularly crashes the viewer...
    mpl.rcParams['pdf.fonttype'] = 42

    # for input_file in INPUT_FILES_PARAMETER_OVERVIEW_V1:
    #     # read file
    #     with open(input_file) as f:
    #         print("Reading file: ", input_file)
    #         data = json.load(f)
    #
    #     # ensure output folder exists
    #     output_folder = f"{input_file}.diagrams"
    #     os.makedirs(output_folder, exist_ok=True)
    #
    #     plot_overall_performance_by_sizes(
    #         test_runs=data["runs"],
    #         filename=join(output_folder, "overall-performance-by-sizes.pdf"),
    #         nr_points=data["env"]["input_file_nr_points"],
    #     )
    #
    #
    #
    for input_file in INPUT_FILES_PARAMETER_OVERVIEW_V2:
        # read file
        with open(input_file) as f:
            print("Reading file: ", input_file)
            data = json.load(f)

        # ensure output folder exists
        output_folder = f"{input_file}.diagrams"
        os.makedirs(output_folder, exist_ok=True)

        plot_insertion_rate_by_nr_threads(
            test_runs=data["runs"]["threads"],
            filename=join(output_folder, "insertion-rate-by-nr-threads.pdf"),
        )

        plot_insertion_rate_by_cache_size(
            test_runs=data["runs"]["cache_size"],
            filename=join(output_folder, "insertion-rate-by-cache-size.pdf"),
        )

    for input_file in INPUT_FILES_QUERY_PERFORMANCE:
        # read file
        with open(input_file) as f:
            print("Reading file: ", input_file)
            data = json.load(f)

        # ensure output folder exists
        output_folder = f"{input_file}.diagrams"
        os.makedirs(output_folder, exist_ok=True)

        plot_query_by_num_points(
            test_runs=data["runs"]["test"],
            nr_points=data["env"]["input_file_nr_points"],
            filename=join(output_folder, "query-by-num-points.pdf"),
            queries=query_names(),
            labels=query_pretty_names(),
        )

        plot_query_by_num_nodes(
            test_runs=data["runs"]["test"],
            nr_nodes=data["runs"]["test"][0]["results"]["index_info"]["directory_info"]["num_nodes"],
            filename=join(output_folder, "query-by-num-nodes.pdf"),
            queries=query_names(),
            labels=query_pretty_names(),
        )

        plot_query_by_num_points_stacked(
            test_runs=data["runs"]["test"],
            nr_points=data["env"]["input_file_nr_points"],
            filename=join(output_folder, "query-by-num-points-stacked.pdf"),
        )

        plot_false_positive_rates(
            test_runs=data["runs"]["test"],
            filename=join(output_folder, "false-positive-rates.pdf"),
            queries=query_names(),
            labels=query_pretty_names(),
        )

        plot_query_by_time(
            test_runs=data["runs"]["test"],
            filename=join(output_folder, "query-by-time.pdf"),
            queries=query_names(),
            labels=query_pretty_names(),
        )

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


def draw_y_latency(ax, xs, latency_runs, x_log=False):
    ax.set_ylabel("Latency | seconds")
    bxpstats = [
        {
            "med": i["all_lods"]["median_latency_seconds"],
            "q1": i["all_lods"]["quantiles"][3]["value"],
            "q3": i["all_lods"]["quantiles"][9]["value"],
            "whislo": i["all_lods"]["quantiles"][1]["value"],
            "whishi": i["all_lods"]["quantiles"][11]["value"],
            "mean": i["all_lods"]["mean_latency_seconds"],
        } if i is not None else None for i in latency_runs
    ]
    indexes, bxpstats = zip(*[(i, v) for i, v in enumerate(bxpstats) if v is not None])
    positions = [xs[i] for i in indexes]
    if x_log:
        widths = [(positions[1] - positions[0]) * pos / positions[0] * 0.5 for it, pos in enumerate(positions)]
    else:
        widths = (positions[1] - positions[0]) * 0.7
    ax.bxp(
        bxpstats,
        positions=positions,
        shownotches=False,
        showmeans=True,
        showcaps=True,
        showbox=True,
        showfliers=False,
        manage_ticks=False,
        widths=widths
    )
    ax.set_ylim(bottom=0, top=min(max(b["q3"] for b in bxpstats) * 5.0, max(b["whishi"] for b in bxpstats) * 1.1, 2.5))


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
    ax.set_xlim(left=-.5, right=len(labels) - .5)
    return xs


def plot_insertion_rate_by_nr_threads(test_runs, filename, title=None):
    fig: plt.Figure = plt.figure()
    ax: plt.Axes = fig.subplots()
    xs = make_x_nr_threads(ax, test_runs)
    ys = make_y_insertion_rate(ax, test_runs)
    ax.scatter(xs, ys)
    if title is not None:
        ax.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
    plt.close(fig)


def plot_latency_by_nr_threads(test_runs, filename, title=None):
    fig: plt.Figure = plt.figure()
    ax: plt.Axes = fig.subplots()
    xs = make_x_nr_threads(ax, test_runs)
    draw_y_latency(ax, xs, test_runs)
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


def plot_latency_by_cache_size(test_runs, filename, title=None):
    fig: plt.Figure = plt.figure()
    ax: plt.Axes = fig.subplots()
    xs = make_x_cache_size(ax, test_runs)
    draw_y_latency(ax, xs, test_runs, x_log=True)
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


def plot_latency_by_priority_function(test_runs, filename, title=None):
    fig: plt.Figure = plt.figure()
    ax: plt.Axes = fig.subplots()
    xs = make_x_priority_function(ax, test_runs)
    draw_y_latency(ax, xs, test_runs, "octree_index")
    if title is not None:
        ax.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
    plt.close(fig)


def plot_compare_insertion_rate(test_run, filename, title=None):
    fig: plt.Figure = plt.figure(figsize=[2.7, 4.8])
    ax: plt.Axes = fig.subplots()
    indexes = ["octree_index", "sensor_pos_index"]
    test_runs = [{"config": test_run["config"], "index": test_run[index]} for index in indexes]
    xs = [0, 1]
    ys = make_y_insertion_rate(ax, test_runs)
    ax.bar(xs, ys, 0.7)
    plt.xticks(xs, indexes)
    if title is not None:
        ax.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
    plt.close(fig)


def plot_compare_latency(test_run, filename, title=None):
    fig: plt.Figure = plt.figure(figsize=[2.7, 4.8])
    ax: plt.Axes = fig.subplots()
    indexes = ["octree_index", "sensor_pos_index"]
    test_runs = [{"config": test_run["config"], "index": test_run[index]} for index in indexes]
    xs = [0, 1]
    draw_y_latency(ax, xs, test_runs, "index")
    plt.xticks(xs, indexes)
    if title is not None:
        ax.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
    plt.close(fig)


def plot_compare_query_time(test_run, filename, title=None):
    fig: plt.Figure = plt.figure(figsize=[4.6, 4.8])
    ax: plt.Axes = fig.subplots()
    xs1 = [0, 3, 6]
    xs2 = [1, 4, 7]
    ys1 = [
        test_run["octree_index"]["query_performance"]["query_1"]["query_time_seconds"] +
        test_run["octree_index"]["query_performance"]["query_1"]["load_time_seconds"],
        test_run["octree_index"]["query_performance"]["query_2"]["query_time_seconds"] +
        test_run["octree_index"]["query_performance"]["query_2"]["load_time_seconds"],
        test_run["octree_index"]["query_performance"]["query_3"]["query_time_seconds"] +
        test_run["octree_index"]["query_performance"]["query_3"]["load_time_seconds"],
    ]
    ys2 = [
        test_run["sensor_pos_index"]["query_performance"]["query_1"]["query_time_seconds"] +
        test_run["sensor_pos_index"]["query_performance"]["query_1"]["load_time_seconds"],
        test_run["sensor_pos_index"]["query_performance"]["query_2"]["query_time_seconds"] +
        test_run["sensor_pos_index"]["query_performance"]["query_2"]["load_time_seconds"],
        test_run["sensor_pos_index"]["query_performance"]["query_3"]["query_time_seconds"] +
        test_run["sensor_pos_index"]["query_performance"]["query_3"]["load_time_seconds"],
    ]
    ax.bar(xs1, ys1, 0.7, label="octree_index")
    ax.bar(xs2, ys2, 0.7, label="sensor_pos_index")
    plt.xticks([0.5, 3.5, 6.5], ["Query 1", "Query 2", "Query 3"])
    ax.set_ylabel("Query time | seconds")
    ax.legend(loc="upper left")
    if title is not None:
        ax.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
    plt.close(fig)


def plot_insertion_rates_by_disk_speed(data, filename, title=None):
    fig: plt.Figure = plt.figure(figsize=[4.6, 4.8])
    xs = [it["disk_speed_mibps"] for it in data]
    ax: plt.Axes = fig.subplots()
    ax.set_xlabel("Disk speed | MiB/s")
    ax.set_xlim(left=0, right=max(xs) + 1.0)
    ax.set_ylabel("Insertion rate | Points/s")
    y_flat = [jt for run in data for jt in run["data"]["compression"]]
    y_octree_compression = [it["octree_index"]["insertion_rate"]["insertion_rate_points_per_sec"] for it in y_flat if
                            it["config"]["compression"] is True]
    y_octree_nocompression = [it["octree_index"]["insertion_rate"]["insertion_rate_points_per_sec"] for it in y_flat if
                              it["config"]["compression"] is False]
    ax.plot(xs, y_octree_compression, label="octree_index with compression")
    ax.plot(xs, y_octree_nocompression, label="octree_index no compression")
    ax.set_ylim(bottom=0)
    ax.legend()
    if title is not None:
        ax.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
    plt.close(fig)


def plot_latencies_by_disk_speed(data, filename, title=None):
    fig: plt.Figure = plt.figure(figsize=[4.6, 4.8])
    xs = [it["disk_speed_mibps"] for it in data]
    ax: plt.Axes = fig.subplots()
    ax.set_xlabel("Disk speed | MiB/s")
    ax.set_xlim(left=0, right=max(xs) + 1.0)
    laz = [it for run in data for it in run["data"]["compression"] if it["config"]["compression"] is True]
    las = [it for run in data for it in run["data"]["compression"] if it["config"]["compression"] is False]

    ids = [index for index, it in enumerate(data) if las[index]["octree_index"]["latency"] is not None]
    xs_octree_las = [xs[index] for index in ids]
    y_octree_las = [las[index]["octree_index"]["latency"]["all_lods"]["median_latency_seconds"] for index in
                    ids]  # median
    y1_octree_las = [las[index]["octree_index"]["latency"]["all_lods"]["quantiles"][3]["value"] for index in
                     ids]  # 25% quantile
    y2_octree_las = [las[index]["octree_index"]["latency"]["all_lods"]["quantiles"][9]["value"] for index in
                     ids]  # 75% quantile

    ids = [index for index, it in enumerate(data) if laz[index]["octree_index"]["latency"] is not None]
    xs_octree_laz = [xs[index] for index in ids]
    y_octree_laz = [laz[index]["octree_index"]["latency"]["all_lods"]["median_latency_seconds"] for index in
                    ids]  # median
    y1_octree_laz = [laz[index]["octree_index"]["latency"]["all_lods"]["quantiles"][3]["value"] for index in
                     ids]  # 25% quantile
    y2_octree_laz = [laz[index]["octree_index"]["latency"]["all_lods"]["quantiles"][9]["value"] for index in
                     ids]  # 75% quantile

    ax.fill_between(xs_octree_laz, y1_octree_laz, y2_octree_laz, alpha=.2, linewidth=0)
    ax.plot(xs_octree_laz, y_octree_laz, label="octree_index with compression")
    ax.fill_between(xs_octree_las, y1_octree_las, y2_octree_las, alpha=.2, linewidth=0)
    ax.plot(xs_octree_las, y_octree_las, label="octree_index no compression")

    ax.set_ylim(bottom=0, top=0.125)

    ax.set_ylabel("Latency | seconds")
    ax.legend()
    if title is not None:
        ax.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
    plt.close(fig)


def plot_latency_by_insertion_rate(test_run, filename, title=None):
    fig: plt.Figure = plt.figure(figsize=[4.6, 4.8])
    ax: plt.Axes = fig.subplots()
    latency_runs = test_run["results"]["latency"]
    xs = [it["settings"]["points_per_sec"] for it in latency_runs]
    ax.set_xlabel("Insertion rate | Points/s")
    draw_y_latency(ax, xs, latency_runs, False)
    if title is not None:
        ax.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
    plt.close(fig)


def plot_latency_by_insertion_rate_foreach_priority_function(test_runs, filename, title=None):
    fig: plt.Figure = plt.figure(figsize=[4.6, 4.8])
    ax: plt.Axes = fig.subplots()
    for run in test_runs:
        prio_fn = run["index"]["priority_function"]
        latency_runs = run["results"]["latency"]
        xs = [it["settings"]["points_per_sec"] for it in latency_runs]
        ys_min = [it["all_lods"]["quantiles"][1]["value"] for it in latency_runs]  # 10% quantile
        ys_med = [it["all_lods"]["median_latency_seconds"] for it in latency_runs]  # median (50% quantile)
        ys_max = [it["all_lods"]["quantiles"][11]["value"] for it in latency_runs]  # 90% quantile
        ax.fill_between(xs, ys_min, ys_max, alpha=.2, linewidth=0)
        ax.plot(xs, ys_med, label=rename_tpf(prio_fn), marker=".")
    ax.set_xlabel("Insertion rate | Points/s")
    ax.set_ylabel("Latency | seconds")
    ax.set_yscale("log")
    ax.legend()
    if title is not None:
        ax.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
    plt.close(fig)


def plot_query_by_num_points(test_runs, nr_points, filename, queries=None, labels=None, title=None):
    fig, ax = plt.subplots(figsize=[10, 6])
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
        ax.legend(handles=custom_legend_handles, loc='upper left', bbox_to_anchor=(0, -0.4), title='Subqueries')

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


def plot_false_positive_rates(test_runs, filename, queries=None, labels=None, title=None):
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


def plot_query_by_time(test_runs, filename, title=None, queries=None, labels=None):
    fig, ax = plt.subplots(figsize=[10, 6])

    if queries is None:
        queries = query_names()
    if labels is None:
        labels = query_pretty_names()
    # subqueries = ["raw_spatial", "raw_point_filtering", "point_filtering_with_node_acc",
    #               "point_filtering_with_full_acc", "only_node_acc", "only_full_acc"]
    subqueries = ["raw_spatial", "raw_point_filtering", "point_filtering_with_node_acc",
                  "point_filtering_with_full_acc"]

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
def plot_overall_performance_by_sizes(test_runs, filename, nr_points, title=None, insertion_color_threshold=150000,
                                      query_run="only_node_acc"):
    fig, ax1 = plt.subplots(figsize=[10, 6])
    ax2 = ax1.twinx()  # Create a twin Axes sharing the xaxis

    names = []
    node_values = []
    sizes_of_roots = []
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

    for i in range(len(names)):
        # names[i] = names[i] + "\n" + str(sizes_of_roots[i]) + "m"
        size = sizes_of_roots[i]
        size = "{:.2f}".format(size)
        names[i] = size + "m" + "\n" + str(node_values[i])

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
    names = names[::-1]
    insertion_speeds = insertion_speeds[::-1]
    query_speeds = query_speeds[::-1]
    point_reductions = point_reductions[::-1]
    colors = colors[::-1]

    # Plotting logic for Insertion Speed
    # plot insertion speed as bar plot with different colors for timeouted runs
    # ax1.bar(names, insertion_speeds, label='Insertion Rate | points/s', color=colors)
    ax1.plot(names, insertion_speeds, marker='o', label='Insertion Rate | Points/s', color='tab:blue')
    ax1.set_xlabel('Node Sizes')
    ax1.set_ylabel('Insertion Rate | Points/s', color='tab:blue')

    # Plotting logic for Query Speedup
    ax2.plot(names, query_speeds, marker='o', label='Average Query Time | Seconds', color='tab:green')
    ax2.set_ylabel('Average Query Time | Seconds', color='tab:green')

    # Creating a third y-axis for Point Reduction
    ax3 = ax1.twinx()
    ax3.spines['right'].set_position(('outward', 60))  # Adjust the position of the third y-axis

    ax3.plot(names, point_reductions, marker='x', label='Average Point Reduction | Percentage', color='tab:red')
    ax3.set_ylabel('Average Point Reduction | Percentage', color='tab:red')

    # ax1.axhline(y=400000, color='grey', linestyle=':', label='Insertion Rate Goal (400000 Points/s)')

    # Combine legends from all axes
    lines, labels = ax1.get_legend_handles_labels()
    lines2, labels2 = ax2.get_legend_handles_labels()
    lines3, labels3 = ax3.get_legend_handles_labels()
    print(lines, labels)

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
            'time_range',
            'ground_classification',
            # 'no_cars_classification',
            'normal_x_vertical',
            'building_classification',
            'high_intensity',
            'low_intensity',
            'one_return',
            # 'one_return',
            # 'mixed_ground_and_one_return',
            'mixed_ground_and_time',
            'mixed_ground_and_one_return',
            # 'mixed_ground_normal_one_return'
            'mixed_ground_normal_one_return'
        ]

    speedup_sum = 0
    for query in query_names:
        speedup_sum += queries[query]["point_filtering_with_node_acc"]["query_time_seconds"]
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


# Plot insertion speeds compared by attribute index states
def plot_insertion_speed_comparison(test_runs, filename):
    fig, ax = plt.subplots(figsize=[10, 6])
    bar_width = 1 / (len(test_runs) + 1)
    colors = ['#DB4437', '#F4B400', '#0F9D58', '#DB4437', '#F4B400', '#0F9D58']
    runs = [
        'uncompressed_no_attribute_index',
        'uncompressed_attribute_index',
        'uncompressed_histogram_index',
        'compressed_no_attribute_index',
        'compressed_attribute_index',
        'compressed_histogram_index',
    ]
    custom_legend_labels = [
        'No Attribute Index\nNo Compression',
        'Bounds Index\nNo Compression',
        'Histogram Index\nNo Compression',
        'No Attribute Index\nCompression',
        'Bounds Index\nCompression',
        'Histogram Index\nCompression',
    ]  # Custom legend labels
    index = []

    for i in range(len(runs)):
        insertion_rate = test_runs[runs[i]][0]["results"]["insertion_rate"]["insertion_rate_points_per_sec"]

        # Determine the x-coordinate for the bar
        if i < 3:
            x_coord = i * bar_width
        else:
            x_coord = i * bar_width + bar_width

        # Create a bar for the insertion rate
        plt.bar([x_coord], insertion_rate, bar_width, label=custom_legend_labels[i], color=colors[i])

        # Place the insertion speed value inside the bar
        plt.text(x_coord, insertion_rate, f"{insertion_rate:.2f}", va='bottom', ha='center', color='black')

        # Store the x-coordinate for later use in setting tick labels
        index.append(x_coord)

    # Set labels, title, and tick labels
    plt.xlabel('Insertion Settings')
    plt.ylabel('Insertion Rate | Points/s')
    # plt.title("Insertion Speed Comparison")
    plt.xticks([p for p in index], custom_legend_labels, rotation=90)

    # Create custom legend handles for the legend
    custom_legend_colors = colors[:len(custom_legend_labels)]
    custom_legend_handles = [Line2D([0], [0], color=color, label=label, linewidth=8) for color, label in
                             zip(custom_legend_colors, custom_legend_labels)]

    # Add the custom legend to the plot
    # ax.legend(handles=custom_legend_handles, loc='upper left', bbox_to_anchor=(1, 1), title='Attribute Index')

    # Adjust layout and save the figure
    plt.tight_layout()
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
    plt.close(fig)


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
