import os.path
from os.path import join, dirname
import json
import matplotlib.pyplot as plt
import matplotlib as mpl
import numpy as np
from matplotlib.lines import Line2D

PROJECT_ROOT = join(dirname(__file__), "..")
INPUT_FILES_PARAMETER_OVERVIEW_V1 = [join(PROJECT_ROOT, "evaluation/results/2023-07-24_macbook_parameter_overview_v1/", file) for file in [
    "macbook_parameter_overview_v1_2023-07-24_1.json",
]]

INPUT_FILES_PARAMETER_OVERVIEW_V2 = [join(PROJECT_ROOT, "evaluation/results/2023-07-25_macbook_parameter_overview_v2/", file) for file in [
    "macbook_parameter_overview_v2_2023-07-25_1.json",
]]

INPUT_FILES_CACHE_SIZE_COMPARISON = [join(PROJECT_ROOT, "evaluation/results/2023-07-31_cache_size_comparisons/", file) for file in [
    "frankfurt_2023-08-01_1.json",
    "freiburg_2023-08-01_1.json",
]]

INPUT_FILES_QUERY_OVERVIEW = [join(PROJECT_ROOT, "evaluation/results/2023-08-02_query_overview/", file) for file in [
    "query_overview_2023-08-02_1.json",
    "query_overview_2023-08-02_2.json",
]]


def main():
    # plot style
    # plt.style.use("seaborn-notebook")

    # font magic to make the output pdf viewable in Evince, and probably other pdf viewers as well...
    # without this pdf rendering of pages with figures is extremely slow, especially when zooming in a lot and
    # regularly crashes the viewer...
    mpl.rcParams['pdf.fonttype'] = 42

    for input_file in INPUT_FILES_PARAMETER_OVERVIEW_V1:
        # read file
        with open(input_file) as f:
            print("Reading file: ", input_file)
            data = json.load(f)

        # ensure output folder exists
        output_folder = f"{input_file}.diagrams"
        os.makedirs(output_folder, exist_ok=True)

        plot_insertion_rate_by_nr_threads(
            test_runs=data["runs"]["num_threads"],
            filename=join(output_folder, "insertion-rate-by-nr-threads.pdf")
        )
        plot_insertion_rate_by_cache_size(
            test_runs=data["runs"]["cache_size"],
            filename=join(output_folder, "insertion-rate-by-cache_size.pdf")
        )
        plot_insertion_rate_by_node_size(
            test_runs=data["runs"]["node_size"],
            filename=join(output_folder, "insertion-rate-by-node_size.pdf")
        )

    for input_file in INPUT_FILES_PARAMETER_OVERVIEW_V2:
        # read file
        with open(input_file) as f:
            print("Reading file: ", input_file)
            data = json.load(f)

        # ensure output folder exists
        output_folder = f"{input_file}.diagrams"
        os.makedirs(output_folder, exist_ok=True)

        plot_insertion_rate_by_cache_size(
            test_runs=data["runs"]["big_cache_size"],
            filename=join(output_folder, "insertion-rate-by-cache_size.pdf")
        )
        plot_insertion_rate_by_nr_threads(
            test_runs=data["runs"]["num_threads_compression"],
            filename=join(output_folder, "insertion-rate-by-nr-threads.pdf")
        )

    for input_file in INPUT_FILES_CACHE_SIZE_COMPARISON:
        # read file
        with open(input_file) as f:
            print("Reading file: ", input_file)
            data = json.load(f)

        # ensure output folder exists
        output_folder = f"{input_file}.diagrams"
        os.makedirs(output_folder, exist_ok=True)

        plot_insertion_rate_by_cache_size(
            test_runs=data["runs"]["cache_size"],
            filename=join(output_folder, "insertion-rate-by-cache_size.pdf")
        )

    for input_file in INPUT_FILES_QUERY_OVERVIEW:
        # read file
        with open(input_file) as f:
            print("Reading file: ", input_file)
            data = json.load(f)

        # ensure output folder exists
        output_folder = f"{input_file}.diagrams"
        os.makedirs(output_folder, exist_ok=True)

        plot_query_by_num_points(
            test_runs=data["runs"]["querying"],
            filename=join(output_folder, "query-by-num-points.pdf"),
            nr_points=data["env"]["input_file_nr_points"]
        )

        plot_query_by_time(
            test_runs=data["runs"]["querying"],
            filename=join(output_folder, "query-by-time.pdf"),
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


def make_x_node_size(ax, test_runs):
    ax.set_xlabel("Max Node Size | nr points")
    ax.set_xscale("log")
    return [int(i["index"]["node_size"]) for i in test_runs]


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


def plot_latency_by_nr_threads(test_runs, filename, title=None):
    fig: plt.Figure = plt.figure()
    ax: plt.Axes = fig.subplots()
    xs = make_x_nr_threads(ax, test_runs)
    draw_y_latency(ax, xs, test_runs)
    if title is not None:
        ax.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})


def plot_insertion_rate_by_cache_size(test_runs, filename, title=None):
    fig: plt.Figure = plt.figure()
    ax: plt.Axes = fig.subplots()
    xs = make_x_cache_size(ax, test_runs)
    ys = make_y_insertion_rate(ax, test_runs)
    ax.plot(xs, ys, marker=".")
    if title is not None:
        ax.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})


def plot_latency_by_cache_size(test_runs, filename, title=None):
    fig: plt.Figure = plt.figure()
    ax: plt.Axes = fig.subplots()
    xs = make_x_cache_size(ax, test_runs)
    draw_y_latency(ax, xs, test_runs, x_log=True)
    if title is not None:
        ax.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})


def plot_insertion_rate_by_node_size(test_runs, filename, title=None):
    fig: plt.Figure = plt.figure()
    ax: plt.Axes = fig.subplots()
    xs = make_x_node_size(ax, test_runs)
    ys = make_y_insertion_rate(ax, test_runs)
    ax.scatter(xs, ys)
    if title is not None:
        ax.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})


def plot_query_time_by_node_size(test_runs, filename, title=None):
    fig: plt.Figure = plt.figure()
    ax: plt.Axes = fig.subplots()
    xs = make_x_node_size(ax, test_runs)
    ys1 = [test_run["sensor_pos_index"]["query_performance"]["query_1"]["query_time_seconds"] +
           test_run["sensor_pos_index"]["query_performance"]["query_1"]["load_time_seconds"] for test_run in test_runs]
    ys2 = [test_run["sensor_pos_index"]["query_performance"]["query_2"]["query_time_seconds"] +
           test_run["sensor_pos_index"]["query_performance"]["query_2"]["load_time_seconds"] for test_run in test_runs]
    ys3 = [test_run["sensor_pos_index"]["query_performance"]["query_3"]["query_time_seconds"] +
           test_run["sensor_pos_index"]["query_performance"]["query_3"]["load_time_seconds"] for test_run in test_runs]
    # ax.scatter(xs, ys1, label="Query 1")
    ax.scatter(xs, ys2, label="Query 2")
    ax.scatter(xs, ys3, label="Query 3")
    ax.legend()
    if title is not None:
        ax.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})


def plot_latency_by_node_size(test_runs, filename, title=None):
    fig: plt.Figure = plt.figure()
    ax: plt.Axes = fig.subplots()
    xs = make_x_node_size(ax, test_runs)
    draw_y_latency(ax, xs, test_runs, "sensor_pos_index", x_log=True)
    if title is not None:
        ax.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})


def plot_insertion_rate_by_priority_function(test_runs, filename, title=None):
    fig: plt.Figure = plt.figure()
    ax: plt.Axes = fig.subplots()
    xs = make_x_priority_function(ax, test_runs)
    ys = make_y_insertion_rate(ax, test_runs)
    ax.bar(xs, ys, 0.7)
    if title is not None:
        ax.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})


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


def plot_latency_by_priority_function(test_runs, filename, title=None):
    fig: plt.Figure = plt.figure()
    ax: plt.Axes = fig.subplots()
    xs = make_x_priority_function(ax, test_runs)
    draw_y_latency(ax, xs, test_runs, "octree_index")
    if title is not None:
        ax.set_title(title)
    fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})


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

def plot_query_by_num_points(test_runs, nr_points, filename, title=None):
    fig, ax = plt.subplots(figsize=[10, 6])

    queries = list(test_runs[0]["results"]["query_performance"].keys())
    subqueries = ["only_node_acc", "only_full_acc", "raw_point_filtering"]

    bar_width = 0.15
    index = range(len(queries))

    colors = ['#DB4437', '#F4B400', '#0F9D58', '#4285F4']

    for run in test_runs:
        for p in range(len(queries)):

            # number of points per subquery
            plt.bar(p, nr_points, bar_width, label="nr_points", color="#DB4437")
            for i, subquery in enumerate(subqueries):

                nr_points_subquery = [run["results"]["query_performance"][queries[p]][subquery]["nr_points"]]
                plt.bar([p + (i+1)*bar_width], nr_points_subquery, bar_width, label=subquery, color=colors[i+1])


        plt.xlabel('Queries')
        plt.ylabel('Number of Points')
        plt.title(title)
        plt.xticks([p + bar_width*2 for p in index], queries, rotation=90)

        custom_legend_labels = ['All points', 'Range Filter', 'Range and Histogram Filter', 'Point Filter']  # Custom legend labels
        custom_legend_colors = colors[:len(custom_legend_labels)]  # Use the same colors for custom legend
        custom_legend_handles = [Line2D([0], [0], color=color, label=label, linewidth=8) for color, label in zip(custom_legend_colors, custom_legend_labels)]
        ax.legend(handles=custom_legend_handles, loc='upper left', bbox_to_anchor=(1, 1), title='Subqueries')

        plt.tight_layout()

        if title is not None:
            ax.set_title(title)
        fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})

def plot_query_by_time(test_runs, filename, title=None):
    fig, ax = plt.subplots(figsize=[10, 6])

    queries = list(test_runs[0]["results"]["query_performance"].keys())
    subqueries = ["raw_point_filtering", "point_filtering_with_node_acc", "point_filtering_with_full_acc", "only_node_acc", "only_full_acc"]

    bar_width = 0.15
    index = range(len(queries))

    colors = ['#DB4437', '#F4B400', '#0F9D58', '#4285F4', '#7CBB00']

    for run in test_runs:
        for p in range(len(queries)):

            # number of points per subquery
            for i, subquery in enumerate(subqueries):
                nr_points_subquery = [run["results"]["query_performance"][queries[p]][subquery]["query_time_seconds"]]
                plt.bar([p + i*bar_width], nr_points_subquery, bar_width, label=subquery, color=colors[i])


        plt.xlabel('Queries')
        plt.ylabel('Execution Time | seconds')
        plt.title(title)
        plt.xticks([p + bar_width*2 for p in index], queries, rotation=90)

        custom_legend_labels = ['Point Filter', 'Point Filter + Range Acceleration', 'Point Filter + Full Acceleration', 'Only Range Acceleration', 'Only Range + Full Acceleration']  # Custom legend labels
        custom_legend_colors = colors[:len(custom_legend_labels)]  # Use the same colors for custom legend
        custom_legend_handles = [Line2D([0], [0], color=color, label=label, linewidth=8) for color, label in zip(custom_legend_colors, custom_legend_labels)]
        ax.legend(handles=custom_legend_handles, loc='upper left', bbox_to_anchor=(1, 1), title='Subqueries')

        plt.tight_layout()

        if title is not None:
            ax.set_title(title)
        fig.savefig(filename, format="pdf", bbox_inches="tight", metadata={"CreationDate": None})




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
