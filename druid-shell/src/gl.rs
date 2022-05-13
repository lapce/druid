/// All APIs related to OpenGL that you can possibly get while using glutin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Api {
    /// The classical OpenGL. Available on Windows, Unix operating systems,
    /// OS/X.
    OpenGl,
    /// OpenGL embedded system. Available on Unix operating systems, Android.
    OpenGlEs,
    /// OpenGL for the web. Very similar to OpenGL ES.
    WebGl,
}

/// Describes the requested OpenGL [`Context`] profiles.
///
/// [`Context`]: struct.Context.html
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlProfile {
    /// Include all the immediate more functions and definitions.
    Compatibility,
    /// Include all the future-compatible functions and definitions.
    Core,
}

/// Describes the OpenGL API and version that are being requested when a context
/// is created.
#[derive(Debug, Copy, Clone)]
pub enum GlRequest {
    /// Request the latest version of the "best" API of this platform.
    ///
    /// On desktop, will try OpenGL.
    Latest,

    /// Request a specific version of a specific API.
    ///
    /// Example: `GlRequest::Specific(Api::OpenGl, (3, 3))`.
    Specific(Api, (u8, u8)),

    /// If OpenGL is available, create an OpenGL [`Context`] with the specified
    /// `opengl_version`. Else if OpenGL ES or WebGL is available, create a
    /// context with the specified `opengles_version`.
    ///
    /// [`Context`]: struct.Context.html
    GlThenGles {
        /// The version to use for OpenGL.
        opengl_version: (u8, u8),
        /// The version to use for OpenGL ES.
        opengles_version: (u8, u8),
    },
}

impl GlRequest {
    /// Extract the desktop GL version, if any.
    pub fn to_gl_version(&self) -> Option<(u8, u8)> {
        match self {
            &GlRequest::Specific(Api::OpenGl, opengl_version) => Some(opengl_version),
            &GlRequest::GlThenGles { opengl_version, .. } => Some(opengl_version),
            _ => None,
        }
    }
}

/// The minimum core profile GL context. Useful for getting the minimum
/// required GL version while still running on OSX, which often forbids
/// the compatibility profile features.
pub static GL_CORE: GlRequest = GlRequest::Specific(Api::OpenGl, (3, 2));

/// Specifies the tolerance of the OpenGL [`Context`] to faults. If you accept
/// raw OpenGL commands and/or raw shader code from an untrusted source, you
/// should definitely care about this.
///
/// [`Context`]: struct.Context.html
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Robustness {
    /// Not everything is checked. Your application can crash if you do
    /// something wrong with your shaders.
    NotRobust,

    /// The driver doesn't check anything. This option is very dangerous.
    /// Please know what you're doing before using it. See the
    /// `GL_KHR_no_error` extension.
    ///
    /// Since this option is purely an optimization, no error will be returned
    /// if the backend doesn't support it. Instead it will automatically
    /// fall back to [`NotRobust`].
    ///
    /// [`NotRobust`]: enum.Robustness.html#variant.NotRobust
    NoError,

    /// Everything is checked to avoid any crash. The driver will attempt to
    /// avoid any problem, but if a problem occurs the behavior is
    /// implementation-defined. You are just guaranteed not to get a crash.
    RobustNoResetNotification,

    /// Same as [`RobustNoResetNotification`] but the context creation doesn't
    /// fail if it's not supported.
    ///
    /// [`RobustNoResetNotification`]:
    /// enum.Robustness.html#variant.RobustNoResetNotification
    TryRobustNoResetNotification,

    /// Everything is checked to avoid any crash. If a problem occurs, the
    /// context will enter a "context lost" state. It must then be
    /// recreated. For the moment, glutin doesn't provide a way to recreate
    /// a context with the same window :-/
    RobustLoseContextOnReset,

    /// Same as [`RobustLoseContextOnReset`] but the context creation doesn't
    /// fail if it's not supported.
    ///
    /// [`RobustLoseContextOnReset`]:
    /// enum.Robustness.html#variant.RobustLoseContextOnReset
    TryRobustLoseContextOnReset,
}

/// The behavior of the driver when you change the current context.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ReleaseBehavior {
    /// Doesn't do anything. Most notably doesn't flush.
    None,

    /// Flushes the context that was previously current as if `glFlush` was
    /// called.
    Flush,
}

/// Describes a possible format.
#[allow(missing_docs)]
#[derive(Debug, Clone)]
pub struct PixelFormat {
    pub hardware_accelerated: bool,
    /// The number of color bits. Does not include alpha bits.
    pub color_bits: u8,
    pub alpha_bits: u8,
    pub depth_bits: u8,
    pub stencil_bits: u8,
    pub stereoscopy: bool,
    pub double_buffer: bool,
    /// `None` if multisampling is disabled, otherwise `Some(N)` where `N` is
    /// the multisampling level.
    pub multisampling: Option<u16>,
    pub srgb: bool,
}

pub struct PixelFormatRequirements {
    /// If true, only hardware-accelerated formats will be considered. If
    /// false, only software renderers. `None` means "don't care". Default
    /// is `Some(true)`.
    pub hardware_accelerated: Option<bool>,

    /// Minimum number of bits for the color buffer, excluding alpha. `None`
    /// means "don't care". The default is `Some(24)`.
    pub color_bits: Option<u8>,

    /// If true, the color buffer must be in a floating point format. Default
    /// is `false`.
    ///
    /// Using floating points allows you to write values outside of the `[0.0,
    /// 1.0]` range.
    pub float_color_buffer: bool,

    /// Minimum number of bits for the alpha in the color buffer. `None` means
    /// "don't care". The default is `Some(8)`.
    pub alpha_bits: Option<u8>,

    /// Minimum number of bits for the depth buffer. `None` means "don't care".
    /// The default value is `Some(24)`.
    pub depth_bits: Option<u8>,

    /// Minimum number of stencil bits. `None` means "don't care".
    /// The default value is `Some(8)`.
    pub stencil_bits: Option<u8>,

    /// If true, only double-buffered formats will be considered. If false,
    /// only single-buffer formats. `None` means "don't care". The default
    /// is `Some(true)`.
    pub double_buffer: Option<bool>,

    /// Contains the minimum number of samples per pixel in the color, depth
    /// and stencil buffers. `None` means "don't care". Default is `None`.
    /// A value of `Some(0)` indicates that multisampling must not be enabled.
    pub multisampling: Option<u16>,

    /// If true, only stereoscopic formats will be considered. If false, only
    /// non-stereoscopic formats. The default is `false`.
    pub stereoscopy: bool,

    /// If true, only sRGB-capable formats will be considered. If false, don't
    /// care. The default is `true`.
    pub srgb: bool,

    /// The behavior when changing the current context. Default is `Flush`.
    pub release_behavior: ReleaseBehavior,

    /// X11 only: set internally to insure a certain visual xid is used when
    /// choosing the fbconfig.
    pub(crate) x11_visual_xid: Option<std::os::raw::c_ulong>,
}

impl Default for PixelFormatRequirements {
    #[inline]
    fn default() -> PixelFormatRequirements {
        PixelFormatRequirements {
            hardware_accelerated: Some(true),
            color_bits: Some(24),
            float_color_buffer: false,
            alpha_bits: Some(8),
            depth_bits: Some(24),
            stencil_bits: Some(8),
            double_buffer: None,
            multisampling: None,
            stereoscopy: false,
            srgb: true,
            release_behavior: ReleaseBehavior::Flush,
            x11_visual_xid: None,
        }
    }
}

/// Attributes to use when creating an OpenGL [`Context`].
///
/// [`Context`]: struct.Context.html
#[derive(Clone, Debug)]
pub struct GlAttributes {
    /// Version to try create. See [`GlRequest`] for more infos.
    ///
    /// The default is [`Latest`].
    ///
    /// [`Latest`]: enum.GlRequest.html#variant.Latest
    /// [`GlRequest`]: enum.GlRequest.html
    pub version: GlRequest,

    /// OpenGL profile to use.
    ///
    /// The default is `None`.
    pub profile: Option<GlProfile>,

    /// Whether to enable the `debug` flag of the context.
    ///
    /// Debug contexts are usually slower but give better error reporting.
    ///
    /// The default is `true` in debug mode and `false` in release mode.
    pub debug: bool,

    /// How the OpenGL [`Context`] should detect errors.
    ///
    /// The default is `NotRobust` because this is what is typically expected
    /// when you create an OpenGL [`Context`]. However for safety you should
    /// consider [`TryRobustLoseContextOnReset`].
    ///
    /// [`Context`]: struct.Context.html
    /// [`TryRobustLoseContextOnReset`]:
    /// enum.Robustness.html#variant.TryRobustLoseContextOnReset
    pub robustness: Robustness,

    /// Whether to use vsync. If vsync is enabled, calling `swap_buffers` will
    /// block until the screen refreshes. This is typically used to prevent
    /// screen tearing.
    ///
    /// The default is `false`.
    pub vsync: bool,
}

impl Default for GlAttributes {
    #[inline]
    fn default() -> GlAttributes {
        GlAttributes {
            version: GlRequest::Latest,
            profile: None,
            debug: cfg!(debug_assertions),
            robustness: Robustness::NotRobust,
            vsync: false,
        }
    }
}
