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
    join(PROJECT_ROOT, "evaluation/results/2023-07-24_macbook_parameter_overview_v1/", file) for file in [
        "macbook_parameter_overview_v1_2023-07-24_1.json",
    ]]

INPUT_FILES_PARAMETER_OVERVIEW_V2 = [
    join(PROJECT_ROOT, "evaluation/results/2023-07-25_macbook_parameter_overview_v2/", file) for file in [
        "macbook_parameter_overview_v2_2023-07-25_1.json",
    ]]

INPUT_FILES_CACHE_SIZE_COMPARISON = [join(PROJECT_ROOT, "evaluation/results/2023-07-31_cache_size_comparisons/", file)
                                     for file in [
                                         "frankfurt_2023-08-01_1.json",
                                         "freiburg_2023-08-01_1.json",
                                     ]]

INPUT_FILES_QUERY_OVERVIEW = [join(PROJECT_ROOT, "evaluation/results/2023-08-02_query_overview/", file) for file in [
    "query_overview_2023-08-02_1.json",
    "query_overview_2023-08-02_2.json",
    "query_overview_2023-08-03_1.json",
    "query_overview_2023-08-03_2.json",
    "query_overview_2023-08-03_3.json",
]]

INPUT_FILES_NODE_HIERARCHY_COMPARISON = [
    join(PROJECT_ROOT, "evaluation/results/2023-08-03_node_hierarchy_comparison/", file) for file in [
        "node_hierarchy_comparison_2023-08-03_1.json",
        "node_hierarchy_comparison_2023-08-05_2.json",
        "node_hierarchy_comparison_v2_2023-08-05_1.json",
    ]]

INPUT_FILES_NODE_HIERARCHY_COMPARISON_3D = [
    join(PROJECT_ROOT, "evaluation/results/2023-08-03_node_hierarchy_comparison/", file) for file in [
        "node_hierarchy_comparison_v3_2023-08-06_1.json",
    ]]


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
    #     plot_insertion_rate_by_nr_threads(
    #         test_runs=data["runs"]["num_threads"],
    #         filename=join(output_folder, "insertion-rate-by-nr-threads.pdf")
    #     )
    #     plot_insertion_rate_by_cache_size(
    #         test_runs=data["runs"]["cache_size"],
    #         filename=join(output_folder, "insertion-rate-by-cache_size.pdf")
    #     )
    #
    # for input_file in INPUT_FILES_PARAMETER_OVERVIEW_V2:
    #     # read file
    #     with open(input_file) as f:
    #         print("Reading file: ", input_file)
    #         data = json.load(f)
    #
    #     # ensure output folder exists
    #     output_folder = f"{input_file}.diagrams"
    #     os.makedirs(output_folder, exist_ok=True)
    #
    #     plot_insertion_rate_by_cache_size(
    #         test_runs=data["runs"]["big_cache_size"],
    #         filename=join(output_folder, "insertion-rate-by-cache_size.pdf")
    #     )
    #     plot_insertion_rate_by_nr_threads(
    #         test_runs=data["runs"]["num_threads_compression"],
    #         filename=join(output_folder, "insertion-rate-by-nr-threads.pdf")
    #     )
    #
    # for input_file in INPUT_FILES_CACHE_SIZE_COMPARISON:
    #     # read file
    #     with open(input_file) as f:
    #         print("Reading file: ", input_file)
    #         data = json.load(f)
    #
    #     # ensure output folder exists
    #     output_folder = f"{input_file}.diagrams"
    #     os.makedirs(output_folder, exist_ok=True)
    #
    #     plot_insertion_rate_by_cache_size(
    #         test_runs=data["runs"]["cache_size"],
    #         filename=join(output_folder, "insertion-rate-by-cache_size.pdf")
    #     )
    #
    # for input_file in INPUT_FILES_QUERY_OVERVIEW:
    #     # read file
    #     with open(input_file) as f:
    #         print("Reading file: ", input_file)
    #         data = json.load(f)
    #
    #     # ensure output folder exists
    #     output_folder = f"{input_file}.diagrams"
    #     os.makedirs(output_folder, exist_ok=True)
    #
    #     plot_query_by_num_points(
    #         test_runs=data["runs"]["querying"],
    #         filename=join(output_folder, "query-by-num-points.pdf"),
    #         nr_points=data["env"]["input_file_nr_points"]
    #     )
    #
    #     plot_query_by_num_points_stacked(
    #         test_runs=data["runs"]["querying"],
    #         filename=join(output_folder, "query-by-num-points-stacked.pdf"),
    #         nr_points=data["env"]["input_file_nr_points"]
    #     )
    #
    #     plot_query_by_time(
    #         test_runs=data["runs"]["querying"],
    #         filename=join(output_folder, "query-by-time.pdf"),
    #     )

    for input_file in INPUT_FILES_NODE_HIERARCHY_COMPARISON:
        # read file
        with open(input_file) as f:
            print("Reading file: ", input_file)
            data = json.load(f)

        # ensure output folder exists
        output_folder = f"{input_file}.diagrams"
        os.makedirs(output_folder, exist_ok=True)

        plot_query_lod_nodes_by_runs(
            test_runs=data["runs"],
            filename=join(output_folder, "query-by-lod-nodes.pdf"),
        )

        plot_overall_performance_by_sizes(
            test_runs=data["runs"],
            filename=join(output_folder, "overall-performance.pdf"),
            nr_points=data["env"]["input_file_nr_points"]
        )

    for input_file in INPUT_FILES_NODE_HIERARCHY_COMPARISON_3D:
        # read file
        with open(input_file) as f:
            print("Reading file: ", input_file)
            data = json.load(f)

        # ensure output folder exists
        output_folder = f"{input_file}.diagrams"
        os.makedirs(output_folder, exist_ok=True)

        hierarchies = {"hierarchies": data["runs"]["hierarchies"]}
        plot_overall_performance_by_sizes(
            test_runs=hierarchies,
            filename=join(output_folder, "overall-performance_hierarchies.pdf"),
            nr_points=data["env"]["input_file_nr_points"]

        )

        hierarchies_fast = {"hierarchies": data["runs"]["hierarchies_cached_high_threads"]}
        plot_overall_performance_by_sizes(
            test_runs=hierarchies_fast,
            filename=join(output_folder, "overall-performance_hierarchies_fast.pdf"),
            nr_points=data["env"]["input_file_nr_points"]

        )

        plot_overall_performance_by_sizes_3d(
            data=data["runs"],
            filename=join(output_folder, f"overall-performance-3d.pdf"),
            nr_points=data["env"]["input_file_nr_points"]
        )


def make_y_insertion_rate(ax, test_runs):
    ys = [i["results"]["insertion_rate"]["insertion_rate_points_per_sec"] for i in test_runs]
    ax.set_ylabel("Insertion rate | points/s")
    ymax = max(ys) * 1.1
    bottom, top = ax.get_ylim()
    if bottom != 0 or top < ymax:
        ax.set_ylim(bottom=0, top=ymax)
    return ys


def make_y_duration_cleanup(ax, test_runs):
    ys = [i["results"]["insertion_rate"]["duration_cleanup_seconds"] for i in test_runs]
    ax.set_ylabel("Cleanup time | points/s")
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
    ax.set_ylabel("Insertion rate | points/s")
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
    ax.set_xlabel("Insertion rate | points/s")
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
    ax.set_xlabel("Insertion rate | points/s")
    ax.set_ylabel("Latency | seconds")
    ax.set_yscale("log")
    ax.legend()
    if title is not None:
        ax.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
    plt.close(fig)


def plot_query_by_num_points(test_runs, nr_points, filename, title=None):
    fig, ax = plt.subplots(figsize=[10, 6])

    queries = \
        [
            'time_range',
            'ground_classification',
            'no_cars_classification',
            'high_intensity',
            'low_intensity',
            'full_red_part',
            'one_return',
            'mixed_ground_and_one_return',
            'mixed_ground_and_time',
        ]
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

        plt.xlabel('Queries')
        plt.ylabel('Number of Points')
        plt.title(title)
        labels = \
            [
                'Time\nSmall Range',
                'Classification\nGround',
                'Classification\nNo Cars',
                'Intensity\nHigh Value',
                'Intensity\nLow Value',
                'Color\nHigh Red Value',
                'Number of Returns\nOne or More Returns',
                'Mixed\nGround and One Return',
                'Mixed\nGround and Time Range',
            ]
        plt.xticks([p + bar_width * 2 for p in index], labels, rotation=90, ha='right')

        custom_legend_labels = ['All points', 'Bounds Filter', 'Histogram Filter',
                                'Point Filter']  # Custom legend labels
        custom_legend_colors = colors[:len(custom_legend_labels)]  # Use the same colors for custom legend
        custom_legend_handles = [Line2D([0], [0], color=color, label=label, linewidth=8) for color, label in
                                 zip(custom_legend_colors, custom_legend_labels)]
        ax.legend(handles=custom_legend_handles, loc='upper left', bbox_to_anchor=(1, 1), title='Subqueries')

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
        plt.title(title)
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


def plot_query_by_time(test_runs, filename, title=None):
    fig, ax = plt.subplots(figsize=[10, 6])

    queries = query_names()
    subqueries = ["raw_spatial", "raw_point_filtering", "point_filtering_with_node_acc",
                  "point_filtering_with_full_acc", "only_node_acc", "only_full_acc"]

    bar_width = 1 / (len(subqueries) + 1)
    index = range(len(queries))

    colors = ['#FF6D00', '#DB4437', '#F4B400', '#0F9D58', '#4285F4', '#7CBB00']

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

        plt.xlabel('Queries')
        plt.ylabel('Execution Time | seconds')
        plt.title(title)
        labels = query_pretty_names()
        plt.xticks([p + bar_width * 2 for p in index], labels, rotation=90)

        custom_legend_labels = [
            'Spatial Query',
            'Spatial Query\nPoint Filter',
            'Spatial Query\nBounds Filter\nPoint Filter',
            'Spatial Query\nBounds Filter\nHistogram Filter\nPoint Filter',
            'Bounds Filter',
            'Bounds Filter\nHistogram Filter',
        ]  # Custom legend labels
        custom_legend_colors = colors[:len(custom_legend_labels)]  # Use the same colors for custom legend
        custom_legend_handles = [Line2D([0], [0], color=color, label=label, linewidth=8) for color, label in
                                 zip(custom_legend_colors, custom_legend_labels)]
        ax.legend(handles=custom_legend_handles, loc='upper left', bbox_to_anchor=(1, 1), title='Subqueries')

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
    plt.title('Number of Nodes per LOD')

    if title is not None:
        ax.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
    plt.close(fig)


# Plots Insertion Speed, Query Time Speedup and Query Point Reduction according to the node and point hierarchy
# IMPORTANT: Always use the same number of points for all runs (no timeout)
# Else the query time speedup is not comparable (rest is probably fine)
def plot_overall_performance_by_sizes(test_runs, filename, nr_points, title=None, insertion_color_threshold=300000):
    fig, ax1 = plt.subplots(figsize=[20, 12])
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
            node_hierarchy = multi_run["index"]["node_hierarchy"]
            point_hierarchy = multi_run["index"]["point_hierarchy"]
            run_name = "N" + str(node_hierarchy) + "P" + str(point_hierarchy)
            names.append(run_name)

            # data calculation
            sizes_of_roots.append(multi_run["results"]["index_info"]["root_cell_size"][0])
            insertion_speeds.append(multi_run["results"]["insertion_rate"]["insertion_rate_points_per_sec"])
            query_speeds.append(calculate_average_query_time_single_run(multi_run))
            point_reductions.append(calculate_average_point_reduction_single_run(multi_run))

            # check if timeouted
            nr_points_run = multi_run["results"]["insertion_rate"]["nr_points"]
            if nr_points_run != nr_points:
                timeouted.append(True)
            else:
                timeouted.append(False)

    for i in range(len(names)):
        names[i] = names[i] + "\n" + str(sizes_of_roots[i]) + "m"

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
    ax1.bar(names, insertion_speeds, label='Insertion Speed (Points per Second)', color=colors)
    ax1.set_xlabel('Runs')
    ax1.set_ylabel('Insertion Speed', color='tab:blue')

    # Plotting logic for Query Speedup
    ax2.plot(names, query_speeds, marker='o', label='Average Query Time (Seconds)', color='tab:green')
    ax2.set_ylabel('Average Query Time (Seconds)', color='tab:green')

    # Creating a third y-axis for Point Reduction
    ax3 = ax1.twinx()
    ax3.spines['right'].set_position(('outward', 60))  # Adjust the position of the third y-axis

    ax3.plot(names, point_reductions, marker='x', label='Average Point Reduction (Percent)', color='tab:red')
    ax3.set_ylabel('Average Point Reduction (Percent)', color='tab:red')

    # Combine legends from all axes
    lines, labels = ax1.get_legend_handles_labels()
    lines2, labels2 = ax2.get_legend_handles_labels()
    lines3, labels3 = ax3.get_legend_handles_labels()

    ax3.legend(lines + lines2 + lines3, labels + labels2 + labels3, loc='upper left')

    ax1.tick_params(axis='y', labelcolor='tab:blue')
    ax2.tick_params(axis='y', labelcolor='tab:green')
    ax3.tick_params(axis='y', labelcolor='tab:red')

    plt.xticks(rotation=45, ha='right')
    plt.title(title if title else 'Overall Performance by Run')
    plt.tight_layout()

    if title is not None:
        ax1.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})
    plt.close(fig)

# Plots overall performance by node and point hierarchy sizes (3D)
# IMPORTANT: Always use the same number of points for all runs (no timeout)
# Else the query time speedup is not comparable (rest is probably fine)
def plot_overall_performance_by_sizes_3d(data, filename, nr_points, title=None):
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
            point_reductions.append(calculate_average_point_reduction_single_run(multi_run))

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
        ax1.set_zlabel('Insertion Speed (Points per Second)')
        ax1.set_title('Insertion Speed ' + str(key))

        ax2.set_xlabel('Node Hierarchy Size')
        ax2.set_ylabel('Point Hierarchy Size')
        ax2.set_zlabel('Average Point Reduction (Percent)')
        ax2.set_title('Average Point Reduction (Percent) ' + str(key))

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
    speedup_sum = 0
    for query in queries:
        speedup_sum += queries[query]["point_filtering_with_full_acc"]["query_time_seconds"]
    if len(queries) > 0:
        return speedup_sum / len(queries)
    return -1


# Calculates the average point reduction over all queries in a single run
# Point reduction is calculated between total number of points and only_full_acc in percent!
def calculate_average_point_reduction_single_run(run):
    queries = run["results"]["query_performance"]
    nr_points = run["results"]["insertion_rate"]["nr_points"]
    point_reduction_sum = 0
    for query in queries:
        point_filtering_with_full_acc = queries[query]["only_full_acc"]["nr_points"]
        point_reduction_sum += nr_points - point_filtering_with_full_acc
    if len(queries) > 0:
        return (point_reduction_sum / len(queries)) / nr_points * 100
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
    return [
        'time_range',
        'ground_classification',
        'no_cars_classification',
        'high_intensity',
        'low_intensity',
        'full_red_part',
        'one_return',
        'mixed_ground_and_one_return',
        'mixed_ground_and_time',
    ]


def query_pretty_names():
    return [
        'Time\nSmall Range',
        'Classification\nGround',
        'Classification\nNo Cars',
        'Intensity\nHigh Value',
        'Intensity\nLow Value',
        'Color\nHigh Red Value',
        'Number of Returns\nOne or More Returns',
        'Mixed\nGround and One Return',
        'Mixed\nGround and Time Range',
    ]


if __name__ == '__main__':
    main()
