//! Debug / visualization views. The mode index is passed to the shaders, so the
//! order of `ViewMode::ALL` must match the `MODE_*` constants in `splat.vert`.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    /// Final shaded result.
    Color,
    /// SH band 0 only: the view-independent base color.
    BaseColor,
    /// SH bands 1..3 only (offset to mid-grey): the view-dependent part.
    ViewDependent,
    /// Alpha-blended expected depth, turbo colormap (near = red, far = blue).
    Depth,
    /// Per-splat opacity, turbo colormap, each splat drawn with unit opacity.
    Opacity,
    /// 1σ / 2σ ellipse outlines of every splat.
    Ellipses,
    /// Splat centers as fixed-size dots: the underlying point cloud.
    Centers,
    /// Screen-space anisotropy (major / minor axis ratio), turbo colormap.
    Anisotropy,
    /// Additive count of fragments that pass the alpha cutoff (fill cost).
    Overdraw,
    /// The 3σ quads actually rasterized for each splat.
    Quads,
}

impl ViewMode {
    pub const ALL: [ViewMode; 10] = [
        Self::Color,
        Self::BaseColor,
        Self::ViewDependent,
        Self::Depth,
        Self::Opacity,
        Self::Ellipses,
        Self::Centers,
        Self::Anisotropy,
        Self::Overdraw,
        Self::Quads,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Color => "color",
            Self::BaseColor => "base-color",
            Self::ViewDependent => "view-dependent",
            Self::Depth => "depth",
            Self::Opacity => "opacity",
            Self::Ellipses => "ellipses",
            Self::Centers => "centers",
            Self::Anisotropy => "anisotropy",
            Self::Overdraw => "overdraw",
            Self::Quads => "quads",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.name() == s)
    }

    pub fn index(self) -> i32 {
        Self::ALL.iter().position(|m| *m == self).unwrap() as i32
    }

    /// Additive blending instead of back-to-front "over".
    pub fn additive(self) -> bool {
        self == Self::Overdraw
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DebugSettings {
    pub view: ViewMode,
    /// Multiplier on every Gaussian's scale (shrink to see structure).
    pub splat_scale: f32,
    /// Splats with lower opacity are culled.
    pub min_opacity: f32,
    /// Cap on the SH degree used for color (min'd with the file's degree).
    pub sh_max: u32,
}

impl Default for DebugSettings {
    fn default() -> Self {
        Self { view: ViewMode::Color, splat_scale: 1.0, min_opacity: 0.0, sh_max: 3 }
    }
}

impl DebugSettings {
    pub fn summary(&self) -> String {
        format!(
            "view {} | scale {:.2} | min opacity {:.2} | SH max {}",
            self.view.name(),
            self.splat_scale,
            self.min_opacity,
            self.sh_max
        )
    }
}
