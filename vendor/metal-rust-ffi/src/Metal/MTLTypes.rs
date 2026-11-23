//! Safe value types shared by the audited Metal wrappers.

use objc2_metal::{
    MTLCommandBufferStatus, MTLOrigin, MTLPixelFormat, MTLPrimitiveType, MTLRegion,
    MTLResourceOptions, MTLSize, MTLTextureType, MTLTextureUsage, MTLViewport,
};

/// A Metal pixel format supported by the safe API.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PixelFormat(pub(crate) u64);

impl PixelFormat {
    pub(crate) const fn from_system_raw(value: usize) -> Self {
        Self(value as u64)
    }

    /// Returns the framework integer representation.
    #[must_use]
    pub const fn as_raw(self) -> usize {
        self.0 as usize
    }

    /// Returns whether this value can be passed back to Metal.
    #[must_use]
    pub const fn is_valid(self) -> bool {
        true
    }

    /// An invalid or unset pixel format.
    pub const INVALID: Self = Self(0);
    /// Eight-bit normalized blue/green/red/alpha format.
    pub const BGRA8_UNORM: Self = Self(80);
    /// Eight-bit normalized red/green/blue/alpha format.
    pub const RGBA8_UNORM: Self = Self(70);
    /// Eight-bit sRGB blue/green/red/alpha format.
    pub const BGRA8_UNORM_SRGB: Self = Self(81);
    /// Eight-bit sRGB red/green/blue/alpha format.
    pub const RGBA8_UNORM_SRGB: Self = Self(71);
    /// Half-precision floating-point red/green/blue/alpha format.
    pub const RGBA16_FLOAT: Self = Self(115);
    /// Half-precision floating-point red format.
    pub const R16_FLOAT: Self = Self(25);
    /// Half-precision floating-point red/green format.
    pub const RG16_FLOAT: Self = Self(65);
    /// 32-bit floating-point depth format.
    pub const DEPTH32_FLOAT: Self = Self(252);

    pub(crate) const fn as_objc(self) -> MTLPixelFormat {
        MTLPixelFormat(self.0 as _)
    }
}

impl Default for PixelFormat {
    fn default() -> Self {
        Self::RGBA8_UNORM
    }
}

/// Resource allocation options accepted by the safe API.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResourceOptions(pub(crate) u64);

impl ResourceOptions {
    pub(crate) const fn from_system_raw(value: usize) -> Self {
        Self(value as u64)
    }

    /// Returns the framework integer representation.
    #[must_use]
    pub const fn as_raw(self) -> usize {
        self.0 as usize
    }

    /// Returns whether this option set originated from safe constants or Metal.
    #[must_use]
    pub const fn is_valid(self) -> bool {
        true
    }

    /// CPU and GPU share a coherent allocation.
    pub const SHARED: Self = Self(0);
    /// CPU and GPU use separate managed copies.
    pub const MANAGED: Self = Self(1 << 4);
    /// The allocation is private to the GPU.
    pub const PRIVATE: Self = Self(2 << 4);
    /// The allocation is memoryless and only valid for render attachments.
    pub const MEMORYLESS: Self = Self(3 << 4);

    pub(crate) const fn as_objc(self) -> MTLResourceOptions {
        MTLResourceOptions(self.0 as _)
    }

    pub(crate) const fn storage_mode(self) -> StorageMode {
        match (self.0 >> 4) & 0xF {
            1 => StorageMode::Managed,
            2 => StorageMode::Private,
            3 => StorageMode::Memoryless,
            _ => StorageMode::Shared,
        }
    }
}

impl Default for ResourceOptions {
    fn default() -> Self {
        Self::SHARED
    }
}

/// The storage mode of a Metal resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StorageMode {
    /// CPU and GPU share the same allocation.
    Shared,
    /// CPU and GPU use managed copies.
    Managed,
    /// The allocation is private to the GPU.
    Private,
    /// The allocation has no persistent backing store.
    Memoryless,
}

impl StorageMode {
    pub(crate) const fn try_from_system_raw(value: usize) -> Option<Self> {
        match value {
            0 => Some(Self::Shared),
            1 => Some(Self::Managed),
            2 => Some(Self::Private),
            3 => Some(Self::Memoryless),
            _ => None,
        }
    }

    /// Returns the framework integer representation.
    #[must_use]
    pub const fn as_raw(self) -> usize {
        match self {
            Self::Shared => 0,
            Self::Managed => 1,
            Self::Private => 2,
            Self::Memoryless => 3,
        }
    }

    pub(crate) const fn is_valid(self) -> bool {
        true
    }
}

/// The dimensionality of a Metal texture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextureType {
    /// A one-dimensional texture.
    D1,
    /// An array of one-dimensional textures.
    D1Array,
    /// A two-dimensional texture.
    #[default]
    D2,
    /// An array of two-dimensional textures.
    D2Array,
    /// A multisampled two-dimensional texture.
    D2Multisample,
    /// An array of multisampled two-dimensional textures.
    D2MultisampleArray,
    /// A three-dimensional texture.
    D3,
    /// A cube texture.
    Cube,
    /// An array of cube textures.
    CubeArray,
    /// A texture backed by a linear texture buffer.
    TextureBuffer,
}

impl TextureType {
    pub(crate) const fn try_from_system_raw(value: usize) -> Option<Self> {
        match value {
            0 => Some(Self::D1),
            1 => Some(Self::D1Array),
            2 => Some(Self::D2),
            3 => Some(Self::D2Array),
            4 => Some(Self::D2Multisample),
            5 => Some(Self::Cube),
            6 => Some(Self::CubeArray),
            7 => Some(Self::D3),
            8 => Some(Self::D2MultisampleArray),
            9 => Some(Self::TextureBuffer),
            _ => None,
        }
    }

    /// Returns the framework integer representation.
    #[must_use]
    pub const fn as_raw(self) -> usize {
        match self {
            Self::D1 => 0,
            Self::D1Array => 1,
            Self::D2 => 2,
            Self::D2Array => 3,
            Self::D2Multisample => 4,
            Self::Cube => 5,
            Self::CubeArray => 6,
            Self::D3 => 7,
            Self::D2MultisampleArray => 8,
            Self::TextureBuffer => 9,
        }
    }

    pub(crate) const fn is_valid(self) -> bool {
        true
    }

    pub(crate) const fn as_objc(self) -> MTLTextureType {
        MTLTextureType(match self {
            Self::D1 => 0,
            Self::D1Array => 1,
            Self::D2 => 2,
            Self::D2Array => 3,
            Self::D2Multisample => 4,
            Self::D3 => 7,
            Self::Cube => 5,
            Self::CubeArray => 6,
            Self::D2MultisampleArray => 8,
            Self::TextureBuffer => 9,
        })
    }
}

/// Bit flags describing how a texture will be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureUsage(pub(crate) u64);

impl TextureUsage {
    pub(crate) const fn from_system_raw(value: usize) -> Self {
        Self(value as u64)
    }

    /// Returns the framework integer representation.
    #[must_use]
    pub const fn as_raw(self) -> usize {
        self.0 as usize
    }

    /// Returns whether this option set originated from safe constants or Metal.
    #[must_use]
    pub const fn is_valid(self) -> bool {
        true
    }

    /// No usage hint.
    pub const UNKNOWN: Self = Self(0);
    /// The texture can be read by a shader.
    pub const SHADER_READ: Self = Self(1);
    /// The texture can be written by a shader.
    pub const SHADER_WRITE: Self = Self(2);
    /// The texture can be used as a render target.
    pub const RENDER_TARGET: Self = Self(4);
    /// The texture can be used as a pixel-format view.
    pub const PIXEL_FORMAT_VIEW: Self = Self(16);
    /// The texture can be used for atomic shader operations.
    pub const SHADER_ATOMIC: Self = Self(32);

    /// Combines two usage hints.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub(crate) const fn as_objc(self) -> MTLTextureUsage {
        MTLTextureUsage(self.0 as _)
    }
}

impl Default for TextureUsage {
    fn default() -> Self {
        Self::SHADER_READ
    }
}

/// A three-dimensional unsigned texture origin.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Origin {
    /// The x coordinate.
    pub x: usize,
    /// The y coordinate.
    pub y: usize,
    /// The z coordinate.
    pub z: usize,
}

impl Origin {
    /// Creates an origin from three coordinates.
    #[must_use]
    pub const fn new(x: usize, y: usize, z: usize) -> Self {
        Self { x, y, z }
    }
}

impl From<Origin> for MTLOrigin {
    fn from(value: Origin) -> Self {
        Self {
            x: value.x,
            y: value.y,
            z: value.z,
        }
    }
}

/// A three-dimensional unsigned Metal size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Size {
    /// The width.
    pub width: usize,
    /// The height.
    pub height: usize,
    /// The depth.
    pub depth: usize,
}

impl Size {
    /// Creates a size from three dimensions.
    #[must_use]
    pub const fn new(width: usize, height: usize, depth: usize) -> Self {
        Self {
            width,
            height,
            depth,
        }
    }
}

impl From<Size> for MTLSize {
    fn from(value: Size) -> Self {
        Self {
            width: value.width,
            height: value.height,
            depth: value.depth,
        }
    }
}

/// A rectangular region in a texture or dispatch grid.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Region {
    /// The region origin.
    pub origin: Origin,
    /// The region dimensions.
    pub size: Size,
}

impl Region {
    /// Creates a region from an origin and size.
    #[must_use]
    pub const fn new(origin: Origin, size: Size) -> Self {
        Self { origin, size }
    }

    /// Creates a one-dimensional region.
    #[must_use]
    pub const fn new_1d(x: usize, width: usize) -> Self {
        Self::new(Origin::new(x, 0, 0), Size::new(width, 1, 1))
    }

    /// Creates a two-dimensional region.
    #[must_use]
    pub const fn new_2d(x: usize, y: usize, width: usize, height: usize) -> Self {
        Self::new(Origin::new(x, y, 0), Size::new(width, height, 1))
    }

    /// Creates a three-dimensional region.
    #[must_use]
    pub const fn new_3d(
        x: usize,
        y: usize,
        z: usize,
        width: usize,
        height: usize,
        depth: usize,
    ) -> Self {
        Self::new(Origin::new(x, y, z), Size::new(width, height, depth))
    }
}

impl From<Region> for MTLRegion {
    fn from(value: Region) -> Self {
        Self {
            origin: value.origin.into(),
            size: value.size.into(),
        }
    }
}

/// A floating-point viewport.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    /// The x origin.
    pub origin_x: f64,
    /// The y origin.
    pub origin_y: f64,
    /// The width.
    pub width: f64,
    /// The height.
    pub height: f64,
    /// The near depth.
    pub z_near: f64,
    /// The far depth.
    pub z_far: f64,
}

impl Viewport {
    /// Creates a viewport.
    #[must_use]
    pub const fn new(
        origin_x: f64,
        origin_y: f64,
        width: f64,
        height: f64,
        z_near: f64,
        z_far: f64,
    ) -> Self {
        Self {
            origin_x,
            origin_y,
            width,
            height,
            z_near,
            z_far,
        }
    }
}

impl From<Viewport> for MTLViewport {
    fn from(value: Viewport) -> Self {
        Self {
            originX: value.origin_x,
            originY: value.origin_y,
            width: value.width,
            height: value.height,
            znear: value.z_near,
            zfar: value.z_far,
        }
    }
}

/// A color used to clear a render target.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ClearColor {
    /// Red component.
    pub red: f64,
    /// Green component.
    pub green: f64,
    /// Blue component.
    pub blue: f64,
    /// Alpha component.
    pub alpha: f64,
}

impl ClearColor {
    /// Creates a clear color.
    #[must_use]
    pub const fn new(red: f64, green: f64, blue: f64, alpha: f64) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }
}

/// The current state of a command buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommandBufferStatus {
    /// The buffer has not been enqueued.
    NotEnqueued,
    /// The buffer is enqueued but not committed.
    Enqueued,
    /// The buffer is committed but not scheduled.
    Committed,
    /// The buffer is scheduled for execution.
    Scheduled,
    /// The buffer completed successfully.
    Completed,
    /// Execution failed.
    Error,
    /// A newer platform returned an unknown value.
    Unknown(u64),
}

impl From<MTLCommandBufferStatus> for CommandBufferStatus {
    fn from(value: MTLCommandBufferStatus) -> Self {
        match value.0 {
            0 => Self::NotEnqueued,
            1 => Self::Enqueued,
            2 => Self::Committed,
            3 => Self::Scheduled,
            4 => Self::Completed,
            5 => Self::Error,
            value => Self::Unknown(value as u64),
        }
    }
}

impl CommandBufferStatus {
    #[allow(dead_code)]
    pub(crate) const fn try_from_system_raw(value: usize) -> Option<Self> {
        Some(match value {
            0 => Self::NotEnqueued,
            1 => Self::Enqueued,
            2 => Self::Committed,
            3 => Self::Scheduled,
            4 => Self::Completed,
            5 => Self::Error,
            value => Self::Unknown(value as u64),
        })
    }

    /// Returns the framework integer representation.
    #[must_use]
    pub const fn as_raw(self) -> usize {
        match self {
            Self::NotEnqueued => 0,
            Self::Enqueued => 1,
            Self::Committed => 2,
            Self::Scheduled => 3,
            Self::Completed => 4,
            Self::Error => 5,
            Self::Unknown(value) => value as usize,
        }
    }

    #[allow(dead_code)]
    pub(crate) const fn is_valid(self) -> bool {
        !matches!(self, Self::Unknown(_))
    }
}

/// A primitive topology used by a render encoder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PrimitiveType {
    /// Point primitives.
    Point,
    /// Line primitives.
    Line,
    /// Connected line primitives.
    LineStrip,
    /// Triangle primitives.
    Triangle,
    /// Connected triangle primitives.
    TriangleStrip,
}

impl PrimitiveType {
    #[allow(dead_code)]
    pub(crate) const fn try_from_system_raw(value: usize) -> Option<Self> {
        match value {
            0 => Some(Self::Point),
            1 => Some(Self::Line),
            2 => Some(Self::LineStrip),
            3 => Some(Self::Triangle),
            4 => Some(Self::TriangleStrip),
            _ => None,
        }
    }

    /// Returns the framework integer representation.
    #[must_use]
    pub const fn as_raw(self) -> usize {
        match self {
            Self::Point => 0,
            Self::Line => 1,
            Self::LineStrip => 2,
            Self::Triangle => 3,
            Self::TriangleStrip => 4,
        }
    }

    #[allow(dead_code)]
    pub(crate) const fn is_valid(self) -> bool {
        true
    }

    pub(crate) const fn as_objc(self) -> MTLPrimitiveType {
        MTLPrimitiveType(match self {
            Self::Point => 0,
            Self::Line => 1,
            Self::LineStrip => 2,
            Self::Triangle => 3,
            Self::TriangleStrip => 4,
        })
    }
}
