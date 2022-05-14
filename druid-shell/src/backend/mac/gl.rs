use std::str::FromStr;

use anyhow::anyhow;
use cgl::{kCGLCECrashOnRemovedFunctions, CGLEnable};
use cocoa::appkit::NSOpenGLPixelFormatAttribute::{
    NSOpenGLPFAAccelerated, NSOpenGLPFAAllowOfflineRenderers, NSOpenGLPFAAlphaSize,
    NSOpenGLPFAClosestPolicy, NSOpenGLPFAColorFloat, NSOpenGLPFAColorSize, NSOpenGLPFADepthSize,
    NSOpenGLPFADoubleBuffer, NSOpenGLPFAMultisample, NSOpenGLPFAOpenGLProfile,
    NSOpenGLPFASampleBuffers, NSOpenGLPFASamples, NSOpenGLPFAStencilSize,
};
use cocoa::appkit::{self, NSOpenGLContext, NSOpenGLPFAOpenGLProfiles, NSOpenGLPixelFormat};
use cocoa::base::{id, nil};
use core_foundation::base::TCFType;
use core_foundation::bundle::{CFBundleGetBundleWithIdentifier, CFBundleGetFunctionPointerForName};
use core_foundation::string::CFString;
use objc::rc::WeakPtr;
use objc::{msg_send, sel, sel_impl};
use NSOpenGLPFAOpenGLProfiles::{
    NSOpenGLProfileVersion3_2Core, NSOpenGLProfileVersion4_1Core, NSOpenGLProfileVersionLegacy,
};

use crate::gl::{
    GlAttributes, GlProfile, GlRequest, PixelFormat, PixelFormatRequirements, ReleaseBehavior,
    Robustness,
};
use crate::Error;

#[derive(Clone)]
pub(crate) struct Context {
    pub(crate) context: WeakPtr,
}

impl Default for Context {
    fn default() -> Self {
        Self {
            context: unsafe { WeakPtr::new(nil) },
        }
    }
}

impl Context {
    pub(crate) fn get_proc_address(&self, addr: &str) -> *const core::ffi::c_void {
        let symbol_name: CFString = FromStr::from_str(addr).unwrap();
        let framework_name: CFString = FromStr::from_str("com.apple.opengl").unwrap();
        let framework =
            unsafe { CFBundleGetBundleWithIdentifier(framework_name.as_concrete_TypeRef()) };
        let symbol = unsafe {
            CFBundleGetFunctionPointerForName(framework, symbol_name.as_concrete_TypeRef())
        };
        symbol as *const _
    }
}

pub(crate) fn create_gl_context(
    view: id,
    pf_reqs: &PixelFormatRequirements,
    gl_attr: &GlAttributes,
) -> Result<Context, Error> {
    match gl_attr.robustness {
        Robustness::RobustNoResetNotification | Robustness::RobustLoseContextOnReset => {
            return Err(anyhow!("You requested robustness, but it is not supported.").into());
        }
        _ => (),
    }

    let gl_profile = get_gl_profile(gl_attr, pf_reqs)?;
    let attributes = build_nsattributes(pf_reqs, gl_profile)?;

    unsafe {
        let pixel_format = NSOpenGLPixelFormat::alloc(nil).initWithAttributes_(&attributes);
        if pixel_format == nil {
            return Err(
                anyhow!("Couldn't find any pixel format that matches the criteria.").into(),
            );
        }

        let gl_context =
            NSOpenGLContext::alloc(nil).initWithFormat_shareContext_(pixel_format as id, nil);
        if gl_context == nil {
            return Err(anyhow!("could not open gl context").into());
        }

        let pixel_format = {
            let get_attr = |attrib: appkit::NSOpenGLPixelFormatAttribute| -> i32 {
                let mut value = 0;
                NSOpenGLPixelFormat::getValues_forAttribute_forVirtualScreen_(
                    pixel_format,
                    &mut value,
                    attrib,
                    NSOpenGLContext::currentVirtualScreen(gl_context),
                );
                value
            };

            PixelFormat {
                hardware_accelerated: get_attr(appkit::NSOpenGLPFAAccelerated) != 0,
                color_bits: (get_attr(appkit::NSOpenGLPFAColorSize)
                    - get_attr(appkit::NSOpenGLPFAAlphaSize)) as u8,
                alpha_bits: get_attr(appkit::NSOpenGLPFAAlphaSize) as u8,
                depth_bits: get_attr(appkit::NSOpenGLPFADepthSize) as u8,
                stencil_bits: get_attr(appkit::NSOpenGLPFAStencilSize) as u8,
                stereoscopy: get_attr(appkit::NSOpenGLPFAStereo) != 0,
                double_buffer: get_attr(appkit::NSOpenGLPFADoubleBuffer) != 0,
                multisampling: if get_attr(appkit::NSOpenGLPFAMultisample) > 0 {
                    Some(get_attr(appkit::NSOpenGLPFASamples) as u16)
                } else {
                    None
                },
                srgb: true,
            }
        };

        gl_context.setView_(view);
        let value = if gl_attr.vsync { 1 } else { 0 };
        gl_context.setValues_forParameter_(
            &value,
            appkit::NSOpenGLContextParameter::NSOpenGLCPSwapInterval,
        );

        CGLEnable(
            gl_context.CGLContextObj() as *mut _,
            kCGLCECrashOnRemovedFunctions,
        );

        let context = Context {
            context: WeakPtr::new(gl_context),
        };
        Ok(context)
    }
}

fn get_gl_profile(
    opengl: &GlAttributes,
    pf_reqs: &PixelFormatRequirements,
) -> Result<NSOpenGLPFAOpenGLProfiles, Error> {
    let version = opengl.version.to_gl_version();
    // first, compatibility profile support is strict
    if opengl.profile == Some(GlProfile::Compatibility) {
        // Note: we are not using ranges because of a rust bug that should be
        // fixed here: https://github.com/rust-lang/rust/pull/27050
        if version.unwrap_or((2, 1)) < (3, 2) {
            Ok(NSOpenGLProfileVersionLegacy)
        } else {
            Err(Error::Other(
                anyhow!("The requested OpenGL version is not supported.").into(),
            ))
        }
    } else if let Some(v) = version {
        // second, process exact requested version, if any
        if v < (3, 2) {
            if opengl.profile.is_none() && v <= (2, 1) {
                Ok(NSOpenGLProfileVersionLegacy)
            } else {
                Err(Error::Other(
                    anyhow!("The requested OpenGL version is not supported.").into(),
                ))
            }
        } else if v == (3, 2) {
            Ok(NSOpenGLProfileVersion3_2Core)
        } else {
            Ok(NSOpenGLProfileVersion4_1Core)
        }
    } else if let GlRequest::Latest = opengl.version {
        // now, find the latest supported version automatically;
        let mut attributes: [u32; 6] = [0; 6];
        let mut current_idx = 0;
        attributes[current_idx] = NSOpenGLPFAAllowOfflineRenderers as u32;
        current_idx += 1;

        if let Some(true) = pf_reqs.hardware_accelerated {
            attributes[current_idx] = NSOpenGLPFAAccelerated as u32;
            current_idx += 1;
        }

        if pf_reqs.double_buffer != Some(false) {
            attributes[current_idx] = NSOpenGLPFADoubleBuffer as u32;
            current_idx += 1
        }

        attributes[current_idx] = NSOpenGLPFAOpenGLProfile as u32;
        current_idx += 1;

        for &profile in &[NSOpenGLProfileVersion4_1Core, NSOpenGLProfileVersion3_2Core] {
            attributes[current_idx] = profile as u32;
            let id = unsafe { NSOpenGLPixelFormat::alloc(nil).initWithAttributes_(&attributes) };
            if id != nil {
                unsafe { msg_send![id, release] }
                return Ok(profile);
            }
        }
        // nothing else to do
        Ok(NSOpenGLProfileVersionLegacy)
    } else {
        Err(Error::Other(
            anyhow!("The requested OpenGL version is not supported.").into(),
        ))
    }
}

fn build_nsattributes(
    pf_reqs: &PixelFormatRequirements,
    profile: NSOpenGLPFAOpenGLProfiles,
) -> Result<Vec<u32>, Error> {
    // NOTE: OS X no longer has the concept of setting individual
    // color component's bit size. Instead we can only specify the
    // full color size and hope for the best. Another hiccup is that
    // `NSOpenGLPFAColorSize` also includes `NSOpenGLPFAAlphaSize`,
    // so we have to account for that as well.
    let alpha_depth = pf_reqs.alpha_bits.unwrap_or(8);
    let color_depth = pf_reqs.color_bits.unwrap_or(24) + alpha_depth;

    let mut attributes = vec![
        NSOpenGLPFAOpenGLProfile as u32,
        profile as u32,
        NSOpenGLPFAClosestPolicy as u32,
        NSOpenGLPFAColorSize as u32,
        color_depth as u32,
        NSOpenGLPFAAlphaSize as u32,
        alpha_depth as u32,
        NSOpenGLPFADepthSize as u32,
        pf_reqs.depth_bits.unwrap_or(24) as u32,
        NSOpenGLPFAStencilSize as u32,
        pf_reqs.stencil_bits.unwrap_or(8) as u32,
        NSOpenGLPFAAllowOfflineRenderers as u32,
    ];

    if let Some(true) = pf_reqs.hardware_accelerated {
        attributes.push(NSOpenGLPFAAccelerated as u32);
    }

    // Note: according to Apple docs, not specifying `NSOpenGLPFADoubleBuffer`
    // equals to requesting a single front buffer, in which case most of the GL
    // renderers will show nothing, since they draw to GL_BACK.
    if pf_reqs.double_buffer != Some(false) {
        attributes.push(NSOpenGLPFADoubleBuffer as u32);
    }

    if pf_reqs.release_behavior != ReleaseBehavior::Flush {
        return Err(Error::from(anyhow::anyhow!(
            "Couldn't find any pixel format that matches the criteria."
        )));
    }

    if pf_reqs.stereoscopy {
        unimplemented!(); // TODO:
    }

    if pf_reqs.float_color_buffer {
        attributes.push(NSOpenGLPFAColorFloat as u32);
    }

    if let Some(samples) = pf_reqs.multisampling {
        attributes.push(NSOpenGLPFAMultisample as u32);
        attributes.push(NSOpenGLPFASampleBuffers as u32);
        attributes.push(1);
        attributes.push(NSOpenGLPFASamples as u32);
        attributes.push(samples as u32);
    }

    // attribute list must be null terminated.
    attributes.push(0);

    Ok(attributes)
}
