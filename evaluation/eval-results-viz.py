import os.path
from os.path import join, dirname
import json
import matplotlib.pyplot as plt
import matplotlib as mpl
import numpy as np

PROJECT_ROOT = join(dirname(__file__), "..")
INPUT_FILES_OCTREE_V1 = [join(PROJECT_ROOT, "evaluation/results/", file) for file in [
    "octree_v1_2022-04-07_1.json",
    "octree_v1_2022-04-08_1.json",
    "octree_v1_2022-04-09_1.json",
    "octree_v1_2022-04-10_1.json",
]]
INPUT_FILES_OCTREE_V2 = [join(PROJECT_ROOT, "evaluation/results/", file) for file in [
    "octree_v2_2022-04-12_1.json",
]]
INPUT_FILES_OCTREE_V3 = [join(PROJECT_ROOT, "evaluation/results/", file) for file in [
    "octree_v3_2022-04-14_1.json",
    "octree_v3_2022-04-27_1.json",
    "octree_v3_2022-04-28_1.json",
    "octree_v3_2022-04-29_1.json",
    "octree_v3_2022-04-30_1.json",
]]
INPUT_FILES_OCTREE_REDUCED_V1 = [join(PROJECT_ROOT, "evaluation/results/", file) for file in [
    "octree_reduced_v1_2022-05-19_1.json",
    "octree_reduced_v1_2022-05-19_2.json",
    "octree_reduced_v1_2022-05-19_3.json",
    "octree_reduced_v1_2022-05-19_4.json",
    "octree_reduced_v1_2022-05-19_5.json",
    "octree_reduced_v1_2022-05-19_6.json",
]]
INPUT_FILES_OCTREE_REDUCED_V2 = [join(PROJECT_ROOT, "evaluation/results/", file) for file in [
    "octree_reduced_v2_2022-05-19_1.json",
    "octree_reduced_v2_2022-05-19_2.json",
    "octree_reduced_v2_2022-05-20_1.json",
    "octree_reduced_v2_2022-05-20_2.json",
]]
INPUT_FILES_OCTREE_OPTIM_V1 = [join(PROJECT_ROOT, "evaluation/results/", file) for file in [
    "octree_optim_v1_2022-05-20_1.json",
    "octree_optim_v1_2022-05-20_2.json",
    "octree_optim_v1_2022-05-21_1.json",
    "octree_optim_v1_2022-05-22_1.json",
]]
INPUT_FILES_SENSORPOS_PARALLELISATION = [join(PROJECT_ROOT, "evaluation/results/", file) for file in [
    "sensorpos_parallelisation_2022-04-11_1.json",
]]
INPUT_FILES_COMBINEDINSERTION_RATE = [
    (
        join(PROJECT_ROOT, "evaluation/results/octree_v3_2022-04-30_1.json"),
        join(PROJECT_ROOT, "evaluation/results/octree_reduced_v2_2022-05-20_1.json"),
        join(PROJECT_ROOT, "evaluation/results/insertion_rate_hdd_vs_ssd"),
    )
]

def main():
    # plot style
    # plt.style.use("seaborn-notebook")

    # font magic to make the output pdf viewable in Evince, and probably other pdf viewers as well...
    # without this pdf rendering of pages with figures is extremely slow, especially when zooming in a lot and
    # regularly crashes the viewer...
    mpl.rcParams['pdf.fonttype'] = 42

    for input_file in INPUT_FILES_SENSORPOS_PARALLELISATION:

        # read file
        with open(input_file) as f:
            data = json.load(f)

        # ensure output folder exists
        output_folder = f"{input_file}.diagrams"
        os.makedirs(output_folder, exist_ok=True)

        # plot
        plot_insertion_rate_by_nr_threads(
            test_runs=data["num_threads"],
            filename=join(output_folder, "insertion-rate-by-nr-threads.pdf")
        )
        plot_insertion_rate_by_nr_threads(
            test_runs=data["num_threads_no_compression"],
            title="no compression",
            filename=join(output_folder, "insertion-rate-by-nr-threads-no-compression.pdf")
        )

    for input_file in INPUT_FILES_OCTREE_V1:

        # read file
        with open(input_file) as f:
            data = json.load(f)

        # ensure output folder exists
        output_folder = f"{input_file}.diagrams"
        os.makedirs(output_folder, exist_ok=True)

        plot_insertion_rate_by_nr_threads(
            test_runs=data["parallelisation"],
            filename=join(output_folder, "insertion-rate-by-nr-threads.pdf")
        )
        plot_insertion_rate_by_priority_function(
            test_runs=data["prio_fn_simple"],
            filename=join(output_folder, "insertion-rate-by-priority-function.pdf")
        )
        plot_insertion_rate_by_priority_function(
            test_runs=data["prio_fn_no_cache"],
            title="no cache",
            filename=join(output_folder, "insertion-rate-by-priority-function-nocache.pdf")
        )
        plot_insertion_rate_by_cache_size(
            test_runs=data["cache"],
            filename=join(output_folder, "insertion-rate-by-cache-size.pdf")
        )
        plot_latency_by_insertion_rate(
            test_run=data["general"][0],
            filename=join(output_folder, "latency-by-insertion-rate.pdf")
        )
        plot_latency_by_insertion_rate_foreach_priority_function(
            test_runs=data["prio_fn_simple"],
            filename=join(output_folder, "latency-by-insertion-rate-foreach-priority-function.pdf")
        )

    for input_file in INPUT_FILES_OCTREE_V2 + INPUT_FILES_OCTREE_V3:

        # read file
        with open(input_file) as f:
            data = json.load(f)

        # ensure output folder exists
        output_folder = f"{input_file}.diagrams"
        os.makedirs(output_folder, exist_ok=True)

        default_run = next(
            it
            for it in data["runs"]["prio_fn_simple"]
            if it["index"]["priority_function"] == "NrPointsWeightedByTaskAge"
        )
        plot_insertion_rate_by_nr_threads(
            test_runs=data["runs"]["parallelisation"],
            filename=join(output_folder, "insertion-rate-by-nr-threads.pdf")
        )
        plot_insertion_rate_by_priority_function(
            test_runs=data["runs"]["prio_fn_simple"],
            filename=join(output_folder, "insertion-rate-by-priority-function.pdf")
        )
        plot_insertion_rate_by_priority_function(
            test_runs=data["runs"]["prio_fn_no_cache"],
            title="no cache",
            filename=join(output_folder, "insertion-rate-by-priority-function-nocache.pdf")
        )
        plot_insertion_rate_by_cache_size(
            test_runs=data["runs"]["cache"],
            filename=join(output_folder, "insertion-rate-by-cache-size.pdf")
        )
        plot_latency_by_insertion_rate(
            test_run=default_run,
            filename=join(output_folder, "latency-by-insertion-rate.pdf")
        )
        plot_latency_by_insertion_rate_foreach_priority_function(
            test_runs=data["runs"]["prio_fn_simple"],
            filename=join(output_folder, "latency-by-insertion-rate-foreach-priority-function.pdf")
        )
        plot_insertion_rate_by_priority_function_bogus(
            test_runs=data["runs"]["prio_fn_simple"] + data["runs"]["prio_fn_with_bogus"],
            filename=join(output_folder, "insertion-rate-by-nr-bogus-points-foreach-priority-function.pdf")
        )
        plot_duration_cleanup_by_priority_function_bogus(
            test_runs=data["runs"]["prio_fn_simple"] + data["runs"]["prio_fn_with_bogus"],
            filename=join(output_folder, "cleanup-time-by-nr-bogus-points-foreach-priority-function.pdf")
        )

    for input_file in INPUT_FILES_OCTREE_REDUCED_V1:

        # read file
        with open(input_file) as f:
            data = json.load(f)

        # ensure output folder exists
        output_folder = f"{input_file}.diagrams"
        os.makedirs(output_folder, exist_ok=True)

        plot_insertion_rate_by_priority_function(
            test_runs=data["runs"]["prio_fn_simple"],
            filename=join(output_folder, "insertion-rate-by-priority-function.pdf")
        )
        plot_insertion_rate_by_priority_function(
            test_runs=data["runs"]["prio_fn_no_cache"],
            title="no cache",
            filename=join(output_folder, "insertion-rate-by-priority-function-nocache.pdf")
        )

    for input_file in INPUT_FILES_OCTREE_REDUCED_V2:

        # read file
        with open(input_file) as f:
            data = json.load(f)

        # ensure output folder exists
        output_folder = f"{input_file}.diagrams"
        os.makedirs(output_folder, exist_ok=True)

        plot_insertion_rate_by_priority_function(
            test_runs=data["runs"]["prio_fn_simple"],
            filename=join(output_folder, "insertion-rate-by-priority-function.pdf")
        )
        plot_insertion_rate_by_nr_threads(
            test_runs=data["runs"]["parallelisation"],
            filename=join(output_folder, "insertion-rate-by-nr-threads.pdf")
        )

    for input_file in INPUT_FILES_OCTREE_OPTIM_V1:

        # read file
        with open(input_file) as f:
            data = json.load(f)

        # ensure output folder exists
        output_folder = f"{input_file}.diagrams"
        os.makedirs(output_folder, exist_ok=True)

        plot_insertion_rate_by_priority_function_bogus(
            test_runs=data["runs"]["prio_fn_with_bogus"],
            filename=join(output_folder, "insertion-rate-by-bogus-foreach-priority-function.pdf")
        )
        plot_insertion_rate_by_priority_function_cache(
            test_runs=data["runs"]["prio_fn_with_cache"],
            filename=join(output_folder, "insertion-rate-by-cache-size-foreach-priority-function.pdf")
        )

    for input_file_1, input_file_2, output_folder in INPUT_FILES_COMBINEDINSERTION_RATE:
        with open(input_file_1) as f:
            data_1 = json.load(f)
        with open(input_file_2) as f:
            data_2 = json.load(f)
        os.makedirs(output_folder, exist_ok=True)

        priofns_1 = [p["index"]["priority_function"] for p in data_1["runs"]["prio_fn_simple"]]
        priofns_2 = [p["index"]["priority_function"] for p in data_2["runs"]["prio_fn_simple"]]
        assert priofns_1 == priofns_2
        fig: plt.Figure = plt.figure()
        ax: plt.Axes = fig.subplots()
        xs = make_x_priority_function(ax, data_1["runs"]["prio_fn_simple"])
        xs1 = [x - .2 for x in xs]
        xs2 = [x + .2 for x in xs]
        ys1 = make_y_insertion_rate(ax, data_1["runs"]["prio_fn_simple"])
        ys2 = make_y_insertion_rate(ax, data_2["runs"]["prio_fn_simple"])
        ax.bar(xs1, ys1, 0.3, label="Virtual Server")
        ax.bar(xs2, ys2, 0.3, label="Laptop System")
        ax.legend()
        fig.savefig(join(output_folder, "by-priority-function.pdf"), format="pdf", bbox_inches="tight", metadata={"CreationDate": None})

        fig: plt.Figure = plt.figure()
        ax: plt.Axes = fig.subplots()
        xs2 = make_x_nr_threads(ax, data_2["runs"]["parallelisation"])
        xs1 = make_x_nr_threads(ax, data_1["runs"]["parallelisation"])
        ys2 = make_y_insertion_rate(ax, data_2["runs"]["parallelisation"])
        ys1 = make_y_insertion_rate(ax, data_1["runs"]["parallelisation"])
        ax.scatter(xs1, ys1, label="Virtual Server")
        ax.scatter(xs2, ys2, label="Laptop System")
        ax.legend()
        fig.savefig(join(output_folder, "by-nr_threads.pdf"), format="pdf", bbox_inches="tight", metadata={"CreationDate": None})



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
    #ax.set_xscale("log")
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
    ys1 = [test_run["sensor_pos_index"]["query_performance"]["query_1"]["query_time_seconds"] + test_run["sensor_pos_index"]["query_performance"]["query_1"]["load_time_seconds"] for test_run in test_runs]
    ys2 = [test_run["sensor_pos_index"]["query_performance"]["query_2"]["query_time_seconds"] + test_run["sensor_pos_index"]["query_performance"]["query_2"]["load_time_seconds"] for test_run in test_runs]
    ys3 = [test_run["sensor_pos_index"]["query_performance"]["query_3"]["query_time_seconds"] + test_run["sensor_pos_index"]["query_performance"]["query_3"]["load_time_seconds"] for test_run in test_runs]
    #ax.scatter(xs, ys1, label="Query 1")
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
        test_run["octree_index"]["query_performance"]["query_1"]["query_time_seconds"] + test_run["octree_index"]["query_performance"]["query_1"]["load_time_seconds"],
        test_run["octree_index"]["query_performance"]["query_2"]["query_time_seconds"] + test_run["octree_index"]["query_performance"]["query_2"]["load_time_seconds"],
        test_run["octree_index"]["query_performance"]["query_3"]["query_time_seconds"] + test_run["octree_index"]["query_performance"]["query_3"]["load_time_seconds"],
    ]
    ys2 = [
        test_run["sensor_pos_index"]["query_performance"]["query_1"]["query_time_seconds"] + test_run["sensor_pos_index"]["query_performance"]["query_1"]["load_time_seconds"],
        test_run["sensor_pos_index"]["query_performance"]["query_2"]["query_time_seconds"] + test_run["sensor_pos_index"]["query_performance"]["query_2"]["load_time_seconds"],
        test_run["sensor_pos_index"]["query_performance"]["query_3"]["query_time_seconds"] + test_run["sensor_pos_index"]["query_performance"]["query_3"]["load_time_seconds"],
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
    y_octree_compression = [it["octree_index"]["insertion_rate"]["insertion_rate_points_per_sec"] for it in y_flat if it["config"]["compression"] is True]
    y_octree_nocompression = [it["octree_index"]["insertion_rate"]["insertion_rate_points_per_sec"] for it in y_flat if it["config"]["compression"] is False]
    y_sensorpos_compression = [it["sensor_pos_index"]["insertion_rate"]["insertion_rate_points_per_sec"] for it in y_flat if it["config"]["compression"] is True]
    y_sensorpos_nocompression = [it["sensor_pos_index"]["insertion_rate"]["insertion_rate_points_per_sec"] for it in y_flat if it["config"]["compression"] is False]
    ax.plot(xs, y_octree_compression, label="octree_index with compression")
    ax.plot(xs, y_octree_nocompression, label="octree_index no compression")
    ax.plot(xs, y_sensorpos_compression, label="sensor_pos_index with compression")
    ax.plot(xs, y_sensorpos_nocompression, label="sensor_pos_index no compression")
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
    y_octree_las = [las[index]["octree_index"]["latency"]["all_lods"]["median_latency_seconds"] for index in ids]  # median
    y1_octree_las = [las[index]["octree_index"]["latency"]["all_lods"]["quantiles"][3]["value"] for index in ids]   # 25% quantile
    y2_octree_las = [las[index]["octree_index"]["latency"]["all_lods"]["quantiles"][9]["value"] for index in ids]   # 75% quantile

    ids = [index for index, it in enumerate(data) if laz[index]["octree_index"]["latency"] is not None]
    xs_octree_laz = [xs[index] for index in ids]
    y_octree_laz = [laz[index]["octree_index"]["latency"]["all_lods"]["median_latency_seconds"] for index in ids]  # median
    y1_octree_laz = [laz[index]["octree_index"]["latency"]["all_lods"]["quantiles"][3]["value"] for index in ids]   # 25% quantile
    y2_octree_laz = [laz[index]["octree_index"]["latency"]["all_lods"]["quantiles"][9]["value"] for index in ids]   # 75% quantile


    ids = [index for index, it in enumerate(data) if las[index]["sensor_pos_index"]["latency"] is not None]
    xs_sensorpos_las = [xs[index] for index in ids]
    y_sensorpos_las = [las[index]["sensor_pos_index"]["latency"]["all_lods"]["median_latency_seconds"] for index in ids]  # median
    y1_sensorpos_las = [las[index]["sensor_pos_index"]["latency"]["all_lods"]["quantiles"][3]["value"] for index in ids]   # 25% quantile
    y2_sensorpos_las = [las[index]["sensor_pos_index"]["latency"]["all_lods"]["quantiles"][9]["value"] for index in ids]   # 75% quantile

    ids = [index for index, it in enumerate(data) if laz[index]["sensor_pos_index"]["latency"] is not None]
    xs_sensorpos_laz = [xs[index] for index in ids]
    y_sensorpos_laz = [laz[index]["sensor_pos_index"]["latency"]["all_lods"]["median_latency_seconds"] for index in ids]  # median
    y1_sensorpos_laz = [laz[index]["sensor_pos_index"]["latency"]["all_lods"]["quantiles"][3]["value"] for index in ids]   # 25% quantile
    y2_sensorpos_laz = [laz[index]["sensor_pos_index"]["latency"]["all_lods"]["quantiles"][9]["value"] for index in ids]   # 75% quantile

    ax.fill_between(xs_octree_laz, y1_octree_laz, y2_octree_laz, alpha=.2, linewidth=0)
    ax.plot(xs_octree_laz, y_octree_laz, label="octree_index with compression")
    ax.fill_between(xs_octree_las, y1_octree_las, y2_octree_las, alpha=.2, linewidth=0)
    ax.plot(xs_octree_las, y_octree_las, label="octree_index no compression")

    ax.fill_between(xs_sensorpos_laz, y1_sensorpos_laz, y2_sensorpos_laz, alpha=.2, linewidth=0)
    ax.plot(xs_sensorpos_laz, y_sensorpos_laz, label="sensor_pos_index with compression")
    ax.fill_between(xs_sensorpos_las, y1_sensorpos_las, y2_sensorpos_las, alpha=.2, linewidth=0)
    ax.plot(xs_sensorpos_las, y_sensorpos_las, label="sensor_pos_index no compression")

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
        ys_min = [it["all_lods"]["quantiles"][1]["value"] for it in latency_runs]   # 10% quantile
        ys_med = [it["all_lods"]["median_latency_seconds"] for it in latency_runs]   # median (50% quantile)
        ys_max = [it["all_lods"]["quantiles"][11]["value"] for it in latency_runs]   # 90% quantile
        ax.fill_between(xs, ys_min, ys_max, alpha=.2, linewidth=0)
        ax.plot(xs, ys_med, label=rename_tpf(prio_fn), marker=".")
    ax.set_xlabel("Insertion rate | points/s")
    ax.set_ylabel("Latency | seconds")
    ax.set_yscale("log")
    ax.legend()
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
