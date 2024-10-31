from os.path import join, dirname
import cbor2
from cbor2 import CBORDecodeEOF
from matplotlib import pyplot as plt
from collections import deque
import matplotlib.transforms as mtransforms

PROJECT_ROOT = join(dirname(__file__), "..")
INPUT_FILES = [join(PROJECT_ROOT, file) for file in [
    "evaluation/results/live_test_metrics/test1_metrics_0.cbor",
    "evaluation/results/live_test_metrics/test1_metrics_1.cbor",
    "evaluation/results/live_test_metrics/test2_metrics_0.cbor",
    "evaluation/results/live_test_metrics/test2_metrics_2.cbor",
    "evaluation/results/live_test_metrics/test2_metrics_3.cbor",
    "evaluation/results/live_test_metrics/test3_metrics_0.cbor",
    "evaluation/results/live_test_metrics/test4_metrics_0.cbor",
    "evaluation/results/live_test_metrics/test5-indoor-mit-viewer_metrics_0.cbor",
    "evaluation/results/live_test_metrics/test6_metrics_0.cbor",
    "evaluation/results/live_test_metrics/test7-auf-wagen_metrics_0.cbor",
    "evaluation/results/live_test_metrics/test8_metrics_0.cbor",
]]

# Constants for decoding the CBOR messages
FIELD_TIMESSTAMP = "t"
FIELD_METRIC = "m"
FIELD_VALUE = "v"
METRIC_NR_TASKS = "t"
METRIC_NR_POINTS = "p"
METRIC_NR_POINTS_ADDED = "a"


def main():
    for input_file in INPUT_FILES:

        # read file
        with open(input_file, 'rb') as f:
            data = []
            while True:
                try:
                    data.append(cbor2.load(f))
                except CBORDecodeEOF as e:
                    break
        print(f"Number of enties: {len(data)}")

        # let time start at 0
        # (time in the raw data starts at 0.0 when the lidarserver was started - usually there is a delay between
        # starting the server and actually beginning with capturing points)
        if len(data) == 0:
            continue
        time_start = min(d[FIELD_TIMESSTAMP] for d in data)
        time_end = max(d[FIELD_TIMESSTAMP] for d in data) - time_start
        for d in data:
            d[FIELD_TIMESSTAMP] -= time_start

        # plot
        fig: plt.Figure = plt.figure(figsize=(8.0, 4.5))  # 16:9 but smaller
        ax1: plt.Axes = fig.subplots()
        ax2: plt.Axes = ax1.twinx()
        _, capture_end = draw_data_interval(data, METRIC_NR_POINTS_ADDED, ax1, facecolor='grey', alpha=0.2)
        draw_metric(metric_moving_max(data, METRIC_NR_TASKS, 1.5), METRIC_NR_TASKS, ax1,
                    label="Nr Tasks, rolling maximum, window size = 1.5s", color="green", linewidth=1.0)
        draw_metric(metric_moving_max(data, METRIC_NR_POINTS, 1.5), METRIC_NR_POINTS, ax2,
                    label="Nr Points, rolling maximum, window size = 1.5s", color="blue", linewidth=1.0)
        ax1.set_ylabel("tasks", color="green")
        ax2.set_ylabel("points", color="blue")
        ax1.set_xlabel("time | s")
        ax1.set_ylim(bottom=0.0)
        ax2.set_ylim(bottom=0.0)
        fig.legend()
        fig.savefig(f"{input_file}.pdf", format="pdf", bbox_inches="tight", metadata={"CreationDate": None})

        detail_views = [
            (40.0, 42.0, "detail_1"),
            (capture_end - .5, time_end, "detail_remaining_tasks"),
        ]
        for xmin, xmax, name in detail_views:
            data_subset = [d for d in data if xmin - 2.0 < d[FIELD_TIMESSTAMP] < xmax + 2.0]
            fig: plt.Figure = plt.figure(figsize=(8.0, 4.5))  # 16:9 but smaller
            ax1: plt.Axes = fig.subplots()
            ax2: plt.Axes = ax1.twinx()
            draw_data_interval(data_subset, METRIC_NR_POINTS_ADDED, ax1, facecolor='grey', alpha=0.2)
            draw_metric_step(data_subset, METRIC_NR_TASKS, ax1, label="Nr Tasks", color="green", linewidth=1.0)
            draw_metric_step(data_subset, METRIC_NR_POINTS, ax2, label="Nr Points", color="blue", linewidth=1.0)
            ax1.set_ylabel("tasks", color="green")
            ax2.set_ylabel("points", color="blue")
            ax1.set_xlabel("time | s")
            fig.legend()
            ax1.set_xlim(xmin, xmax)
            fig.savefig(f"{input_file}.{name}.pdf", format="pdf", bbox_inches="tight", metadata={"CreationDate": None})


def draw_metric(data, metric, ax, **kwargs):
    metric_data = sorted((d for d in data if d[FIELD_METRIC] == metric), key=lambda d: d[FIELD_TIMESSTAMP])
    xs = [d[FIELD_TIMESSTAMP] for d in metric_data]
    ys = [d[FIELD_VALUE] for d in metric_data]
    ax.plot(xs, ys, **kwargs)


def draw_metric_step(data, metric, ax, **kwargs):
    metric_data = sorted((d for d in data if d[FIELD_METRIC] == metric), key=lambda d: d[FIELD_TIMESSTAMP])
    xs = [d[FIELD_TIMESSTAMP] for d in metric_data]
    ys = [d[FIELD_VALUE] for d in metric_data]
    ax.step(xs, ys, where="post", **kwargs)


def draw_data_interval(data, metric, ax, **kwargs):
    metric_data = [d for d in data if d[FIELD_METRIC] == metric]
    start = min(d[FIELD_TIMESSTAMP] for d in metric_data)
    end = max(d[FIELD_TIMESSTAMP] for d in metric_data)
    trans = mtransforms.blended_transform_factory(ax.transData, ax.transAxes)
    ax.fill_between([start, end], 0, 1, transform=trans, **kwargs)
    return start, end


def metric_moving_max(data, metric, window_size_seconds):
    metric_data = sorted((d for d in data if d[FIELD_METRIC] == metric), key=lambda d: d[FIELD_TIMESSTAMP])
    window = deque()
    result = []
    for data_point in metric_data:
        window.append(data_point)
        while window[0][FIELD_TIMESSTAMP] <= data_point[FIELD_TIMESSTAMP] - window_size_seconds:
            window.popleft()
        maximum = max([d[FIELD_VALUE] for d in window])
        result.append({
            FIELD_METRIC: metric,
            FIELD_TIMESSTAMP: data_point[FIELD_TIMESSTAMP],
            FIELD_VALUE: maximum
        })
    return result


if __name__ == '__main__':
    main()
