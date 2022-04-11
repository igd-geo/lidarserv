import os.path
from os.path import join, dirname
import json
import matplotlib.pyplot as plt
import matplotlib as mpl

PROJECT_ROOT = join(dirname(__file__), "..")
INPUT_FILE = join(PROJECT_ROOT, "evaluation/results/octree_2022-04-10_1.json")
OUTPUT_FOLDER = INPUT_FILE + ".diagrams"


def main():

    # font magic to make the output pdf viewable in Evince, and probably other pdf viewers as well...
    # without this pdf rendering of pages with figures is extremely slow, especially when zooming in a lot and
    # regularly crashes the viewer...
    mpl.rcParams['pdf.fonttype'] = 42

    # read file
    with open(INPUT_FILE) as f:
        data = json.load(f)

    # ensure output folder exists
    os.makedirs(OUTPUT_FOLDER, exist_ok=True)

    plot_insertion_rate_by_nr_threads(data["parallelisation"], "mno")
    plot_insertion_rate_by_priority_function(data["prio_fn_simple"], "mno")
    plot_insertion_rate_by_priority_function(data["prio_fn_no_cache"], "mno nocache")
    plot_insertion_rate_by_cache_size(data["cache"], "mno")
    plot_latency_by_insertion_rate(data["general"][0], "mno")

    # plot_latency_by_nr_threads(data["num_threads"], "octree_index")
    # plot_latency_by_nr_threads(data["num_threads"], "sensor_pos_index")
    # plot_insertion_rate_by_priority_function(tpf_hack(data["task_priority_function_low_cache"]), "octree_index no cache")
    # plot_latency_by_priority_function(tpf_hack(data["task_priority_function"]), "octree_index")
    # plot_latency_by_priority_function(tpf_hack(data["task_priority_function_low_cache"]), "octree_index no cache")
    # plot_insertion_rate_by_cache_size(data["max_cache_size"], "octree_index")
    # plot_latency_by_cache_size(data["max_cache_size"], "octree_index")
    # plot_latency_by_cache_size(data["max_cache_size"], "sensor_pos_index")
    # plot_insertion_rate_by_node_size(data["max_node_size"])
    # plot_query_time_by_node_size(data["max_node_size"])
    # plot_latency_by_node_size(data["max_node_size"])
    # plot_compare_insertion_rate(data["default"][0])
    # plot_compare_latency(data["default"][0])
    # plot_compare_query_time(data["default"][0])


def make_y_insertion_rate(ax, test_runs):
    ys = [i["results"]["insertion_rate"]["insertion_rate_points_per_sec"] for i in test_runs]
    ax.set_ylabel("Insertion rate | points/s")
    ax.set_ylim(bottom=0, top=max(ys) * 1.1)
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
    ax.set_xscale("log")
    return [int(i["index"]["cache_size"]) for i in test_runs]


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


def plot_insertion_rate_by_nr_threads(test_runs, index):
    fig: plt.Figure = plt.figure()
    ax: plt.Axes = fig.subplots()
    xs = make_x_nr_threads(ax, test_runs)
    ys = make_y_insertion_rate(ax, test_runs)
    ax.scatter(xs, ys)
    ax.set_title(index)
    fig.savefig(join(OUTPUT_FOLDER, f"{index}-insertion_rate_by_nr_threads.pdf"), format="pdf", bbox_inches="tight")


def plot_latency_by_nr_threads(test_runs, index):
    fig: plt.Figure = plt.figure()
    ax: plt.Axes = fig.subplots()
    xs = make_x_nr_threads(ax, test_runs)
    draw_y_latency(ax, xs, test_runs, index)
    ax.set_title(index)
    fig.savefig(join(OUTPUT_FOLDER, f"{index}-latency_by_nr_threads.pdf"), format="pdf", bbox_inches="tight")


def plot_insertion_rate_by_cache_size(test_runs, index):
    fig: plt.Figure = plt.figure()
    ax: plt.Axes = fig.subplots()
    xs = make_x_cache_size(ax, test_runs)
    ys = make_y_insertion_rate(ax, test_runs)
    ax.scatter(xs, ys)
    ax.set_title(index)
    fig.savefig(join(OUTPUT_FOLDER, f"{index}-insertion_rate_by_cache_size.pdf"), format="pdf", bbox_inches="tight")


def plot_latency_by_cache_size(test_runs, index):
    fig: plt.Figure = plt.figure()
    ax: plt.Axes = fig.subplots()
    xs = make_x_cache_size(ax, test_runs)
    draw_y_latency(ax, xs, test_runs, index, x_log=True)
    ax.set_title(index)
    fig.savefig(join(OUTPUT_FOLDER, f"{index}-latency_by_cache_size.pdf"), format="pdf", bbox_inches="tight")


def plot_insertion_rate_by_node_size(test_runs):
    fig: plt.Figure = plt.figure()
    ax: plt.Axes = fig.subplots()
    xs = make_x_node_size(ax, test_runs)
    ys = make_y_insertion_rate(ax, test_runs)
    ax.scatter(xs, ys)
    ax.set_title("sensor_pos_index")
    fig.savefig(join(OUTPUT_FOLDER, f"sensor_pos_index-insertion_rate_by_node_size.pdf"), format="pdf", bbox_inches="tight")


def plot_query_time_by_node_size(test_runs):
    fig: plt.Figure = plt.figure()
    ax: plt.Axes = fig.subplots()
    xs = make_x_node_size(ax, test_runs)
    ys1 = [test_run["sensor_pos_index"]["query_performance"]["query_1"]["query_time_seconds"] + test_run["sensor_pos_index"]["query_performance"]["query_1"]["load_time_seconds"] for test_run in test_runs]
    ys2 = [test_run["sensor_pos_index"]["query_performance"]["query_2"]["query_time_seconds"] + test_run["sensor_pos_index"]["query_performance"]["query_2"]["load_time_seconds"] for test_run in test_runs]
    ys3 = [test_run["sensor_pos_index"]["query_performance"]["query_3"]["query_time_seconds"] + test_run["sensor_pos_index"]["query_performance"]["query_3"]["load_time_seconds"] for test_run in test_runs]
    #ax.scatter(xs, ys1, label="Query 1")
    ax.scatter(xs, ys2, label="Query 2")
    ax.scatter(xs, ys3, label="Query 3")
    ax.set_title("sensor_pos_index")
    ax.legend()
    fig.savefig(join(OUTPUT_FOLDER, f"sensor_pos_index-query_time_by_node_size.pdf"), format="pdf", bbox_inches="tight")


def plot_latency_by_node_size(test_runs):
    fig: plt.Figure = plt.figure()
    ax: plt.Axes = fig.subplots()
    xs = make_x_node_size(ax, test_runs)
    draw_y_latency(ax, xs, test_runs, "sensor_pos_index", x_log=True)
    ax.set_title("sensor_pos_index")
    fig.savefig(join(OUTPUT_FOLDER, f"sensor_pos_index-latency_by_node_size.pdf"), format="pdf", bbox_inches="tight")


def plot_insertion_rate_by_priority_function(test_runs, title):
    fig: plt.Figure = plt.figure()
    ax: plt.Axes = fig.subplots()
    xs = make_x_priority_function(ax, test_runs)
    ys = make_y_insertion_rate(ax, test_runs)
    ax.bar(xs, ys, 0.7)
    ax.set_title(title)
    fig.savefig(join(OUTPUT_FOLDER, f"{title}-insertion_rate_by_priority_function.pdf"), format="pdf", bbox_inches="tight")


def plot_latency_by_priority_function(test_runs, title):
    fig: plt.Figure = plt.figure()
    ax: plt.Axes = fig.subplots()
    xs = make_x_priority_function(ax, test_runs)
    draw_y_latency(ax, xs, test_runs, "octree_index")
    ax.set_title(title)
    fig.savefig(join(OUTPUT_FOLDER, f"{title}-latency_by_priority_function.pdf"), format="pdf", bbox_inches="tight")


def plot_compare_insertion_rate(test_run):
    fig: plt.Figure = plt.figure(figsize=[2.7, 4.8])
    ax: plt.Axes = fig.subplots()
    indexes = ["octree_index", "sensor_pos_index"]
    test_runs = [{"config": test_run["config"], "index": test_run[index]} for index in indexes]
    xs = [0, 1]
    ys = make_y_insertion_rate(ax, test_runs)
    ax.bar(xs, ys, 0.7)
    plt.xticks(xs, indexes)
    fig.savefig(join(OUTPUT_FOLDER, f"compare_insertion_rate.pdf"), format="pdf", bbox_inches="tight")


def plot_compare_latency(test_run):
    fig: plt.Figure = plt.figure(figsize=[2.7, 4.8])
    ax: plt.Axes = fig.subplots()
    indexes = ["octree_index", "sensor_pos_index"]
    test_runs = [{"config": test_run["config"], "index": test_run[index]} for index in indexes]
    xs = [0, 1]
    draw_y_latency(ax, xs, test_runs, "index")
    plt.xticks(xs, indexes)
    fig.savefig(join(OUTPUT_FOLDER, f"compare_latency.pdf"), format="pdf", bbox_inches="tight")


def plot_compare_query_time(test_run):
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
    fig.savefig(join(OUTPUT_FOLDER, f"compare_queryperf.pdf"), format="pdf", bbox_inches="tight")


def plot_insertion_rates_by_disk_speed(data):
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
    fig.savefig(join(OUTPUT_FOLDER, f"compare_insertion_rate_by_disk_speed.pdf"), format="pdf", bbox_inches="tight")


def plot_latencies_by_disk_speed(data):
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
    fig.savefig(join(OUTPUT_FOLDER, f"compare_latency_by_disk_speed.pdf"), format="pdf", bbox_inches="tight")


def plot_latency_by_insertion_rate(data, index):
    fig: plt.Figure = plt.figure(figsize=[4.6, 4.8])
    ax: plt.Axes = fig.subplots()
    latency_runs = data["results"]["latency"]
    xs = [it["settings"]["points_per_sec"] for it in latency_runs]
    ax.set_xlabel("Insertion rate | points/s")
    draw_y_latency(ax, xs, latency_runs, False)
    fig.savefig(join(OUTPUT_FOLDER, f"{index}-latency_by_insertion_rate.pdf"), format="pdf", bbox_inches="tight")




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
