# Lidar Serv

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

The point cloud server is the main component, that manages the point cloud. Any point cloud project is started, by initializing a new index:

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

After creating a point cloud, we can start the server like so:

```shell
lidarserv-server serve my-pointcloud
```

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