use crate::backend::PlatformIcon;

/// An icon used for the window titlebar, taskbar, etc.
#[derive(Clone)]
pub struct Icon {
    pub(crate) inner: PlatformIcon,
}

/// For platforms which don't have window icons (e.g. web)
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NoIcon;
