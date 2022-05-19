use crate::{backend::window::PlatformIcon, Error};

#[repr(C)]
#[derive(Debug)]
pub(crate) struct Pixel {
    pub(crate) r: u8,
    pub(crate) g: u8,
    pub(crate) b: u8,
    pub(crate) a: u8,
}

pub(crate) const PIXEL_SIZE: usize = std::mem::size_of::<Pixel>();

/// An icon used for the window titlebar, taskbar, etc.
#[derive(Clone)]
pub struct Icon {
    pub(crate) inner: PlatformIcon,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RgbaIcon {
    pub(crate) rgba: Vec<u8>,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

/// For platforms which don't have window icons (e.g. web)
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NoIcon;

impl RgbaIcon {
    /// Creates an `Icon` from 32bpp RGBA data.
    ///
    /// The length of `rgba` must be divisible by 4, and `width * height` must equal
    /// `rgba.len() / 4`. Otherwise, this will return a `BadIcon` error.
    pub fn from_rgba(rgba: Vec<u8>, width: u32, height: u32) -> Result<Self, Error> {
        if rgba.len() % PIXEL_SIZE != 0 {
            return Err(Error::Other(anyhow::anyhow!("bad icon").into()));
        }
        let pixel_count = rgba.len() / PIXEL_SIZE;
        if pixel_count != (width * height) as usize {
            Err(Error::Other(anyhow::anyhow!("bad icon").into()))
        } else {
            Ok(RgbaIcon {
                rgba,
                width,
                height,
            })
        }
    }
}

impl NoIcon {
    pub fn from_rgba(rgba: Vec<u8>, width: u32, height: u32) -> Result<Self, Error> {
        // Create the rgba icon anyway to validate the input
        let _ = RgbaIcon::from_rgba(rgba, width, height)?;
        Ok(NoIcon)
    }
}
