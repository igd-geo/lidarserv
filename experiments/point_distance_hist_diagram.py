import matplotlib.pyplot as plt
import numpy as np


def main():
    # read file
    buckets = []
    with open("point_distance_hist.out.txt") as f:
        for line in f:
            count_str = line.split(" ")[1]
            buckets.append(int(count_str))

    prefix_sum = np.zeros(len(buckets))
    sum = 0
    for i in range(len(buckets)):
        sum += buckets[i]
        prefix_sum[i] = sum
    prefix_sum = prefix_sum / sum
    print(prefix_sum)

    x = 0.5 + np.arange(len(buckets))
    y = buckets
    fig, ax = plt.subplots()
    fig.set_figwidth(10)
    fig.set_figheight(4)
    ax.set_yscale('log')
    ax.set_xlabel('distance (m)')
    ax.set_ylabel('number of points')
    ax.bar(x, y, width=1, edgecolor="white", linewidth=1.0)
    ax.set(xticks=np.arange(0, len(buckets), 10))
    plt.show()



if __name__ == '__main__':
    main()
