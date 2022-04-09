# Lidar Serv

![](img/mno.mp4)
![](img/bvg.mp4)

## Table of Contents

[[_TOC_]]

## Building

The project is built with cargo:

```shell
cargo build --release --all
```

The project consists of several binaries. You can build and run specific binaries using the `--bin` argument. 
Make sure to always use release mode, as debug mode will usually be too slow.

Example: 

```shell
cargo run --release --bin lidarserv-server -- --help
```

Overview of the included binaries:

| Binary              | Description                                                              |
|---------------------|--------------------------------------------------------------------------|
| lidarserv-server    | The server                                                               |
| lidarserv-viewer    | Client that connects to the server and visualizes the served point cloud |
| velodyne-csv-replay | Simulates a LiDAR scanner that sends point data to the server            |
| evaluation          | Evaluation for the index data structures                                 |

If you are working with these tools a lot, it might be helpful to install them into your system
so that you don't have to repeat the full cargo command every time:

```shell
cargo install --path ./lidarserv-server
cargo install --path ./lidarserv-viewer
cargo install --path ./velodyne-csv-replay
cargo install --path ./evaluation
```

## Tutorial

### Create a new indexed point cloud

The lidar server is the main component, that manages the point cloud. Any point cloud project is started, by initializing a new index:

```shell
lidarserv-server init my-pointcloud
```

This will create a new empty point cloud in the folder `my-pointcloud`. Since we did not specify any additional parameters, the default settings will be used.

You can pass a few options to the init command to configure the point cloud indexer. In order to get a full list of the supported options, run `lidarserv-server init --help`.
The most important ones are:

| Option            | Description                                                   |
|-------------------|---------------------------------------------------------------|
| `--index mno`     | Uses the octree index structure for indexing the point cloud. |
| `--index bvg`     | Uses the sensor position index for indexing the point cloud.  |
| `--num-threads 4` | The number of threads to use for indexing.                    |

The options are stored in `my-pointcloud/settings.json`. You can change the options later by editing this file. 
However, note that not all options can be changed after the index has been created.

### Start the lidar server

After creating a point cloud, we can start the server like so:

```shell
lidarserv-server serve my-pointcloud
```

If needed, you can use the optional parameters `-h` and `-p` to bind to a specific host and port number. The default is to listen on `::1` (IPv6 loopback address), port `4567`.

### Insert points

The point cloud that is currently being served is still empty. In order to insert points, a LiDAR scanner 
can connect and stream in its captured points to the server. The server will then index and store the received 
points.

Here, we will use the `velodyne-csv-replay` tool to emulate a LiDAR scanner by replaying a previously captured LiDAR 
dataset. The dataset consists of two csv files, `trajectory.txt` and `points.txt`. Please refer to section [CSV LiDAR captures](#csv-lidar-captures) for an in depth description of the file formats. Here is an example for how the contents of the two files look:

`trajectory.txt`:
```csv
Timestamp Distance Easting Northing Altitude Latitude Longitude Altitude Roll Pitch Heading Velocity_Easting Velocity_Northing Velocity_Down
1720 0.0 412880.0778701233 5318706.438465869 293.18469233020437 48.015708664806255 7.831739127285349 293.18469233020437 14.30153383419094 -4.994178990936039 -110.9734934213208 -0.06 -0.023 0.013000000000000001
1720 0.0 412880.0778701233 5318706.438465869 293.18469233020437 48.015708664806255 7.831739127285349 293.18469233020437 14.30153383419094 -4.994178990936039 -110.9734934213208 -0.06 -0.023 0.013000000000000001
1720 0.0 412880.0778701233 5318706.438465869 293.18469233020437 48.015708664806255 7.831739127285349 293.18469233020437 14.30153383419094 -4.994178990936039 -110.9734934213208 -0.06 -0.023 0.013000000000000001
```

`points.txt`:
```csv
Timestamp point_3d_x point_3d_y point_3d_z Intensity Polar_Angle
1707.49593 1.01496763 0.727449579 -0.220185889 0.137254902 395.63 
1707.49593 0.998263499 0.715015937 0.0143595437 0.141176471 395.6125 
1707.49594 1.00160911 0.71694949 -0.187826059 0.121568627 395.595 
```

As a first step, we convert this dataset to a `*.laz` file. The resulting LAZ file can be used to replay the point data more efficiently. With the LiDAR server running, execute the following command:

```shell
velodyne-csv-replay convert --points-file /path/to/points.txt --trajectory-file /path/to/trajectory.txt -x 412785.340004 -y 5318821.784996 -z 290.0 --fps=5 --output-file preconv.laz
```

| Option                                                      | Description                                                                                                                       |
|-------------------------------------------------------------|-----------------------------------------------------------------------------------------------------------------------------------|
| `--points-file points.txt --trajectory-file trajectory.txt` | Input files                                                                                                                       |
| `--output-file preconf.laz`                                 | Output file                                                                                                                       |
| `-x 412785.340004 -y 5318821.784996 -z 290.0`               | Moves the point cloud, so that the given coordinate becomes the origin.                                                           |
| `--fps 5`                                                   | Frames per second, at which the point will be replayed later. Higher fps values lead to more frames with fewer points per frame.  |

This produces the file `preconf.laz`.

Note, that the files produced by `velodyne-csv-replay convert` are no ordinary LAZ files. They contain trajectory information for each point and use specific scale and shift values in the LAS header that the server requested. This means, that it is not possible, to use arbitrary LAZ files - you have to either use the conversion tool or build your own LAZ files according to the rules in section [Preprocessed LAZ file](#preprocessed-laz-file).

We can now send the point cloud to the LiDAR server with the following command:

```shell
velodyne-csv-replay replay --fps 5 preconv.laz
```

This will stream the contents of `preconv.laz` to the LiDAR server, in the same speed, that the points originally got captured by the sensor.

### View the point cloud

While the replay command is still running, we can start the viewer to get a live visualisation of the growing point cloud:

```shell
lidarserv-viewer
```

## Evaluation

The evaluation is a two-step process: First, the `evaluation` executable is used to measure the performance of the index with varying parameters. The results are saved in a `*.json` file, from which the python script `eval-results-viz.py` generates various diagrams that can be used in a publication.

The `evaluation` executable takes the path to a configuration file as its first and only argument. The configuration file is used to define, 
which tests to run:

```shell
evaluation example.toml
```

```toml
# FILE example.toml
data_folder = "data/evaluation"
output_file = "evaluation/results/evaluation_%d_%i.json"
points_file = "data/20210427_messjob/20210427_mess3/IAPS_20210427_162821.txt"
trajectory_file = "data/20210427_messjob/20210427_mess3/trajectory.txt"
offset = [412785.340004, 5318821.784996, 290.0]

[defaults]
type = "Octree"
priority_function = "NrPointsWeightedByTaskAge"
num_threads = 4
cache_size = 500
node_size = 10000
compression = true
nr_bogus_points = [0, 0]
insertion_rate.target_point_pressure = 1_000_000
query_perf.enable = true
latency.enable = true
latency.points_per_sec = 300000
latency.frames_per_sec = 50

[runs.example]
compression = [true, false]
cache_size = [8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096]
```

The file begins with a few mandatory file paths:

 - `data_folder`: Working directory where the indexes are stored.
 - `output_file`: File that the results will be written to after the evaluation finished. You should double-check this filename, because it can be quite disappointing if you run an evaluation for several hours and in the end all results are lost because the results file cannot be written. The filename can contain two placeholders:
   - `%d` Will be replaced by the current date.
   - `%i` Will be replaced by an ascending sequence of numbers. (Each time the evaluation is executed, `%i` is incremented by 1)
 - `points_file`, `trajectory_file`: Files containing the point data to use for the tests.
 - `offset`: New origin of the point data.

The evaluation consists of several runs. Each run is configured in its own section `[runs.*]`. The example above only 
defines a single run called `example`, in the `[runs.example]` section. Each run will execute tests for all possible 
combinations of values defined in its section. In the example above, each of the listed `cache_size` values will be 
tested both with compression enabled and disabled. The `[defaults]` section can be used to configure default 
values for the full evaluation across all for all runs.

The following table gives an overview of all keys, that can occur in a `[runs.*]` 
section or in the `[defaults]` section:

| Key                                    | Type                 | Description                                                                                                                                                |
|----------------------------------------|----------------------|------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `type`                                 | String               | Which index structure to test. Possible values: `"Octree"` or `"SensorPosTree"`                                                                            |
| `num_threads`                          | Integer              | Number of worker threads used for indexing.                                                                                                                |
| `cache_size`                           | Integer              | Number of nodes, that fitr into the node LRU cache.                                                                                                        |
| `compression`                          | Boolean              | Weather to enable LasZIP compression (`*.laz`) for storing the point data.                                                                                 |
| `priority_function`                    | String               | (Octree index:) The priority function that the octree index uses. Possible values: `"NrPoints"` or `"Lod"` or `"TaskAge"` or `"NrPointsWeightedByTaskAge"` |
| `nr_bogus_points`                      | \[Integer, Integer\] | (Octree index:) The maximum number of bogus points that a node can store, for inner nodes and leaf nodes, respectively.                                    |
| `node_size`                            | Integer              | (Sensor pos index:) Number of points, at which a node split is performed.                                                                                  |
| `insertion_rate.target_point_pressure` | Integer              | Always fill up the internal buffers to this number of points.                                                                                              |
| `latency.enable`                       | Boolean              | Weather to do the latency measurement                                                                                                                      |
| `latency.points_per_sec`               | Integer              | How quickly to insert points when measuring the latency.                                                                                                   |
| `latency.frames_per_sec`               | Integer              | How many times per second to insert points when measuring the latency,                                                                                     |
| `query_perf.enable`                    | Boolean              | Weather to measure the query performance                                                                                                                   |

There are three kind of performance measurements, that the evaluation executable can do:

 - Insertion rate: The insertion rate is always measured. It is the indexing throughput that the index can archive, measured in points per second. To measure the insertion rate, we have to consider the number of points that are currently "waiting for insertion" - points that have been passed to the indexer but not stored in a node yet. For the octree index, these are for example the points in the inboxes of all nodes. To measure the insertion rate, we repeatedly pass as many points to the indexer as needed to top up the waiting points to a certain fixed number (the `insertion_rate.target_point_pressure` parameter). Two times are measured: In the json results file, `duration_seconds` is the time needed to pass all points to the index, `duration_cleanup_seconds` is the time to process the remaining waiting points and write all cached nodes to disk.
 - Query performance: Measures the execution time for a query on the index. Three different queries are tested, that at the moment are hard-coded in `evaluation/src/queries.rs`. The queries re-use the index that has been built when measuring the insertion rate, and are executed after the insertion process has completed.
 - Latency: Measures the time between points being passed to the index and them becoming visible in queries. This test inserts points into the index at a fixed rate, controlled by the `latency.points_per_sec` and `latency.frames_per_sec` parameters. If the indexing throughput measured during the insertion rate test is not high enough for `latency.points_per_sec`, then the latency test will be skipped. Concurrently to inserting the points, a second thread executes a query. For each point, the time stamp when it was passed to the index and when we first see it in a query is recorded to calculate this points latency value. The results contain various statistics of all point latencies (mean, median, percentiles).

In order to visualize the results, the python script at `evaluation/eval-results-viz.py` can be used. It needs python 3.6 or newer, as well as the matplotlib library. You can either install matplotlib manually (`pip install matplotlib`), or use the [pipenv](https://pipenv.pypa.io/en/latest/) file at the root of this repository:

```shell
pipenv install
pipenv run python evaluation/eval-results-viz.py
```

The script takes no parameters. You will need to modify the constants at the top to specify the path to the input `*.json` file. Various types of diagrams can be generated easily using the `plot_XXX_by_YYY` helper functions. Just tweak the main function depending on which diagrams you want. For the input file `path/to/data.json`, a folder named `path/to/data.json.diagrams` will be created containing the rendered diagrams as pdf.

## CSV LiDAR captures

## Protocol

This section describes the communication protocol used by the LiDAR Server and its clients. It contains all information necessary to develop additional client applications interacting with the LiDAR Server.

Through this protocol, it is possible to
 - Stream points to the server for insertion into the point cloud.
 - Access the point cloud by subscribing to queries.
 
The protocol has no built-in security (authentication & authorisation, encryption, ...). Make sure, that only trusted clients can access the server.

### Binary layer

After the TCP connection is established, each peer sends the following 18 byte magic number. 
By verifying the magic number sent by the server, the client can make sure, that it is indeed connected to a compatible
LiDAR Server speaking the same protocol, and not some other arbitrary network service.

| Index | Length | Type        | Field                                                                                                            |
|-------|--------|-------------|------------------------------------------------------------------------------------------------------------------|
| 0     | 18     | Binary data | Magic number. <br/>HEX: "4C 69 64 61 72 53 65 72 76 20 50 72 6F 74 6F 63 6F 6C" <br/>ASCII: "LidarServ Protocol" |


After this, the connection is regarded as established. Further protocol version compatibility checking will be done as 
the first message in the messages layer.

The remaining communication consists of message frames, sent in both direction (client to server or server to client).

| Index | Length | Type                                   | Field                |
|-------|--------|----------------------------------------|----------------------|
| 0     | 8      | Unsigned 64-Bit Integer, little endian | Message size (`len`) |
| 8     | `len`  | Binary data                            | CBor encoded message |

### Message layer

The messages sent on the binary layer are [CBOR](https://cbor.io/) encoded message objects. Since CBOR is a binary data format, we will use JSON or [CBOR Extended Diagnostic Notation](https://www.rfc-editor.org/rfc/rfc8610#appendix-G) in this documentation to format message contents.

The protocol begins with an initialisation phase. After the initialisation is complete, it can continue in two different protocol modes: 
 - CaptureDevice Mode: New points are streamed from the client to the server, that will be added to the point cloud.
 - Viewer Mode: The client can subscribe to a query. The server will return the matching subset of the point cloud and incrementally update the query result, whenever new points are indexed.
 
#### Message: Hello

First, both the server and the client send a `Hello` message to each other:

```json
{
  "Hello": {
    "protocol_version": 1
  }
}
```

The message contains the protocol version, that the peer speaks. After the Hello messages have been exchanged, the compatibility between both protocol versions is determined. If one of the peers deems the protocol versions as incompatible, the connection is closed again.

If both peers speak the same protocol version, they are obviously compatible to each other. If the protocol versions are different, there is still the chance, that the newer version is backwards compatible to the older version. Since the peer speaking the older protocol version usually has no knowledge about newer protocol versions and their backwards compatibility, it is always the task of the peer with the newer protocol version to determine if the protocol versions are compatible.

Therefore, if the client receives a Hello message from the server, it should behave like this:

 - If `server_protocol_version == own_protocol_version` - Client and server use the same protocol version: Continue normally.
 - If `server_protocol_version < own_protocol_version` - Client is connected to an older server: The client should test, if it is backwards compatible to the server's protocol version. If it is, continue normally. Otherwise: Close the connection.
 - If `server_protocol_version > own_protocol_version` - Client is connected to a newer Server: The server will check, if it can be backwards compatible to the client's protocol version. The client should continue normally, but expect the server to close the connection.

The current protocol version (that is described in this document) has the version number `1`.

#### Message: PointCloudInfo

After the Hello messages have been exchanged, the server sends some general metadata about the point cloud, that it manages.

```json
{
  "PointCloudInfo": {
    "coordinate_system": {
      "I32CoordinateSystem": {
        "scale": [1.0, 1.0, 1.0],
        "offset": [0.0, 0.0, 0.0]
      }
    }
  }
}
```

As of now, the metadata only contains the `coordinate_system` used by the server for storing/transmitting point data. Also, `I32CoordinateSystem` is the only supported type of coordinate system. The `scale` and `offset` values define the transformation between the `i32` representation of the coordinates and the actual global coordinates.

This metadata is especially important for the CaptureDevice mode, as any LAS-encoded point data sent to the server **MUST** use these values for the corresponding fields in the LAS file header.

#### Message: ConnectionMode

The initialisation phase ends with the client sending the protocol mode to the server, that it wishes to proceed with.

Switch to CaptureDevice mode:

```json
{
  "ConnectionMode": {
    "device": "CaptureDevice"
  }
}
```

Switch to Viewer mode:

```json
{
  "ConnectionMode": {
    "device": "Viewer"
  }
}
```

#### CaptureDevice mode

In CaptureDevice mode, the client streams point data to the server, that will be indexed and added to the point cloud.

![](img/mermaid-diagram-insert-points.svg)
[Diagram source](https://mermaid-js.github.io/mermaid-live-editor/edit/#pako:eNqNkcFqwzAMhl_F-Jy9QA6F0QwW2EZZrrkIW93EHCmz5cIoffc5cwotg1KfpF_fb8nW0TrxaFub8DsjO-wIPiJMI5tyZohKjmZgNS_UPb6bATlJ_F8dMB5w1d9E0UhJrzxNRVrTMylBoARKwtVyCT5sNmf0GUOQSlSp1C7Re4idEOs2SPY97-Vmu60wo1umei2fUnKYNUfs8EAO73rblcMst1RbEJlrdKN9zwmj_s2bKozsbWMnjBOQL0s6LvJo9RMnHG1bQg_xa7QjnwqXZw-KT55Uom33EBI2FrLK8MPOthoznqF1yyt1-gUsHK6Y)

##### Message: InsertPoints

In the CaptureDevice mode, the client repeatedly sends InsertPoints messages to the server:

```edn
{
  "InsertPoints": {
    "data": h'DE AD BE EF' /binary LAS-encoded point data/
  }
}
```

The point data is encoded in the LAS format. The data may (but does not have to) be LasZIP compressed. The value for scale and offset in the LAS header **MUST** match the values that the server provided as part of the PointCloudInfo message. See section [Encoding of point data](#encoding-of-point-data) for further encoding rules.

#### Viewer mode

In Viewer mode, the client can subscribe to a query. The server will return the query result and keep it up-to-date as new points are added to the point cloud.

The client starts with an empty query result. The server will send a series of `IncrementalResult` messages that contain 
instructions for the client of how to update the current query result. Like this, the server keeps the 
client-side query result in sync with the actual query result, updating it whenever new points are added to the point 
cloud, or after the query has been changed by the client.

![](img/mermaid-diagram-query.svg)
[Diagram source](https://mermaid-js.github.io/mermaid-live-editor/edit/#pako:eNp9kk1OwzAQha8SeUt6AS8qoYJEF6BCJVbZjOwpWHVmgjMGVVXvjqNJiCIoXo3f-8bPf2fj2KOxpsePjOTwLsBbgrahqowOkgQXOiCp9pg-Mf3WXwN-TfoTC1ZcsJGu1bTVloIEiKEHCUwKq7dar5W11QPGyOqpVLxpgf-8HQeSTeTst3TgK4tvmAjdkP5YDmyvbFvVemrSaTV0KBmZO63-DHnOmE6zv6QXe7-Zb8YlbJEE4gv2Ocqy4SdiNWYodOuOM4fkdVIKU5sWUwvBlzc9D3Jj5L0ENMaW0kM6NqahS-Fy50Hw3gfhZOwBYo-1gSy8P5EzVlLGCRo_xUhdvgF9-7pg)

##### Message: Query

The first action for the client after entering the Viewer protocol mode is to subscribe to a query using the `Query` message. At any later point in time, the client can re-send the query
 message in order to update the query subscription.

Two types of queries are supported: Aabb Queries select all points of a fixed LOD within a certain bounding box. 
View Frustum Queries select all points inside the View Frustum of a virtual camera, with LODs depending on the 
distance to the camera, so that a fixed point density is reached on screen.

AABB-Queries:
```json
{
  "Query": {
    "AabbQuery": {
      "min_bounds": [0.0, 0.0, 0.0],
      "max_bounds": [5.0, 5.0, 5.0],
      "lod_level": 5
    }
  }
}
```

ViewFrustumQuery-Queries:
```json
{
  "Query": {
    "ViewFrustumQuery": {
      "view_projection_matrix": [
        1.2840488017219491,     1.045301427691423e-16, 4.329788940746688e-17, 4.3297802811774664e-17,
        -7.862531274888896e-17, 1.7071067811865472,    0.707108195401524,     0.7071067811865476,
        -7.913561719330721e-33, 1.7071067811865475,    -0.7071081954015239,   -0.7071067811865475,
        1.1917566422486195e-29, 0.0,                   667.9741663425807,     681.6049150647954
      ],
      "view_projection_matrix_inv": [
        0.7787865995894931,     -4.76869258203062e-17, -4.7996429837981346e-33, 0.0,
        1.7934537145592993e-17, 0.2928932188134524,    0.2928932188134525,      0.0,
        2.1648879756985935e-15, 35.35530370398832,     -35.35530370398832,      -0.07335620517825471,
        -2.1215945026671e-15,   -34.64826763347989,    34.64826763347988,       0.07335635189081177
      ],
      "window_width_pixels": 500.0,
      "min_distance_pixels": 10.0
    }
  }
}
```

Here, the `view_projection_matrix` projects points from world space into clip space which ranges from -1 to 1 on all 
three axes. The coordinate system is oriented such that smaller z values are closer to the camera. For example, the min distance plane is at `z = -1`. 

The `view_projection_matrix_inv` is the inversion of the projection matrix and projects coordinates from clip space back into world space.

The values `window_width_pixels` and `min_distance_pixels` control the point density on screen. 
The `window_width_pixels` is used for the conversion from clip space (`[-1, 1]`) to screen space (`[0, window_width_pixels]`).
The query engine will keep loading more LODs, until the distance between neighbouring points on screen is smaller than `min_distance_pixels`.
As a result, two neighbouring points on screen will be closer than `min_distance_pixels` pixels, making `min_distance_pixels`
actually a maximum value / upper bound. The `min` in `min_distance_pixels` refers to it being the minimum value at which 
the query engine keeps loading finer LODs.

##### Message: IncrementalResult

The server sends updates to the query result using the `IncrementalResult` message.

In general, the query result consists out of a set of nodes. 
Each node is identified by its lod (level of detail) and a 14 byte id. Note, that the id value alone does not uniquely identify a node, it only identifies a node within its level of detail. 
The point data of a node is stored in the LAS or LAZ format. One node might consist of multiple las files. Having more than one LAS/LAZ file is an edge-case that should not happen frequently though. The normal behaviour is, that each node has exactly one point data file.

The `IncrementalResult` has two fields: The field `nodes` contains a list of nodes with their point data that should be added to the query result. The `replaces` field optionally contains a node, that should be removed from the query result.

Add node:
```json
{
 "IncrementalResult": {
  "replaces": null,
  "nodes": [
   [
    {"lod_level": 3, "id":  h'00 00 00 00 00 00 00 00 00 00 00 00 00 00 /14 byte node id/'}, 
    [h'DEAD BEEF /LAS point data/']
   ]
  ]
 }
}
```

Remove node:
```json
{
 "IncrementalResult": {
  "replaces": {"lod_level": 3, "id":  h'01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 0E /14 byte node id/'},
  "nodes": []
 }
}
```

Reload node: (after points have been added - basically replaces the node by the new version of itself)
```json
{
 "IncrementalResult": {
  "replaces": {"lod_level": 3, "id":  h'01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 0E /14 byte node id/'},
  "nodes": [
   [
    {"lod_level": 3, "id":  h'01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 0E /14 byte node id/'}, 
    [h'DEAD BEEF /LAS point data/']
   ]
  ]
 }
}
```

Split node: (specific to the sensor position index)
```json
{
 "IncrementalResult": {
  "replaces": {"lod_level": 3, "id":  h'01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 0E /14 byte node id/'},
  "nodes": [
   [
    {"lod_level": 3, "id":  h'E1 E1 E1 E1 E1 E1 E1 E1 E1 E1 E1 E1 E1 E1 /14 byte node id/'}, 
    [h'DEAD BEEF /LAS point data/']
   ],
   [
    {"lod_level": 3, "id":  h'F2 F2 F2 F2 F2 F2 F2 F2 F2 F2 F2 F2 F2 F2 /14 byte node id/'}, 
    [h'DEAD BEEF /LAS point data/']
   ],
   [
    {"lod_level": 3, "id":  h'3A 3A 3A 3A 3A 3A 3A 3A 3A 3A 3A 3A 3A 3A /14 byte node id/'}, 
    [h'DEAD BEEF /LAS point data/']
   ]
  ]
 }
}
```

##### Message: ResultAck

The `ResultAck` message exists to ensure, that the server does not send `IncrementalResult`s faster than the client can receive and process. 

The TCP/IP connection between server and client acts as a fifo buffer, where multiple messages can be "in flight". If the client updates the query, it still has to process all in-flight messages before it sees the first `IncrementalResult`, that respects the new query. If there are many in-flight messages, this can lead to visible delays in the client application. To avoid this, the server throttles its `IncrementalResult` messages so that the number of in-flight messages is limited to 10 or less.

To know the number of in-flight messages, the server keeps track of how many `IncrementalResult` messages it has sent out and subtracts the number of `IncrementalResult` messages that the client has processed. The `ResultAck` message tells the server, how many `IncrementalResult` messages the client has processed. The client should keep track of this number and periodically send it to the server - optimally after every processed update, but at least after each 10 updates.

```json
{
 "ResultAck": {
  "update_number": 123
 }
}
```

## Encoding of point data

- LAS 1.2
- trajectory extra data

## Usages

### `lidarserv-server`

```
A tool to index and query lidar point clouds, in soft real time

USAGE:
    lidarserv-server [OPTIONS] <SUBCOMMAND>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
        --log-level <log-level>    Verbosity of the command line output [default: info]  [possible values: trace, debug, info, warn, error]

SUBCOMMANDS:
    help     Prints this message or the help of the given subcommand(s)
    init     Initializes a new point cloud
    serve    Runs the indexing server
```

#### `lidarserv-server init` subcommand

```
Initializes a new point cloud

USAGE:
    lidarserv-server init [FLAGS] [OPTIONS] [path]

FLAGS:
    -h, --help                  Prints help information
        --las-no-compression    Disables laz compression of point data
    -V, --version               Prints version information

OPTIONS:
        --bvg-max-points-per-node <bvg-max-points-per-node>    The maximum number of points that can be inserted into a node, before that node is split. This option only applies to the bvg index [default: 100000]
        --cache-size <cache-size>                              Maximum number of nodes to keep in memory, while indexing [default: 500]
        --index <index>                                        Index structure to use so the point cloud can be queried efficiently [default: mno]  [possible values: mno, bvg]
        --las-offset <las-offset>                              The offset used for storing point data. (usually fine to be left at '0.0, 0.0, 0.0') [default: 0]
        --las-scale <las-scale>                                The resolution used for storing point data [default: 0.001]
        --max-lod <max-lod>                                    Maximum level of detail of the index [default: 10]
        --mno-node-grid-size <mno-node-grid-size>              The size of the nodes at the coarsest level of detail. With each finer LOD, the node size will be halved. This option only applies to the mno index [default: 1024.0]
        --mno-task-priority <mno-task-priority>                The order, in which to process pending tasks. This option only applies to the mno index [default: nr_points]  [possible values: nr_points, lod, newest_point, oldest_point, task_age]
        --num-threads <num-threads>                            Number of threads used for indexing the points [default: 4]
        --point-grid-size <point-grid-size>                    The distance between two points at the coarsest level of detail [default: 8.0]

ARGS:
    <path>    Folder, that the point cloud will be created in. By default, the current folder will be used
```

#### `lidarserv-server serve` subcommand

```
Runs the indexing server

USAGE:
    lidarserv-server serve [OPTIONS] [path]

FLAGS:
        --help       
            Prints help information

    -V, --version    
            Prints version information


OPTIONS:
    -h, --host <host>    
            Hostname to listen on [default: ::1]

    -p, --port <port>    
            Port to listen on [default: 4567]


ARGS:
    <path>    
            Folder, that the point cloud data will be stored in.
            Use the `init` command first, to initialize a new point cloud in that folder. By default, the current folder will be used.
```

### `lidarserv-viewer`

```
USAGE:
    lidarserv-viewer [OPTIONS]

FLAGS:
        --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -h, --host <host>                         [default: ::1]
        --log-level <log-level>              Verbosity of the command line output [default: info]  [possible values: trace, debug, info, warn, error]
        --point-color <point-color>           [default: fixed]  [possible values: fixed, intensity]
        --point-distance <point-distance>     [default: 10]
        --point-size <point-size>             [default: 10]
    -p, --port <port>                         [default: 4567]
```

### `velodyne-csv-replay`

```
USAGE:
    velodyne-csv-replay [OPTIONS] <SUBCOMMAND>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
        --log-level <log-level>    Verbosity of the command line output [default: info]  [possible values: trace, debug, info, warn, error]

SUBCOMMANDS:
    convert        Reads the csv files with point and trajectory data and converts them to a laz file, that can be used with the replay command
    help           Prints this message or the help of the given subcommand(s)
    live-replay    Replays the point data directly from the csv files containing the point and trajectory information. Calculation of point positions and encoding of LAZ data is done on-the-fly
    replay         Replays the given laz file. Each frame sent to the server at the given frame rate (fps) contains exactly one chunk of compressed point data from the input file
```

#### `velodyne-csv-replay convert` subcommand

```
USAGE:
    velodyne-csv-replay convert [OPTIONS] --output-file <output-file> --points-file <points-file> --trajectory-file <trajectory-file>

FLAGS:
        --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
        --fps <fps>                            Frames per second at which to store point data [default: 20]
    -h, --host <host>                          Host name for the point cloud server. The converter will briefly connect to this server to determine the correct settings for encoding the point data [default: ::1]
    -x, --offset-x <offset-x>                  The offset moves each point, such that (offset-x, offset-y, offset-z) becomes the origin [default: 0.0]
    -y, --offset-y <offset-y>                  See offset-x [default: 0.0]
    -z, --offset-z <offset-z>                  See offset-x [default: 0.0]
        --output-file <output-file>            Name of the output file
        --points-file <points-file>            Input file with the point data
    -p, --port <port>                          Port for the point cloud server. The converter will briefly connect to this server to determine the correct settings for encoding the point data [default: 4567]
        --speed-factor <speed-factor>          speeds up or slows down the reader by the given factor [default: 1.0]
        --trajectory-file <trajectory-file>    Input file with the sensor trajectory
```

#### `velodyne-csv-replay replay` subcommand

```
Replays the given laz file. Each frame sent to the server at the given frame rate (fps) contains exactly one chunk of compressed point data from the input file

USAGE:
    velodyne-csv-replay replay [OPTIONS] <input-file>

FLAGS:
        --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
        --fps <fps>      Frames per second at which to replay point data [default: 20]
    -h, --host <host>    Host name for the point cloud server [default: ::1]
    -p, --port <port>    Port for the point cloud server [default: 4567]

ARGS:
    <input-file>    Name of the file containing the point data
```

#### `velodyne-csv-replay live-replay` subcommand

```
Replays the point data directly from the csv files containing the point and trajectory information. Calculation of point positions and encoding of LAZ data is done on-the-fly

USAGE:
    velodyne-csv-replay live-replay [FLAGS] [OPTIONS] --points-file <points-file> --trajectory-file <trajectory-file>

FLAGS:
        --help              
            Prints help information

        --no-compression    
            Disables laz compression of point data

    -V, --version           
            Prints version information


OPTIONS:
        --fps <fps>                            
            Frames per second at which to send point data.
            
            Note: A higher fps will NOT send more points per second. It will just smaller packages of points being sent more frequently. [default: 20]
    -h, --host <host>                          
             [default: ::1]

    -x, --offset-x <offset-x>                  
            The offset moves each point, such that (offset-x, offset-y, offset-z) becomes the origin [default: 0.0]

    -y, --offset-y <offset-y>                  
            See offset-x [default: 0.0]

    -z, --offset-z <offset-z>                  
            See offset-x [default: 0.0]

        --points-file <points-file>            
            File with the point data

    -p, --port <port>                          
             [default: 4567]

        --speed-factor <speed-factor>          
            speeds up or slows down the reader by the given factor [default: 1.0]

        --trajectory-file <trajectory-file>    
            File with the sensor trajectory
```