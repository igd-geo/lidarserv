//! Settings for how the point clouds should look.

use pasture_core::layout::{PointAttributeDefinition, attributes};
use std::time::Duration;

/// Settings for controlling the look of one point cloud viewer window.
#[derive(Clone, Debug)]
pub struct BaseRenderSettings {
    /// Window title of the renderer window
    pub window_title: String,

    /// Background color
    pub bg_color: Color,

    /// Options for the grid to draw on the xy-plane.
    /// Set this to [None], to disable the grid.
    pub grid: Option<Grid>,

    /// Enables or disables Eye Dome Lighting.
    pub enable_edl: bool,
    // todo: optionally draw a legend (ColorMap...)
}

/// Settings for how the grid should be rendered.
#[derive(Clone, Debug)]
pub struct Grid {
    /// Color of the grid lines.
    pub color: Color,

    /// Maximum opacity of the grid lines, with 0.0 being fully transparent and 1.0 being fully opaque.
    pub opacity: f32,

    /// The size of the whole grid.
    pub size: f64,

    /// The number of cells (along any axis).
    ///
    /// Together with [Self::size],
    /// this controls the grid spacing - to make the cells larger, either
    /// decrease (nr_cells)[Self::nr_cells], or increase (size)[Self::size].
    pub nr_cells: u8,

    /// Width of the grid lines.
    /// (In logical size units - this number will be multiplied with the screens scale factor to
    /// get the actual line width in pixels.)
    pub line_width: f32,
    // todo orientation (xy-plane, yz-plane, xz-plane)
    // todo offset (distance to axis plane)
}

/// Settings for how a single point cloud should be rendered.
#[derive(Clone, Debug)]
pub struct PointCloudRenderSettings {
    /// The color of the points.
    pub point_color: PointColor,

    /// THe shape of the points.
    pub point_shape: PointShape,

    /// The size of the points.
    pub point_size: PointSize, // todo: optionally draw a bounding box
}

/// Defines, how the points of a point cloud should be colored.
#[derive(Clone, Debug)]
pub enum PointColor {
    /// Draws every point with the same, fixed color.
    Fixed(Color),

    /// Color every point based on some (scalar) point attribute, such as the intensity,
    /// by sampling from a continuous color map.
    ScalarAttribute(ScalarAttributeColoring),

    /// Color every point based on some (categorical) point attribute, such as the classification,
    /// by looking up each points color in a palette of discrete colors.
    /// Note, that this is only valid for integer point attributes.
    CategoricalAttribute(CategoricalAttributeColoring),

    // todo: doc comment - and maybe rename
    Rgb(RgbPointColoring),
}

/// Settings for coloring a point cloud based on a scalar attribute.
#[derive(Clone, Debug)]
pub struct ScalarAttributeColoring {
    /// The attribute to use for the coloring
    pub attribute: PointAttributeDefinition,

    /// Color map to map the attribute value to a color
    pub color_map: ColorMap,

    /// Minimum value of the attribute.
    /// All attribute values smaller than (min)[Self::min] will be clamped to this value.
    /// This will be mapped to the first color of the color map.
    pub min: f32,

    /// Maximum value of the attribute.
    /// All attribute values larger than this will be clamped to this value.
    /// This will be mapped to the end color of the color map.
    pub max: f32,
}

/// Settings for coloring a point cloud based on a categorical attribute.
#[derive(Clone, Debug)]
pub struct CategoricalAttributeColoring {
    /// The attribute to use for the coloring
    pub attribute: PointAttributeDefinition,

    /// Color palette for mapping the individual values to different colors
    pub color_palette: ColorPalette,
}

#[derive(Clone, Debug)]
pub struct RgbPointColoring {
    /// The vec3 attribute to use for the coloring
    pub attribute: PointAttributeDefinition,
}

/// Defines a mapping from an input value between 0.0 and 1.0 to a color.
#[derive(Clone, Debug)]
pub struct ColorMap {
    colors: Vec<(f32, Color)>,
}

/// Defines a list of colors, that can be used for coloring attributes like classification.
#[derive(Clone, Debug)]
pub struct ColorPalette {
    colors: Vec<Color>,
    default: Color,
}

/// Defines the shape of the individual points
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub enum PointShape {
    /// Square points
    Square,

    /// Round points
    Round,
}

/// Defines, how the sizing of the points should be determined
#[derive(Copy, Clone, Debug)]
pub enum PointSize {
    /// All points will have the same, fixed, size.
    Fixed(f32),

    /// Points that are closer to the camera wil appear larger.
    Depth(f32),
}

/// An RGB color value.
/// Each of the three channels should be in between 0.0 and 1.0.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Color {
    /// red
    pub r: f32,

    /// green
    pub g: f32,

    /// blue
    pub b: f32,
}

/// Settings for animating the transition between two camera positions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnimationSettings {
    /// Defines, how long the animation will run for.
    pub duration: Duration,

    /// Easing allows to smoothly start or stop the animation.
    pub easing: AnimationEasing,
}

/// Defines, if the animation starts or stops smoothly.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub enum AnimationEasing {
    /// The animation will run at constant speed. No smooth starting or stopping.
    Linear,

    /// The camera will smoothly accelerate at the beginning of the animation.
    EaseIn,

    /// The camera will smoothly decelerate at the end of the animation.
    EaseOut,

    /// The animation will both start and stop smoothly.
    #[default]
    EaseInOut,
}

impl Default for AnimationSettings {
    fn default() -> Self {
        AnimationSettings {
            duration: Duration::from_secs_f64(0.75),
            easing: Default::default(),
        }
    }
}

impl Color {
    /// Creates a color from a r, g, b component
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Color { r, g, b }
    }

    /// Returns the same color with the r,g,b values clamped between 0.0. and 1.0
    pub fn clamped(&self) -> Color {
        Color {
            r: self.r.clamp(0.0, 1.0),
            g: self.g.clamp(0.0, 1.0),
            b: self.b.clamp(0.0, 1.0),
        }
    }

    // "pure" colors
    pub const BLACK: Color = Color::rgb(0.0, 0.0, 0.0);
    pub const RED: Color = Color::rgb(1.0, 0.0, 0.0);
    pub const GREEN: Color = Color::rgb(0.0, 1.0, 0.0);
    pub const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);
    pub const CYAN: Color = Color::rgb(0.0, 1.0, 1.0);
    pub const MAGENTA: Color = Color::rgb(1.0, 0.0, 1.0);
    pub const YELLOW: Color = Color::rgb(1.0, 1.0, 0.0);
    pub const WHITE: Color = Color::rgb(1.0, 1.0, 1.0);

    // greys
    pub const GREY_1: Color = Color::rgb(0.1, 0.1, 0.1);
    pub const GREY_2: Color = Color::rgb(0.2, 0.2, 0.2);
    pub const GREY_3: Color = Color::rgb(0.3, 0.3, 0.3);
    pub const GREY_4: Color = Color::rgb(0.4, 0.4, 0.4);
    pub const GREY_5: Color = Color::rgb(0.5, 0.5, 0.5);
    pub const GREY_6: Color = Color::rgb(0.6, 0.6, 0.6);
    pub const GREY_7: Color = Color::rgb(0.7, 0.7, 0.7);
    pub const GREY_8: Color = Color::rgb(0.8, 0.8, 0.8);
    pub const GREY_9: Color = Color::rgb(0.9, 0.9, 0.9);

    // todo presets for colors, that work great on a white background
    // todo presets for colors, that work great on a dark background
}

impl ColorMap {
    /// Makes a simple color map, that is a gradient between the two passed in colors.
    pub fn gradient(color_1: Color, color_2: Color) -> Self {
        ColorMap {
            colors: vec![(0.0, color_1), (1.0, color_2)],
        }
    }

    /// Makes a color map with equally sized gradients between the passed in colors.
    ///
    /// # Panicks
    /// Panicks, if less than two colors are given.
    pub fn equally_spaced(colors: &[Color]) -> Self {
        assert!(colors.len() >= 2);
        let nr_gradients = colors.len() as f32 - 1.0;
        let colors = colors
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, c)| (i as f32 / nr_gradients, c))
            .collect();
        ColorMap { colors }
    }

    /// Samples the color map at the given position.
    /// The value where the color map is sampled should be between 0.0 and 1.0.
    pub fn color_at(&self, value: f32) -> Color {
        let &(min_val, min_color) = self.colors.first().unwrap();
        if value <= min_val {
            return min_color;
        }

        for i in 0..self.colors.len() - 1 {
            let (left_val, left_color) = self.colors[i];
            let (right_val, right_color) = self.colors[i + 1];
            if left_val < value && value <= right_val {
                let f1 = (right_val - value) / (right_val - left_val);
                let f2 = (value - left_val) / (right_val - left_val);
                return Color {
                    r: f1 * left_color.r + f2 * right_color.r,
                    g: f1 * left_color.g + f2 * right_color.g,
                    b: f1 * left_color.b + f2 * right_color.b,
                };
            }
        }

        let &(_, max_color) = self.colors.last().unwrap();
        max_color
    }

    // simple gradients
    pub fn red_green() -> ColorMap {
        ColorMap {
            colors: vec![(0.0, Color::RED), (1.0, Color::GREEN)],
        }
    }
    pub fn red_blue() -> ColorMap {
        ColorMap {
            colors: vec![(0.0, Color::RED), (1.0, Color::BLUE)],
        }
    }
    pub fn green_red() -> ColorMap {
        ColorMap {
            colors: vec![(0.0, Color::GREEN), (1.0, Color::RED)],
        }
    }
    pub fn green_blue() -> ColorMap {
        ColorMap {
            colors: vec![(0.0, Color::GREEN), (1.0, Color::BLUE)],
        }
    }
    pub fn blue_red() -> ColorMap {
        ColorMap {
            colors: vec![(0.0, Color::BLUE), (1.0, Color::RED)],
        }
    }
    pub fn blue_green() -> ColorMap {
        ColorMap {
            colors: vec![(0.0, Color::BLUE), (1.0, Color::GREEN)],
        }
    }
    pub fn yellow_cyan() -> ColorMap {
        ColorMap {
            colors: vec![(0.0, Color::YELLOW), (1.0, Color::CYAN)],
        }
    }
    pub fn magenta_cyan() -> ColorMap {
        ColorMap {
            colors: vec![(0.0, Color::MAGENTA), (1.0, Color::CYAN)],
        }
    }
    pub fn cyan_yellow() -> ColorMap {
        ColorMap {
            colors: vec![(0.0, Color::CYAN), (1.0, Color::YELLOW)],
        }
    }
    pub fn magenta_yellow() -> ColorMap {
        ColorMap {
            colors: vec![(0.0, Color::MAGENTA), (1.0, Color::YELLOW)],
        }
    }
    pub fn yellow_magenta() -> ColorMap {
        ColorMap {
            colors: vec![(0.0, Color::YELLOW), (1.0, Color::MAGENTA)],
        }
    }
    pub fn cyan_magenta() -> ColorMap {
        ColorMap {
            colors: vec![(0.0, Color::CYAN), (1.0, Color::MAGENTA)],
        }
    }
    pub fn greyscale() -> ColorMap {
        ColorMap {
            colors: vec![(0.0, Color::BLACK), (1.0, Color::WHITE)],
        }
    }
    pub fn greyscale_inverted() -> ColorMap {
        ColorMap {
            colors: vec![(0.0, Color::WHITE), (1.0, Color::BLACK)],
        }
    }

    // some more fancy presets
    pub fn fire() -> ColorMap {
        ColorMap {
            colors: vec![
                (0.0, Color::BLACK),
                (0.33, Color::RED),
                (1.0, Color::YELLOW),
            ],
        }
    }
    pub fn rainbow() -> ColorMap {
        ColorMap {
            colors: vec![
                (0.0, Color::MAGENTA),
                (0.2, Color::BLUE),
                (0.4, Color::CYAN),
                (0.6, Color::GREEN),
                (0.8, Color::YELLOW),
                (1.0, Color::RED),
            ],
        }
    }
    pub fn sky() -> ColorMap {
        ColorMap {
            colors: vec![(0.0, Color::BLACK), (0.5, Color::BLUE), (1.0, Color::WHITE)],
        }
    }
}

impl ColorPalette {
    /// Constructs a new empty color palette.
    /// Use in combination with [Self::with_color] to define the colors in the palette.
    pub fn new(default_color: Color) -> Self {
        ColorPalette {
            colors: vec![],
            default: default_color,
        }
    }

    /// Like [Self::set_color], just that it can be used in a method chaining fashion, to
    /// conveniently construct new palettes, or to modify the existing preset:
    ///
    /// ```rust
    /// use point_cloud_viewer::renderer::settings::{ColorPalette, Color};
    ///
    /// // Constructing a new color palette
    /// let palette = ColorPalette::new(Color::BLACK)
    ///     .with_color(0, Color::YELLOW)
    ///     .with_color(1, Color::BLUE)
    ///     .with_color(2, Color::MAGENTA);
    /// assert_eq!(palette.get_color(1), Color::BLUE);
    ///
    /// // Modifying an included "preset" color palette.
    /// let nicer_blue = Color::rgb(0.0, 0.1, 1.0);
    /// let palette = ColorPalette::las_classification_colors().with_color(9, nicer_blue);
    /// assert_eq!(palette.get_color(9), nicer_blue);
    /// ```
    pub fn with_color(mut self, index: usize, color: Color) -> Self {
        self.set_color(index, color);
        self
    }

    /// Updates the color at the given index.
    pub fn set_color(&mut self, index: usize, color: Color) {
        while index >= self.colors.len() {
            self.colors.push(self.default);
        }
        self.colors[index] = color;
    }

    /// Returns a new Color palette, that is equal to this color palette, with
    /// an updated default color.
    pub fn set_default_color(&mut self, default: Color) {
        self.default = default;
    }

    /// Colors suitable for the classification attribute in LAS, 1.4, point formats 0 - 5
    pub fn las_classification_colors() -> ColorPalette {
        ColorPalette {
            colors: vec![
                Color::GREY_4,                   // 0: Created, Never Classified
                Color::GREY_3,                   // 1: Unassigned
                Color::rgb(0.396, 0.263, 0.129), // 2: Ground
                Color::rgb(0.0, 0.6, 0.4),       // 3: Low Vegetation
                Color::rgb(0.0, 0.8, 0.2),       // 4: Medium Vegetation
                Color::rgb(0.0, 1.0, 0.0),       // 5: High Vegetation
                Color::RED,                      // 6: Building
                Color::MAGENTA,                  // 7: Low Point (Noise)
                Color::YELLOW,                   // 8: Model Key-Point (Mass Point)
                Color::rgb(0.4, 0.8, 1.0),       // 9: Water
                Color::GREY_5,                   // 10: Reserved for ASPRS Definition
                Color::GREY_5,                   // 11: Reserved for ASPRS Definition
                Color::BLUE,                     // 12: Overlap Points
            ],
            default: Color::GREY_5, // 13-31: Reserved for ASPRS Definition
        }
    }

    /// Gets the colors of the palette
    pub fn colors(&self) -> &[Color] {
        &self.colors
    }

    /// Returns the default color, that we will fall back to in case a value larger than the palette size is looked up.
    pub fn default_color(&self) -> Color {
        self.default
    }

    /// Get the color at the given index, or the default color, if the given index is outside the range of the palette.
    pub fn get_color(&self, index: usize) -> Color {
        *self.colors.get(index).unwrap_or(&self.default)
    }
}

impl Default for BaseRenderSettings {
    fn default() -> Self {
        BaseRenderSettings {
            window_title: "Point Cloud Viewer".to_string(),
            bg_color: Color::rgb(1.0, 1.0, 1.0),
            grid: None,
            enable_edl: false,
        }
    }
}

impl Default for Grid {
    fn default() -> Self {
        Self {
            color: Color::rgb(0.5, 0.5, 0.5),
            opacity: 1.0,
            size: 10.0,
            nr_cells: 100,
            line_width: 1.0,
        }
    }
}

impl Default for PointCloudRenderSettings {
    fn default() -> Self {
        PointCloudRenderSettings {
            point_color: PointColor::Fixed(Color::rgb(0.0, 0.0, 0.0)),
            point_shape: PointShape::Square,
            point_size: PointSize::Fixed(3.0),
        }
    }
}

impl Default for ScalarAttributeColoring {
    fn default() -> Self {
        Self {
            attribute: attributes::INTENSITY.clone(),
            color_map: ColorMap::sky(),
            min: 0.0,
            max: 1.0,
        }
    }
}
