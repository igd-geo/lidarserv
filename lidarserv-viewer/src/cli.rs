use std::str::FromStr;
use structopt::StructOpt;

#[derive(StructOpt, Debug, Clone)]
pub struct Args {
    /// Verbosity of the command line output.
    #[structopt(long, default_value="info", possible_values = &["trace", "debug", "info", "warn", "error"])]
    pub log_level: log::Level,

    #[structopt(long, short, default_value = "::1")]
    pub host: String,

    #[structopt(long, short, default_value = "4567")]
    pub port: u16,

    /// The multisampling level used during rendering.
    ///
    /// The value MUST be a power of 2. A value of `0` indicates, that multisampling is disabled.
    #[structopt(long, default_value = "2")]
    pub multisampling: u16,

    #[structopt(long)]
    pub disable_eye_dome_lighting: bool,

    #[structopt(long, default_value = "fixed", possible_values = &["fixed", "intensity", "rgb"])]
    pub point_color: PointColorArg,

    #[structopt(long, default_value = "10")]
    pub point_size: f32,

    #[structopt(long, default_value = "10")]
    pub point_distance: f64,

    /// The query is in json format instead of the query language. Useful for scripts.
    #[structopt(long)]
    pub json: bool,

    /// Disable point filtering of attributes, only filter node-wise.
    #[structopt(long)]
    pub disable_point_filtering: bool,

    /// The query to run.
    ///
    /// Examples:
    ///  - Return ALL points:
    ///    `full`
    ///  - Return all points up to some level of detail:
    ///    `lod(3)`
    ///  - Return all points in a bounding box:
    ///    `aabb([555000.1, 5923000.6, 20.2], [555999.1, 5923999.6, 88.5])`
    ///  - Return all points in a bounding box up to some level of detail:
    ///    `lod(3) and aabb([555000.1, 5923000.6, 20.2], [555999.1, 5923999.6, 88.5])`
    ///
    /// Query language specification:
    ///  - `full`
    ///    Matches any point
    ///  - `empty`
    ///    Matches no point
    ///  - `lod(x)`
    ///    (x is a positive integer)
    ///    Matches any point from level of detail x or lower
    ///  - `aabb([xmin, ymin, zmin], [xmax, ymax, zmax])`
    ///    (xmin, ymin, zmin, xmax, ymax, zmax are floating point numbers)
    ///    Matches any point within the given aabb
    ///  - `view_frustum(
    ///        camera_pos: [x1, y1, z1],
    ///        camera_dir: [x2, y2, z2],
    ///        camera_up: [x3, y3, z3],
    ///        fov_y: f4,
    ///        z_near: f5,
    ///        z_far: f6,
    ///        window_size: [x7, y7] ,
    ///        max_distance: f8
    ///    )`
    ///    (all numbers x*, y*, z*, f* are floating point numbers.)
    ///    Performs a view frustum query with the given camera parameters. Will
    ///    only match points that are in the cameras view frustum. Points closer to
    ///    the camera will have a higher lod, points further away will have a lower
    ///    lod, so that after perspective projection, the given max_distance between
    ///    points in pixels is upheld.
    ///  - `(q)`
    ///    (q is a query)
    ///    Brackets around a (sub)query can be used to override the order of operator
    ///    precedence.
    ///    The default operator precedence is: brackets > not > and > or
    ///  - `!q`
    ///    (q is a query)
    ///    Inverts the query q. Matches any point that is not matched by q and
    ///    vice-versa.
    ///  - `q1 and q2`
    ///    (q1 and q2 are queries)
    ///    Matches the intersection of queries q1 and q2.
    ///  - `q1 or q2`
    ///    (q1 and q2 are queries)
    ///    Matches the union of queries q1 and q2.
    ///  - `attr(name comp value)`
    ///    (name is a string, comp is a comparison operator, value is a floating point number)
    ///
    /// Json queries:
    /// With the `--json` parameter, you can write queries directly in json, which
    /// might be useful for scripting. The json translates directly into the query
    /// language described above. Valid json queries are:
    ///  - `"Empty"`
    ///  - `"Full"`
    ///  - `{"Lod": i}`
    ///  - `{"Aabb": {
    ///         "min": [x1, y1, z1],
    ///         "max": [x2, y2, z2]
    ///     }}`
    ///  - `{"ViewFrustum": {
    ///         "camera_pos": [x1, y1, z1],
    ///         "camera_dir": [x2, y2, z2],
    ///         "camera_up": [x3, y3, z3],
    ///         "fov_y": f4,
    ///         "z_near": f5,
    ///         "z_far": f6,
    ///         "window_size": [x7, y7],
    ///         "max_distance": f8,
    ///     }}`
    ///  - `{"Not": q}`
    ///  - `{"And": [q1, q2, ...]}`
    ///  - `{"Or": [q1, q2, ...]}`
    ///
    /// NOTE: Be careful to correctly escape the query on the shell. It is probably
    /// best to quote the whole query.
    #[structopt(long, default_value = "full")]
    pub query: String,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum PointColorArg {
    Fixed,
    Intensity,
    Rgb,
}

impl FromStr for PointColorArg {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "fixed" => Ok(PointColorArg::Fixed),
            "intensity" => Ok(PointColorArg::Intensity),
            "rgb" => Ok(PointColorArg::Rgb),
            _ => Err(anyhow::Error::msg(
                "Invalid value - must be one of: 'fixed', 'intensity'",
            )),
        }
    }
}
