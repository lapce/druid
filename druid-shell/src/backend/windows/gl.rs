use std::{
    ffi::{CStr, CString, OsStr},
    marker::PhantomData,
    os::{raw, windows::prelude::OsStrExt},
};

use winapi::{
    shared::{
        minwindef::{HMODULE, UINT},
        ntdef::LPCWSTR,
        windef::{HDC, HGLRC, HWND},
    },
    um::{
        libloaderapi::{GetModuleHandleW, GetProcAddress, LoadLibraryW},
        wingdi::{
            ChoosePixelFormat, DescribePixelFormat, GetPixelFormat, SetPixelFormat, SwapBuffers,
            PFD_DOUBLEBUFFER, PFD_DRAW_TO_WINDOW, PFD_GENERIC_FORMAT, PFD_MAIN_PLANE, PFD_STEREO,
            PFD_SUPPORT_OPENGL, PFD_TYPE_RGBA, PIXELFORMATDESCRIPTOR,
        },
        winuser::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, GetClassInfoExW, GetClassNameW, GetDC,
            GetWindowPlacement, RegisterClassExW, CW_USEDEFAULT, WINDOWPLACEMENT, WNDCLASSEXW,
            WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_EX_APPWINDOW, WS_POPUP,
        },
    },
};

use crate::{
    gl::{
        Api, GlAttributes, GlProfile, GlRequest, PixelFormat, PixelFormatRequirements,
        ReleaseBehavior, Robustness,
    },
    Error,
};

/// A guard for when you want to make the context current. Destroying the guard
/// restores the previously-current context.
#[derive(Debug)]
pub struct CurrentContextGuard<'a, 'b> {
    previous_hdc: HDC,
    previous_hglrc: HGLRC,
    marker1: PhantomData<&'a ()>,
    marker2: PhantomData<&'b ()>,
}

impl<'a, 'b> CurrentContextGuard<'a, 'b> {
    pub unsafe fn make_current(
        hdc: HDC,
        context: HGLRC,
    ) -> Result<CurrentContextGuard<'a, 'b>, Error> {
        let previous_hdc = glutin_wgl_sys::wgl::GetCurrentDC() as HDC;
        let previous_hglrc = glutin_wgl_sys::wgl::GetCurrentContext() as HGLRC;

        let result = glutin_wgl_sys::wgl::MakeCurrent(hdc as *const _, context as *const _);
        if result == 0 {
            return Err(anyhow::anyhow!(
                "wglMakeCurrent function failed: {}",
                std::io::Error::last_os_error()
            )
            .into());
        }

        Ok(CurrentContextGuard {
            previous_hdc,
            previous_hglrc,
            marker1: PhantomData,
            marker2: PhantomData,
        })
    }
}

impl<'a, 'b> Drop for CurrentContextGuard<'a, 'b> {
    fn drop(&mut self) {
        unsafe {
            glutin_wgl_sys::wgl::MakeCurrent(
                self.previous_hdc as *const raw::c_void,
                self.previous_hglrc as *const raw::c_void,
            );
        }
    }
}

#[derive(Debug)]
pub struct Context {
    context: ContextWrapper,

    hdc: HDC,

    /// Bound to `opengl32.dll`.
    ///
    /// `wglGetProcAddress` returns null for GL 1.1 functions because they are
    ///  already defined by the system. This module contains them.
    gl_library: HMODULE,

    /// The pixel format that has been used to create this context.
    pixel_format: PixelFormat,
}

/// A simple wrapper that destroys the window when it is destroyed.
#[derive(Debug)]
struct WindowWrapper(HWND, HDC);

impl Drop for WindowWrapper {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.0);
        }
    }
}

/// Wraps around a context so that it is destroyed when necessary.
#[derive(Debug)]
struct ContextWrapper(HGLRC);

impl Drop for ContextWrapper {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            glutin_wgl_sys::wgl::DeleteContext(self.0 as *const _);
        }
    }
}

impl Context {
    /// Attempt to build a new WGL context on a window.
    ///
    /// # Unsafety
    ///
    /// The `window` must continue to exist as long as the resulting `Context`
    /// exists.
    #[inline]
    pub unsafe fn new(
        pf_reqs: &PixelFormatRequirements,
        opengl: &GlAttributes,
        win: HWND,
    ) -> Result<Context, Error> {
        let hdc = GetDC(win);
        if hdc.is_null() {
            let err = Err(anyhow::anyhow!(
                "GetDC function failed: {}",
                std::io::Error::last_os_error()
            )
            .into());
            return err;
        }

        // loading the functions that are not guaranteed to be supported
        let extra_functions = load_extra_functions(win)?;

        // getting the list of the supported extensions
        let extensions = if extra_functions.GetExtensionsStringARB.is_loaded() {
            let data = extra_functions.GetExtensionsStringARB(hdc as *const _);
            let data = CStr::from_ptr(data).to_bytes().to_vec();
            String::from_utf8(data).unwrap()
        } else if extra_functions.GetExtensionsStringEXT.is_loaded() {
            let data = extra_functions.GetExtensionsStringEXT();
            let data = CStr::from_ptr(data).to_bytes().to_vec();
            String::from_utf8(data).unwrap()
        } else {
            format!("")
        };

        let use_arb_for_pixel_format = extensions
            .split(' ')
            .find(|&i| i == "WGL_ARB_pixel_format")
            .is_some();

        // calling SetPixelFormat, if not already done
        let mut pixel_format_id = GetPixelFormat(hdc);
        if pixel_format_id == 0 {
            let id = if use_arb_for_pixel_format {
                choose_arb_pixel_format_id(&extra_functions, &extensions, hdc, pf_reqs)
                    .map_err(|_| anyhow::anyhow!("no avaible pixel format"))?
            } else {
                choose_native_pixel_format_id(hdc, pf_reqs)
                    .map_err(|_| anyhow::anyhow!("no avaible pixel format"))?
            };

            set_pixel_format(hdc, id)?;
            pixel_format_id = id;
        }

        let pixel_format = if use_arb_for_pixel_format {
            choose_arb_pixel_format(&extra_functions, &extensions, hdc, pixel_format_id)
                .map_err(|_| anyhow::anyhow!("no avaible pixel format"))?
        } else {
            choose_native_pixel_format(hdc, pf_reqs, pixel_format_id)
                .map_err(|_| anyhow::anyhow!("no avaible pixel format"))?
        };

        // creating the OpenGL context
        let context = create_context(
            Some((&extra_functions, pf_reqs, opengl, &extensions)),
            win,
            hdc,
        )?;

        // loading the opengl32 module
        let gl_library = load_opengl32_dll()?;

        // handling vsync
        if extensions
            .split(' ')
            .find(|&i| i == "WGL_EXT_swap_control")
            .is_some()
        {
            let _guard = CurrentContextGuard::make_current(hdc, context.0)?;

            if extra_functions.SwapIntervalEXT(if opengl.vsync { 1 } else { 0 }) == 0 {
                return Err(anyhow::anyhow!("wglSwapIntervalEXT failed".to_string(),).into());
            }
        }

        Ok(Context {
            context,
            hdc,
            gl_library,
            pixel_format,
        })
    }

    /// Returns the raw HGLRC.
    #[inline]
    pub fn get_hglrc(&self) -> HGLRC {
        self.context.0
    }

    #[inline]
    pub unsafe fn make_current(&self) -> Result<(), Error> {
        if glutin_wgl_sys::wgl::MakeCurrent(self.hdc as *const _, self.context.0 as *const _) != 0 {
            Ok(())
        } else {
            Err(anyhow::anyhow!(std::io::Error::last_os_error()).into())
        }
    }

    #[inline]
    pub unsafe fn make_not_current(&self) -> Result<(), Error> {
        if self.is_current()
            && glutin_wgl_sys::wgl::MakeCurrent(self.hdc as *const _, std::ptr::null()) != 0
        {
            Ok(())
        } else {
            Err(anyhow::anyhow!(std::io::Error::last_os_error()).into())
        }
    }

    #[inline]
    pub fn is_current(&self) -> bool {
        unsafe { glutin_wgl_sys::wgl::GetCurrentContext() == self.context.0 as *const raw::c_void }
    }

    pub fn get_proc_address(&self, addr: &str) -> *const core::ffi::c_void {
        let addr = CString::new(addr.as_bytes()).unwrap();
        let addr = addr.as_ptr();

        unsafe {
            let p = glutin_wgl_sys::wgl::GetProcAddress(addr) as *const core::ffi::c_void;
            if !p.is_null() {
                return p;
            }
            GetProcAddress(self.gl_library, addr) as *const _
        }
    }

    #[inline]
    pub fn swap_buffers(&self) -> Result<(), Error> {
        // TODO: decide how to handle the error
        // if unsafe { SwapBuffers(self.hdc) } != 0 {
        // Ok(())
        // } else {
        // Err(ContextError::IoError(std::io::Error::last_os_error()))
        // }
        unsafe { SwapBuffers(self.hdc) };
        Ok(())
    }

    #[inline]
    pub fn get_api(&self) -> Api {
        // FIXME: can be opengl es
        Api::OpenGl
    }

    #[inline]
    pub fn get_pixel_format(&self) -> PixelFormat {
        self.pixel_format.clone()
    }
}

/// Loads the WGL functions that are not guaranteed to be supported.
///
/// The `window` must be passed because the driver can vary depending on the
/// window's characteristics.
unsafe fn load_extra_functions(win: HWND) -> Result<glutin_wgl_sys::wgl_extra::Wgl, Error> {
    let (ex_style, style) = (
        WS_EX_APPWINDOW,
        WS_POPUP | WS_CLIPSIBLINGS | WS_CLIPCHILDREN,
    );

    // creating a dummy invisible window
    let dummy_win = {
        // getting the rect of the real window
        let rect = {
            let mut placement: WINDOWPLACEMENT = std::mem::zeroed();
            placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as UINT;
            if GetWindowPlacement(win, &mut placement) == 0 {
                panic!();
            }
            placement.rcNormalPosition
        };

        // getting the class name of the real window
        let mut class_name = [0u16; 128];
        if GetClassNameW(win, class_name.as_mut_ptr(), 128) == 0 {
            return Err(anyhow::anyhow!(
                "GetClassNameW function failed: {}",
                std::io::Error::last_os_error()
            )
            .into());
        }

        // access to class information of the real window
        let instance = GetModuleHandleW(std::ptr::null());
        let mut class: WNDCLASSEXW = std::mem::zeroed();

        if GetClassInfoExW(instance, class_name.as_ptr(), &mut class) == 0 {
            return Err(anyhow::anyhow!(
                "GetClassInfoExW function failed: {}",
                std::io::Error::last_os_error()
            )
            .into());
        }

        // register a new class for the dummy window,
        // similar to the class of the real window but with a different callback
        let class_name = OsStr::new("WglDummy Class")
            .encode_wide()
            .chain(Some(0).into_iter())
            .collect::<Vec<_>>();

        class.cbSize = std::mem::size_of::<WNDCLASSEXW>() as UINT;
        class.lpszClassName = class_name.as_ptr();
        class.lpfnWndProc = Some(DefWindowProcW);

        // this shouldn't fail if the registration of the real window class
        // worked. multiple registrations of the window class trigger an
        // error which we want to ignore silently (e.g for multi-window
        // setups)
        RegisterClassExW(&class);

        // this dummy window should match the real one enough to get the same
        // OpenGL driver
        let title = OsStr::new("dummy window")
            .encode_wide()
            .chain(Some(0).into_iter())
            .collect::<Vec<_>>();
        let win = CreateWindowExW(
            ex_style,
            class_name.as_ptr(),
            title.as_ptr() as LPCWSTR,
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            rect.right - rect.left,
            rect.bottom - rect.top,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null_mut(),
        );

        if win.is_null() {
            return Err(anyhow::anyhow!(
                "CreateWindowEx function failed: {}",
                std::io::Error::last_os_error()
            )
            .into());
        }

        let hdc = GetDC(win);
        if hdc.is_null() {
            let err = Err(anyhow::anyhow!(
                "GetDC function failed: {}",
                std::io::Error::last_os_error()
            )
            .into());
            return err;
        }

        WindowWrapper(win, hdc)
    };

    // getting the pixel format that we will use and setting it
    {
        let id = choose_dummy_pixel_format(dummy_win.1)?;
        set_pixel_format(dummy_win.1, id)?;
    }

    // creating the dummy OpenGL context and making it current
    let dummy_ctx = create_context(None, dummy_win.0, dummy_win.1)?;
    let _current_context = CurrentContextGuard::make_current(dummy_win.1, dummy_ctx.0)?;

    // loading the extra WGL functions
    Ok(glutin_wgl_sys::wgl_extra::Wgl::load_with(|addr| {
        let addr = CString::new(addr.as_bytes()).unwrap();
        let addr = addr.as_ptr();
        glutin_wgl_sys::wgl::GetProcAddress(addr) as *const raw::c_void
    }))
}

/// Creates an OpenGL context.
///
/// If `extra` is `Some`, this function will attempt to use the latest WGL
/// functions to create the context.
///
/// Otherwise, only the basic API will be used and the chances of
/// `CreationError::NotSupported` being returned increase.
unsafe fn create_context(
    extra: Option<(
        &glutin_wgl_sys::wgl_extra::Wgl,
        &PixelFormatRequirements,
        &GlAttributes,
        &str,
    )>,
    _: HWND,
    hdc: HDC,
) -> Result<ContextWrapper, Error> {
    if let Some((extra_functions, _pf_reqs, opengl, extensions)) = extra {
        if extensions
            .split(' ')
            .find(|&i| i == "WGL_ARB_create_context")
            .is_some()
        {
            let mut attributes = Vec::new();

            match opengl.version {
                GlRequest::Latest => {}
                GlRequest::Specific(Api::OpenGl, (major, minor)) => {
                    attributes
                        .push(glutin_wgl_sys::wgl_extra::CONTEXT_MAJOR_VERSION_ARB as raw::c_int);
                    attributes.push(major as raw::c_int);
                    attributes
                        .push(glutin_wgl_sys::wgl_extra::CONTEXT_MINOR_VERSION_ARB as raw::c_int);
                    attributes.push(minor as raw::c_int);
                }
                GlRequest::Specific(Api::OpenGlEs, (major, minor)) => {
                    if extensions
                        .split(' ')
                        .find(|&i| i == "WGL_EXT_create_context_es2_profile")
                        .is_some()
                    {
                        attributes.push(
                            glutin_wgl_sys::wgl_extra::CONTEXT_PROFILE_MASK_ARB as raw::c_int,
                        );
                        attributes.push(
                            glutin_wgl_sys::wgl_extra::CONTEXT_ES2_PROFILE_BIT_EXT as raw::c_int,
                        );
                    } else {
                        return Err(anyhow::anyhow!("OpenGL version not supported").into());
                    }

                    attributes
                        .push(glutin_wgl_sys::wgl_extra::CONTEXT_MAJOR_VERSION_ARB as raw::c_int);
                    attributes.push(major as raw::c_int);
                    attributes
                        .push(glutin_wgl_sys::wgl_extra::CONTEXT_MINOR_VERSION_ARB as raw::c_int);
                    attributes.push(minor as raw::c_int);
                }
                GlRequest::Specific(_, _) => {
                    return Err(anyhow::anyhow!("OpenGL version not supported").into());
                }
                GlRequest::GlThenGles {
                    opengl_version: (major, minor),
                    ..
                } => {
                    attributes
                        .push(glutin_wgl_sys::wgl_extra::CONTEXT_MAJOR_VERSION_ARB as raw::c_int);
                    attributes.push(major as raw::c_int);
                    attributes
                        .push(glutin_wgl_sys::wgl_extra::CONTEXT_MINOR_VERSION_ARB as raw::c_int);
                    attributes.push(minor as raw::c_int);
                }
            }

            if let Some(profile) = opengl.profile {
                if extensions
                    .split(' ')
                    .find(|&i| i == "WGL_ARB_create_context_profile")
                    .is_some()
                {
                    let flag = match profile {
                        GlProfile::Compatibility => {
                            glutin_wgl_sys::wgl_extra::CONTEXT_COMPATIBILITY_PROFILE_BIT_ARB
                        }
                        GlProfile::Core => glutin_wgl_sys::wgl_extra::CONTEXT_CORE_PROFILE_BIT_ARB,
                    };
                    attributes
                        .push(glutin_wgl_sys::wgl_extra::CONTEXT_PROFILE_MASK_ARB as raw::c_int);
                    attributes.push(flag as raw::c_int);
                } else {
                    return Err(anyhow::anyhow!(
                        "required extension \"WGL_ARB_create_context_profile\" not found"
                            .to_string(),
                    )
                    .into());
                }
            }

            let flags = {
                let mut flags = 0;

                // robustness
                if extensions
                    .split(' ')
                    .find(|&i| i == "WGL_ARB_create_context_robustness")
                    .is_some()
                {
                    match opengl.robustness {
                        Robustness::RobustNoResetNotification
                        | Robustness::TryRobustNoResetNotification => {
                            attributes.push(
                                glutin_wgl_sys::wgl_extra::CONTEXT_RESET_NOTIFICATION_STRATEGY_ARB
                                    as raw::c_int,
                            );
                            attributes.push(
                                glutin_wgl_sys::wgl_extra::NO_RESET_NOTIFICATION_ARB as raw::c_int,
                            );
                            flags = flags
                                | glutin_wgl_sys::wgl_extra::CONTEXT_ROBUST_ACCESS_BIT_ARB
                                    as raw::c_int;
                        }
                        Robustness::RobustLoseContextOnReset
                        | Robustness::TryRobustLoseContextOnReset => {
                            attributes.push(
                                glutin_wgl_sys::wgl_extra::CONTEXT_RESET_NOTIFICATION_STRATEGY_ARB
                                    as raw::c_int,
                            );
                            attributes.push(
                                glutin_wgl_sys::wgl_extra::LOSE_CONTEXT_ON_RESET_ARB as raw::c_int,
                            );
                            flags = flags
                                | glutin_wgl_sys::wgl_extra::CONTEXT_ROBUST_ACCESS_BIT_ARB
                                    as raw::c_int;
                        }
                        Robustness::NotRobust => (),
                        Robustness::NoError => (),
                    }
                } else {
                    match opengl.robustness {
                        Robustness::RobustNoResetNotification
                        | Robustness::RobustLoseContextOnReset => {
                            return Err(anyhow::anyhow!("Robustness not supported").into());
                        }
                        _ => (),
                    }
                }

                if opengl.debug {
                    flags = flags | glutin_wgl_sys::wgl_extra::CONTEXT_DEBUG_BIT_ARB as raw::c_int;
                }

                flags
            };

            attributes.push(glutin_wgl_sys::wgl_extra::CONTEXT_FLAGS_ARB as raw::c_int);
            attributes.push(flags);

            attributes.push(0);

            let ctx = extra_functions.CreateContextAttribsARB(
                hdc as *const raw::c_void,
                std::ptr::null_mut() as *const raw::c_void,
                attributes.as_ptr(),
            );

            if ctx.is_null() {
                return Err(anyhow::anyhow!(
                    "wglCreateContextAttribsARB failed: {}",
                    std::io::Error::last_os_error()
                )
                .into());
            } else {
                return Ok(ContextWrapper(ctx as HGLRC));
            }
        }
    }

    let ctx = glutin_wgl_sys::wgl::CreateContext(hdc as *const raw::c_void);
    if ctx.is_null() {
        return Err(anyhow::anyhow!(
            "wglCreateContext failed: {}",
            std::io::Error::last_os_error()
        )
        .into());
    }

    Ok(ContextWrapper(ctx as HGLRC))
}

/// Calls `SetPixelFormat` on a window.
unsafe fn set_pixel_format(hdc: HDC, id: raw::c_int) -> Result<(), Error> {
    let mut output: PIXELFORMATDESCRIPTOR = std::mem::zeroed();

    if DescribePixelFormat(
        hdc,
        id,
        std::mem::size_of::<PIXELFORMATDESCRIPTOR>() as UINT,
        &mut output,
    ) == 0
    {
        return Err(anyhow::anyhow!(
            "DescribePixelFormat function failed: {}",
            std::io::Error::last_os_error()
        )
        .into());
    }

    if SetPixelFormat(hdc, id, &output) == 0 {
        return Err(anyhow::anyhow!(
            "SetPixelFormat function failed: {}",
            std::io::Error::last_os_error()
        )
        .into());
    }

    Ok(())
}

/// This function chooses a pixel format that is likely to be provided by
/// the main video driver of the system.
fn choose_dummy_pixel_format(hdc: HDC) -> Result<raw::c_int, Error> {
    // building the descriptor to pass to ChoosePixelFormat
    let descriptor = PIXELFORMATDESCRIPTOR {
        nSize: std::mem::size_of::<PIXELFORMATDESCRIPTOR>() as u16,
        nVersion: 1,
        dwFlags: PFD_DRAW_TO_WINDOW | PFD_SUPPORT_OPENGL | PFD_DOUBLEBUFFER,
        iPixelType: PFD_TYPE_RGBA,
        cColorBits: 24,
        cRedBits: 0,
        cRedShift: 0,
        cGreenBits: 0,
        cGreenShift: 0,
        cBlueBits: 0,
        cBlueShift: 0,
        cAlphaBits: 8,
        cAlphaShift: 0,
        cAccumBits: 0,
        cAccumRedBits: 0,
        cAccumGreenBits: 0,
        cAccumBlueBits: 0,
        cAccumAlphaBits: 0,
        cDepthBits: 24,
        cStencilBits: 8,
        cAuxBuffers: 0,
        iLayerType: PFD_MAIN_PLANE,
        bReserved: 0,
        dwLayerMask: 0,
        dwVisibleMask: 0,
        dwDamageMask: 0,
    };

    // now querying
    let pf_id = unsafe { ChoosePixelFormat(hdc, &descriptor) };
    if pf_id == 0 {
        return Err(anyhow::anyhow!("No available pixel format".to_owned(),).into());
    }

    Ok(pf_id)
}

/// Chooses a pixel formats without using WGL.
///
/// Gives less precise results than `enumerate_arb_pixel_formats`.
unsafe fn choose_native_pixel_format_id(
    hdc: HDC,
    pf_reqs: &PixelFormatRequirements,
) -> Result<raw::c_int, ()> {
    // TODO: hardware acceleration is not handled

    // handling non-supported stuff
    if pf_reqs.float_color_buffer {
        return Err(());
    }

    match pf_reqs.multisampling {
        Some(0) => (),
        None => (),
        Some(_) => return Err(()),
    };

    if pf_reqs.stereoscopy {
        return Err(());
    }

    if pf_reqs.srgb {
        return Err(());
    }

    if pf_reqs.release_behavior != ReleaseBehavior::Flush {
        return Err(());
    }

    // building the descriptor to pass to ChoosePixelFormat
    let descriptor = PIXELFORMATDESCRIPTOR {
        nSize: std::mem::size_of::<PIXELFORMATDESCRIPTOR>() as u16,
        nVersion: 1,
        dwFlags: {
            let f1 = match pf_reqs.double_buffer {
                None => PFD_DOUBLEBUFFER, /* Should be PFD_DOUBLEBUFFER_DONTCARE after you can choose */
                Some(true) => PFD_DOUBLEBUFFER,
                Some(false) => 0,
            };

            let f2 = if pf_reqs.stereoscopy { PFD_STEREO } else { 0 };

            PFD_DRAW_TO_WINDOW | PFD_SUPPORT_OPENGL | f1 | f2
        },
        iPixelType: PFD_TYPE_RGBA,
        cColorBits: pf_reqs.color_bits.unwrap_or(0),
        cRedBits: 0,
        cRedShift: 0,
        cGreenBits: 0,
        cGreenShift: 0,
        cBlueBits: 0,
        cBlueShift: 0,
        cAlphaBits: pf_reqs.alpha_bits.unwrap_or(0),
        cAlphaShift: 0,
        cAccumBits: 0,
        cAccumRedBits: 0,
        cAccumGreenBits: 0,
        cAccumBlueBits: 0,
        cAccumAlphaBits: 0,
        cDepthBits: pf_reqs.depth_bits.unwrap_or(0),
        cStencilBits: pf_reqs.stencil_bits.unwrap_or(0),
        cAuxBuffers: 0,
        iLayerType: PFD_MAIN_PLANE,
        bReserved: 0,
        dwLayerMask: 0,
        dwVisibleMask: 0,
        dwDamageMask: 0,
    };

    // now querying
    let pf_id = ChoosePixelFormat(hdc, &descriptor);
    if pf_id == 0 {
        return Err(());
    }

    Ok(pf_id)
}

unsafe fn choose_native_pixel_format(
    hdc: HDC,
    pf_reqs: &PixelFormatRequirements,
    pf_id: raw::c_int,
) -> Result<PixelFormat, ()> {
    // querying back the capabilities of what windows told us
    let mut output: PIXELFORMATDESCRIPTOR = std::mem::zeroed();
    if DescribePixelFormat(
        hdc,
        pf_id,
        std::mem::size_of::<PIXELFORMATDESCRIPTOR>() as u32,
        &mut output,
    ) == 0
    {
        return Err(());
    }

    // windows may return us a non-conforming pixel format if none are
    // supported, so we have to check this
    if (output.dwFlags & PFD_DRAW_TO_WINDOW) == 0 {
        return Err(());
    }
    if (output.dwFlags & PFD_SUPPORT_OPENGL) == 0 {
        return Err(());
    }
    if output.iPixelType != PFD_TYPE_RGBA {
        return Err(());
    }

    let pf_desc = PixelFormat {
        hardware_accelerated: (output.dwFlags & PFD_GENERIC_FORMAT) == 0,
        color_bits: output.cRedBits + output.cGreenBits + output.cBlueBits,
        alpha_bits: output.cAlphaBits,
        depth_bits: output.cDepthBits,
        stencil_bits: output.cStencilBits,
        stereoscopy: (output.dwFlags & PFD_STEREO) != 0,
        double_buffer: (output.dwFlags & PFD_DOUBLEBUFFER) != 0,
        multisampling: None,
        srgb: false,
    };

    if pf_desc.alpha_bits < pf_reqs.alpha_bits.unwrap_or(0) {
        return Err(());
    }
    if pf_desc.depth_bits < pf_reqs.depth_bits.unwrap_or(0) {
        return Err(());
    }
    if pf_desc.stencil_bits < pf_reqs.stencil_bits.unwrap_or(0) {
        return Err(());
    }
    if pf_desc.color_bits < pf_reqs.color_bits.unwrap_or(0) {
        return Err(());
    }
    if let Some(req) = pf_reqs.hardware_accelerated {
        if pf_desc.hardware_accelerated != req {
            return Err(());
        }
    }
    if let Some(req) = pf_reqs.double_buffer {
        if pf_desc.double_buffer != req {
            return Err(());
        }
    }

    Ok(pf_desc)
}

/// Enumerates the list of pixel formats by using extra WGL functions.
///
/// Gives more precise results than `enumerate_native_pixel_formats`.
unsafe fn choose_arb_pixel_format_id(
    extra: &glutin_wgl_sys::wgl_extra::Wgl,
    extensions: &str,
    hdc: HDC,
    pf_reqs: &PixelFormatRequirements,
) -> Result<raw::c_int, ()> {
    let descriptor = {
        let mut out: Vec<raw::c_int> = Vec::with_capacity(37);

        out.push(glutin_wgl_sys::wgl_extra::DRAW_TO_WINDOW_ARB as raw::c_int);
        out.push(1);

        out.push(glutin_wgl_sys::wgl_extra::SUPPORT_OPENGL_ARB as raw::c_int);
        out.push(1);

        out.push(glutin_wgl_sys::wgl_extra::PIXEL_TYPE_ARB as raw::c_int);
        if pf_reqs.float_color_buffer {
            if extensions
                .split(' ')
                .find(|&i| i == "WGL_ARB_pixel_format_float")
                .is_some()
            {
                out.push(glutin_wgl_sys::wgl_extra::TYPE_RGBA_FLOAT_ARB as raw::c_int);
            } else {
                return Err(());
            }
        } else {
            out.push(glutin_wgl_sys::wgl_extra::TYPE_RGBA_ARB as raw::c_int);
        }

        if let Some(hardware_accelerated) = pf_reqs.hardware_accelerated {
            out.push(glutin_wgl_sys::wgl_extra::ACCELERATION_ARB as raw::c_int);
            out.push(if hardware_accelerated {
                glutin_wgl_sys::wgl_extra::FULL_ACCELERATION_ARB as raw::c_int
            } else {
                glutin_wgl_sys::wgl_extra::NO_ACCELERATION_ARB as raw::c_int
            });
        }

        if let Some(color) = pf_reqs.color_bits {
            out.push(glutin_wgl_sys::wgl_extra::COLOR_BITS_ARB as raw::c_int);
            out.push(color as raw::c_int);
        }

        if let Some(alpha) = pf_reqs.alpha_bits {
            out.push(glutin_wgl_sys::wgl_extra::ALPHA_BITS_ARB as raw::c_int);
            out.push(alpha as raw::c_int);
        }

        if let Some(depth) = pf_reqs.depth_bits {
            out.push(glutin_wgl_sys::wgl_extra::DEPTH_BITS_ARB as raw::c_int);
            out.push(depth as raw::c_int);
        }

        if let Some(stencil) = pf_reqs.stencil_bits {
            out.push(glutin_wgl_sys::wgl_extra::STENCIL_BITS_ARB as raw::c_int);
            out.push(stencil as raw::c_int);
        }

        // Prefer double buffering if unspecified (probably shouldn't once you
        // can choose)
        let double_buffer = pf_reqs.double_buffer.unwrap_or(true);
        out.push(glutin_wgl_sys::wgl_extra::DOUBLE_BUFFER_ARB as raw::c_int);
        out.push(if double_buffer { 1 } else { 0 });

        if let Some(multisampling) = pf_reqs.multisampling {
            if extensions
                .split(' ')
                .find(|&i| i == "WGL_ARB_multisample")
                .is_some()
            {
                out.push(glutin_wgl_sys::wgl_extra::SAMPLE_BUFFERS_ARB as raw::c_int);
                out.push(if multisampling == 0 { 0 } else { 1 });
                out.push(glutin_wgl_sys::wgl_extra::SAMPLES_ARB as raw::c_int);
                out.push(multisampling as raw::c_int);
            } else {
                return Err(());
            }
        }

        out.push(glutin_wgl_sys::wgl_extra::STEREO_ARB as raw::c_int);
        out.push(if pf_reqs.stereoscopy { 1 } else { 0 });

        // WGL_*_FRAMEBUFFER_SRGB might be assumed to be true if not listed;
        // so it's best to list it out and set its value as necessary.
        if extensions
            .split(' ')
            .find(|&i| i == "WGL_ARB_framebuffer_sRGB")
            .is_some()
        {
            out.push(glutin_wgl_sys::wgl_extra::FRAMEBUFFER_SRGB_CAPABLE_ARB as raw::c_int);
            out.push(pf_reqs.srgb as raw::c_int);
        } else if extensions
            .split(' ')
            .find(|&i| i == "WGL_EXT_framebuffer_sRGB")
            .is_some()
        {
            out.push(glutin_wgl_sys::wgl_extra::FRAMEBUFFER_SRGB_CAPABLE_EXT as raw::c_int);
            out.push(pf_reqs.srgb as raw::c_int);
        } else if pf_reqs.srgb {
            return Err(());
        }

        match pf_reqs.release_behavior {
            ReleaseBehavior::Flush => (),
            ReleaseBehavior::None => {
                if extensions
                    .split(' ')
                    .find(|&i| i == "WGL_ARB_context_flush_control")
                    .is_some()
                {
                    out.push(glutin_wgl_sys::wgl_extra::CONTEXT_RELEASE_BEHAVIOR_ARB as raw::c_int);
                    out.push(
                        glutin_wgl_sys::wgl_extra::CONTEXT_RELEASE_BEHAVIOR_NONE_ARB as raw::c_int,
                    );
                }
            }
        }

        out.push(0);
        out
    };

    let mut format_id = std::mem::zeroed();
    let mut num_formats = std::mem::zeroed();
    if extra.ChoosePixelFormatARB(
        hdc as *const _,
        descriptor.as_ptr(),
        std::ptr::null(),
        1,
        &mut format_id,
        &mut num_formats,
    ) == 0
    {
        return Err(());
    }

    if num_formats == 0 {
        return Err(());
    }

    Ok(format_id)
}

unsafe fn choose_arb_pixel_format(
    extra: &glutin_wgl_sys::wgl_extra::Wgl,
    extensions: &str,
    hdc: HDC,
    format_id: raw::c_int,
) -> Result<PixelFormat, ()> {
    let get_info = |attrib: u32| {
        let mut value = std::mem::zeroed();
        extra.GetPixelFormatAttribivARB(
            hdc as *const _,
            format_id as raw::c_int,
            0,
            1,
            [attrib as raw::c_int].as_ptr(),
            &mut value,
        );
        value as u32
    };

    let pf_desc = PixelFormat {
        hardware_accelerated: get_info(glutin_wgl_sys::wgl_extra::ACCELERATION_ARB)
            != glutin_wgl_sys::wgl_extra::NO_ACCELERATION_ARB,
        color_bits: get_info(glutin_wgl_sys::wgl_extra::RED_BITS_ARB) as u8
            + get_info(glutin_wgl_sys::wgl_extra::GREEN_BITS_ARB) as u8
            + get_info(glutin_wgl_sys::wgl_extra::BLUE_BITS_ARB) as u8,
        alpha_bits: get_info(glutin_wgl_sys::wgl_extra::ALPHA_BITS_ARB) as u8,
        depth_bits: get_info(glutin_wgl_sys::wgl_extra::DEPTH_BITS_ARB) as u8,
        stencil_bits: get_info(glutin_wgl_sys::wgl_extra::STENCIL_BITS_ARB) as u8,
        stereoscopy: get_info(glutin_wgl_sys::wgl_extra::STEREO_ARB) != 0,
        double_buffer: get_info(glutin_wgl_sys::wgl_extra::DOUBLE_BUFFER_ARB) != 0,
        multisampling: {
            if extensions
                .split(' ')
                .find(|&i| i == "WGL_ARB_multisample")
                .is_some()
            {
                match get_info(glutin_wgl_sys::wgl_extra::SAMPLES_ARB) {
                    0 => None,
                    a => Some(a as u16),
                }
            } else {
                None
            }
        },
        srgb: if extensions
            .split(' ')
            .find(|&i| i == "WGL_ARB_framebuffer_sRGB")
            .is_some()
        {
            get_info(glutin_wgl_sys::wgl_extra::FRAMEBUFFER_SRGB_CAPABLE_ARB) != 0
        } else if extensions
            .split(' ')
            .find(|&i| i == "WGL_EXT_framebuffer_sRGB")
            .is_some()
        {
            get_info(glutin_wgl_sys::wgl_extra::FRAMEBUFFER_SRGB_CAPABLE_EXT) != 0
        } else {
            false
        },
    };

    Ok(pf_desc)
}

/// Loads the `opengl32.dll` library.
unsafe fn load_opengl32_dll() -> Result<HMODULE, Error> {
    let name = OsStr::new("opengl32.dll")
        .encode_wide()
        .chain(Some(0).into_iter())
        .collect::<Vec<_>>();

    let lib = LoadLibraryW(name.as_ptr());

    if lib.is_null() {
        return Err(anyhow::anyhow!(
            "LoadLibrary function failed: {}",
            std::io::Error::last_os_error()
        )
        .into());
    }

    Ok(lib)
}
