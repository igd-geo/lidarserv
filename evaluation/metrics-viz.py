from os.path import join, dirname
import cbor2
from cbor2 import CBORDecodeEOF
from matplotlib import pyplot as plt

PROJECT_ROOT = join(dirname(__file__), "..")
INPUT_FILES = [join(PROJECT_ROOT, file) for file in [
    "data/mno/metrics_0.cbor",
]]

# Constants for decoding the CBOR messages
FIELD_TIMESSTAMP = "t"
FIELD_METRIC = "m"
FIELD_VALUE = "v"
METRIC_NR_TASKS = "t"
METRIC_NR_POINTS = "p"


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

        # plot
        fig: plt.Figure = plt.figure()
        ax1: plt.Axes = fig.subplots()
        ax2: plt.Axes = ax1.twinx()
        draw_metric(data, METRIC_NR_TASKS, ax1, label="Nr Tasks", color="green", linewidth=1.0)
        draw_metric(data, METRIC_NR_POINTS, ax2, label="Nr Points", color="blue", linewidth=1.0)
        ax1.set_ylabel("tasks", color="green")
        ax2.set_ylabel("points", color="blue")
        ax1.set_xlabel("time | s")
        fig.legend()
        plt.show()


def draw_metric(data, metric, ax, **kwargs):
    metric_data = sorted((d for d in data if d[FIELD_METRIC] == metric), key=lambda d: d[FIELD_TIMESSTAMP])
    xs = [d[FIELD_TIMESSTAMP] for d in metric_data]
    ys = [d[FIELD_VALUE] for d in metric_data]
    ax.plot(xs, ys, **kwargs)



if __name__ == '__main__':
    main()
