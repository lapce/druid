use crate::{backend::window::PlatformIcon, Error};

/// An icon used for the window titlebar, taskbar, etc.
#[derive(Clone, PartialEq)]
pub struct Icon {
    pub(crate) inner: PlatformIcon,
}

impl Icon {
    /// Creates an `Icon` from 32bpp RGBA data.
    ///
    /// The length of `rgba` must be divisible by 4, and `width * height` must equal
    /// `rgba.len() / 4`. Otherwise, this will return a `BadIcon` error.
    pub fn from_rgba(rgba: Vec<u8>, width: u32, height: u32) -> Result<Self, Error> {
        Ok(Icon {
            inner: PlatformIcon::from_rgba(rgba, width, height)?,
        })
    }
}
