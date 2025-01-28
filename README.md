# LidarServ

LidarServ is a real time indexer for LiDAR point clouds. It can index, query and visualize point clouds live while it is being recorded.

[Video](img/mno.mp4)

## Building

LidarServ uses the rust programming language. To install rust, please refer to the [official installation guide](https://www.rust-lang.org/tools/install).

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

| Binary               | Description                                                                       |
|----------------------|-----------------------------------------------------------------------------------|
| lidarserv-server     | The server                                                                        |
| lidarserv-viewer     | Client that connects to the server and visualizes the served point cloud          |
| lidarserv-query      | Client that sends queries to the server and stores the queried points as las file |
| lidarserv-input-file | Simulates a LiDAR scanner that sends point data to the server                     |
| lidarserv-evaluation | Evaluation for the index data structures                                          |

If you are working with these tools a lot, it might be helpful to install them into your system
so that you don't have to repeat the full cargo command every time:

```shell
cargo install --path ./lidarserv-server
cargo install --path ./lidarserv-viewer
cargo install --path ./lidarserv-query
cargo install --path ./lidarserv-input-file
cargo install --path ./lidarserv-evaluation
```

## Tutorial

This tutorial will guide you through the basics to get started with lidarserv.

### Prepare a dataset

In this tutorial, we will not use a real lidar scanner. Instead, we will simulate a scanner by replaying the points in a pre-recorded las file. As an example, we will be using a tile from the AHN dataset.

Download the file using the following commands:

```bash
mkdir data
cd data
wget -O ahn.laz https://ns_hwh.fundaments.nl/hwh-ahn/AHN3/LAZ/C_25BZ2.LAZ
```

The next step requires an uncompressed `las` file. You can use any tool of your liking to convert the `laz` file to `las`. The following command uses the `laszip` command from the LAStools ([GitHub](https://github.com/LAStools/LAStools), [Homepage](https://lastools.github.io/), [Download](https://rapidlasso.de/downloads/)) suite: 

```
laszip -i ahn.laz -o ahn.las
```

We later want to replay the points to lidarserv in the order that they have been captured by the lidar scanner. In many datasets, the points are not in their original order any more. Use the `sort` subcommand from `lidarserv-input-file` to sort the points by the GpsTime attribute:

```
lidarserv-input-file sort --output-file ahn-sorted.las ahn.las
```

### Create a new indexed point cloud

The lidar server is the main component, that manages the point cloud. Any point cloud project is started, by initializing a new index:

```shell
lidarserv-server init my-pointcloud
```

This will create a new empty point cloud in the folder `my-pointcloud`. 

![](img/lidarserv_init.svg)

It will interactively ask a few questions about the index to create:

```
################################################################################
# Point Format
################################################################################
✔ Select a point format preset: · LAS point format 1
You have added the following point attributes so far:
 - Position3D (Vec3<f64>)
 - Intensity (U16)
 - ReturnNumber (U8)
 - NumberOfReturns (U8)
 - ScanDirectionFlag (U8)
 - EdgeOfFlightLine (U8)
 - Classification (U8)
 - ScanAngleRank (I8)
 - UserData (U8)
 - PointSourceID (U16)
 - GpsTime (F64)
✔ Edit attributes: · Done.
✔ Point Data compression:  · None

################################################################################
# Coordinate System
################################################################################
✔ Scale X: · 1
✔ Scale Y: · 1
✔ Scale Z: · 1
✔ Offset X: · 0
✔ Offset Y: · 0
✔ Offset Z: · 0

################################################################################
# Octree
################################################################################
✔ Largest node size in metres: · 5000
✔ Finest point spacing in metres: · 0.01
✔ Sampling grid size: · 128
Based on your input, the following octree parameters have been calculated:
 - node hierarchy shift: 12
 - point hierarchy shift: 5
 - max level of detail: LOD12

With these parameters, the octree will have the following properties:
 - In LOD0, the node size is 4096.000 metres.
 - In LOD1, the node size is 2048.000 metres.
 - In LOD2, the node size is 1024.000 metres.
 - In LOD3, the node size is 512.000 metres.
 - In LOD4, the node size is 256.000 metres.
 - In LOD5, the node size is 128.000 metres.
 - In LOD6, the node size is 64.000 metres.
 - In LOD7, the node size is 32.000 metres.
 - In LOD8, the node size is 16.000 metres.
 - In LOD9, the node size is 8.000 metres.
 - In LOD10, the node size is 4.000 metres.
 - In LOD11, the node size is 2.000 metres.
 - In LOD12, the node size is 1.000 metres.
 - Each node contains a 128x128x128 sampling grid.
 - In LOD0, the point distance is 32.000 metres.
 - In LOD1, the point distance is 16.000 metres.
 - In LOD2, the point distance is 8.000 metres.
 - In LOD3, the point distance is 4.000 metres.
 - In LOD4, the point distance is 2.000 metres.
 - In LOD5, the point distance is 1.000 metres.
 - In LOD6, the point distance is 0.500 metres.
 - In LOD7, the point distance is 0.250 metres.
 - In LOD8, the point distance is 0.125 metres.
 - In LOD9, the point distance is 0.062 metres.
 - In LOD10, the point distance is 0.031 metres.
 - In LOD11, the point distance is 0.016 metres.
 - In LOD12, the point distance is 0.008 metres.
✔ Does this look acceptable to you? · yes

################################################################################
# Indexing
################################################################################
✔ Number of threads:  · 20
✔ Task priority function:  · NrPointsTaskAge
✔ Cache size (nodes):  · 5000
✔ Maximum number of bogus points per inner node: · 0
✔ Maximum number of bogus points per leaf node: · 0
✔ Should metrics be recorded during indexing? · no
✔ Would you like to add any attribute indexes? · no
```

The options are stored in `my-pointcloud/settings.json`. You can change the options later by editing this file. However, note that not all options can be changed after the index has been created.

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

Here, we will use the `lidarserv-input-file` tool to emulate a LiDAR scanner by replaying a previously captured LiDAR 
dataset. 

Use the following command to replay the points from the `ahn-sorted.las` file that we have prepared above:

```shell
lidarserv-input-file replay --points-per-second 500000 ahn-sorted.las
```

This will stream the contents of `ahn-sorted.las` to the LiDAR server with a point rate of 500K points per second.

```
[fps:  0 pps:       0][buffer: 100%][          | 0/510190296 points sent]
[fps: 20 pps:  499999][buffer: 100%][          | 499999/510190296 points sent]
[fps: 20 pps:  500000][buffer: 100%][          | 999999/510190296 points sent]
[fps: 20 pps:  500000][buffer: 100%][          | 1499999/510190296 points sent]
[fps: 20 pps:  500000][buffer: 100%][          | 1999999/510190296 points sent]
[fps: 20 pps:  500000][buffer: 100%][          | 2499999/510190296 points sent]
[fps: 20 pps:  500000][buffer: 100%][          | 2999999/510190296 points sent]
[fps: 20 pps:  500000][buffer: 100%][▏         | 3499999/510190296 points sent]
[fps: 20 pps:  500000][buffer: 100%][▏         | 3999999/510190296 points sent]
[fps: 20 pps:  500000][buffer: 100%][▏         | 4499999/510190296 points sent]
[fps: 20 pps:  500000][buffer: 100%][▏         | 4999999/510190296 points sent]
```

### View the point cloud

While the replay command is still running, we can start the viewer to get a live visualisation of the growing point cloud:

```shell
lidarserv-viewer
```

## Query language

LidarServ comes with a query language that can be used to select a subset of the indexed point cloud based on some criteria. 

It can for example be used with the `lidarserv-query` tool that executes the query and writes the query result to a `las` file. The following command will connect to the running LidarServ server, execute the query `full`, and write the result to `./query-result.las`. This will basically create a dump of the entire point cloud.

```bash
lidarserv-query --outfile ./query-result.las "full"
```

The following syntax elements can be used in the query language:

 - `full`: Matches any point.
 - `empty`: Matches no point.
 - `lod(x)`: Matches any point from level of detail x or lower. (x is a positive integer)
 - `aabb([xmin, ymin, zmin], [xmax, ymax, zmax])`: Matches any point within the given bounding box. (xmin, ymin, zmin, xmax, ymax, zmax are floating point numbers)
 - Attribute queries select points based on the value of some point attribute. The following kinds of attribute queries exist:
    - Select all points with the given attribute value:
        ```
        attr(attribute == val)
        ``` 
    - Select all points, where the attribute value is larger / smaller than the provided value:
        ```
        attr(attribute < val)
        attr(attribute <= val)
        attr(attribute > val)
        attr(attribute >= val)
        ```
    - Select all points, where the attribute value is within the given range:
        ```
        attr(val1 < attribute < val2)
        attr(val1 < attribute <= val2)
        attr(val1 <= attribute < val2)
        attr(val1 <= attribute <= val2)
        ```
    - Select all points that have a different attribute value:
        ```
        attr(attribute != val)
        ``` 
    In these examples, `attribute` is always the name of a point attribute, like `Intensity` or `Classification`. Note that attribute names are case sensitive. The placeholders `val`, `val1` and `val2` have to match the datatype of the queried attribute. For example, the classification is usually a positive integer, but the gps time can be a floating point number. Attributes with vector data types such as point colors or normals are written using square brackets: `attr(ColorRGB < [75, 75, 75])`

 - View frustum queries take a set of camera parameters. They will only match points that are in the cameras view frustum. Points closer to the camera will have a higher level of detail, points further away will have a lower level of detail, so that after perspective projection, the given `max_distance` between points in pixels is upheld.

    ```
    view_frustum(
        camera_pos: [x1, y1, z1],
        camera_dir: [x2, y2, z2],
        camera_up: [x3, y3, z3],
        fov_y: f4,
        z_near: f5,
        z_far: f6,
        window_size: [x7, y7] ,
        max_distance: f8
    )
    ```
    (x*, y*, z*, f* are all floating point numbers.)
 - `(q)`: Brackets around a (sub)query can be used to override the order of operator precedence. The default operator precedence is: brackets > not > and > or.
 - `!q`: Inverts the query q. Matches any point that is not matched by q and vice-versa.
 - `q1 and q2`: Matches the intersection of queries q1 and q2.
 - `q1 or q2`: Matches the union of queries q1 and q2.

### Example queries

The following example queries have been tested with the same point cloud that we also used in the tutorial above.

Get an overview of the entire point cloud at a lower resolution:

```
lod(2)
```

Get a part of the point cloude in a small bounding box:

```
aabb(
    [116027.90, 492946.03, -50.0],
    [116396.33, 493259.96, 50.0]
)
```

Get a part of the point cloud in a (larger) bounding box at a lower resolution:

```
lod(2) and aabb(
    [115003.95, 491100.58, -50.0],
    [119956.85, 493163.37, 50.0]
)
```

Select all points with classification 26 (bridges). **Warning**: The tutorial above does not create attribute indexes, in which case this query takes really long to execute. You can make it faster by adding attribute indexes in the `lidarserv-server init` step.

```
attr(Classification == 26)
```

Select a single bridge at full resolution and the water surface at a lower resolution:

```
aabb(
    [116036.137024, 493358.135986, -50.0],
    [116304.460999, 493491.625000, 50.0]
) and (
    attr(Classification == 26)
    or (
        attr(Classification == 9) and lod(4)
    )
)
```

A camera view frustum query:

```
view_frustum(
    camera_pos: [117876.57, 490596.26, 20.0],
    camera_dir: [0.75, 0.65, -0.12],
    camera_up: [0.0, 0.0, 1.0],
    fov_y : 0.78,
    z_near: 5.0,
    z_far : 10000.0,
    window_size: [500.0, 500.0],
    max_distance: 10
)
```

## Publications

 - Hermann, Paul, Michel, Krämer, Tobias, Dorra, and Arjan, Kuĳper. "Min-Max Modifiable Nested Octrees M3NO: Indexing Point Clouds with Arbitrary Attributes in Real Time." . In Computer Graphics and Visual Computing (CGVC). The Eurographics Association, 2024. https://doi.org/10.2312/cgvc.20241235.
 - Hermann, Paul. “Real-Time Indexing of Arbitrarily Attributed Point Clouds,” 2023. https://publica.fraunhofer.de/handle/publica/458643. https://github.com/Pahegi/bachelor-thesis.
 - Bormann, Pascal, Tobias, Dorra, Bastian, Stahl, and Dieter W., Fellner. "Real-time Indexing of Point Cloud Data During LiDAR Capture." . In Computer Graphics and Visual Computing (CGVC). The Eurographics Association, 2022. https://doi.org/10.2312/cgvc.20221173.
 - Dorra, Tobias. “Indexing of LiDAR Point Clouds during Capture,” 2022. https://publica.fraunhofer.de/handle/publica/416643. https://github.com/tobias93/master-thesis.