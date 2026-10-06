//! ScreenCaptureKit region capture (macOS 15.2+) and in-memory ImageIO PNG encoding.
//! Load the optional framework at runtime so older OS versions can still run
//! keyboard/window scripts. No shell, picker, permission prompt or file writes.
use block2::RcBlock;
use core_foundation::{
    base::{CFType, CFTypeRef, TCFType},
    data::{CFDataCreateMutable, CFDataGetBytePtr, CFDataGetLength, CFMutableDataRef},
    string::{CFString, CFStringRef},
};
use core_graphics::display::CGDisplay;
use objc2::{
    msg_send,
    runtime::{AnyClass, Bool},
    sel,
};
use objc2_foundation::{NSError, NSPoint, NSRect, NSSize};
use scriptaro_core::Bounds;
use scriptaro_platform::{BackendError, BackendResult, PendingScreenshot};
use std::{
    ffi::{c_char, c_void},
    ptr,
    sync::{Mutex, OnceLock},
};

unsafe extern "C" {
    fn dlopen(path: *const c_char, mode: i32) -> *mut c_void;
}
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGImageGetWidth(image: *const c_void) -> usize;
    fn CGImageGetHeight(image: *const c_void) -> usize;
}
#[link(name = "ImageIO", kind = "framework")]
unsafe extern "C" {
    fn CGImageDestinationCreateWithData(
        data: CFMutableDataRef,
        kind: CFStringRef,
        count: usize,
        options: CFTypeRef,
    ) -> CFTypeRef;
    fn CGImageDestinationAddImage(
        destination: CFTypeRef,
        image: *const c_void,
        properties: CFTypeRef,
    );
    fn CGImageDestinationFinalize(destination: CFTypeRef) -> bool;
}
fn native(message: impl Into<String>) -> BackendError {
    BackendError::Native(message.into())
}

fn manager() -> Option<&'static AnyClass> {
    static MANAGER: OnceLock<Option<&'static AnyClass>> = OnceLock::new();
    *MANAGER.get_or_init(|| {
        // SAFETY: constant system framework path and SDK RTLD_LAZY=1. Intentionally
        // keep the process-lifetime handle loaded while its ObjC classes are used.
        let library = unsafe {
            dlopen(
                c"/System/Library/Frameworks/ScreenCaptureKit.framework/ScreenCaptureKit".as_ptr(),
                1,
            )
        };
        if library.is_null() {
            return None;
        }
        let class = AnyClass::get(c"SCScreenshotManager")?;
        // SAFETY: NSObject class method, BOOL result, selector carries no arguments.
        let supported: Bool = unsafe {
            msg_send![class, respondsToSelector: sel!(captureImageInRect:completionHandler:)]
        };
        supported.as_bool().then_some(class)
    })
}
pub(crate) fn permitted() -> bool {
    // SAFETY: read-only no-argument process permission query, available since 10.15.
    unsafe { CGPreflightScreenCaptureAccess() }
}
pub(crate) fn preflight() -> BackendResult<()> {
    if manager().is_none() {
        return Err(native(
            "screenshot requires macOS 15.2 or later (ScreenCaptureKit region API)",
        ));
    }
    if !permitted() {
        return Err(BackendError::PermissionDenied("grant Screen Recording access to Scriptaro (or its launching terminal) in System Settings > Privacy & Security, then restart it".into()));
    }
    Ok(())
}

// Only call while ScreenCaptureKit keeps the borrowed non-null CGImage alive.
unsafe fn png(image: *const c_void) -> BackendResult<Vec<u8>> {
    // SAFETY: caller provides a live CGImage from the completion handler.
    let (width, height) = unsafe { (CGImageGetWidth(image), CGImageGetHeight(image)) };
    if width == 0 || height == 0 || width.checked_mul(height).is_none_or(|n| n > 64_000_000) {
        return Err(native(
            "screenshot exceeds the 64000000 pixel limit or has no pixels",
        ));
    }
    // SAFETY: default allocator and dynamically sized CFMutableData; owns +1 result.
    let data = unsafe { CFDataCreateMutable(ptr::null(), 0) };
    if data.is_null() {
        return Err(native("could not allocate screenshot data"));
    }
    // SAFETY: non-null Create result retained until after copying its bytes.
    let _data_owner = unsafe { CFType::wrap_under_create_rule(data.cast()) };
    let kind = CFString::new("public.png");
    // SAFETY: writable CFData, valid PNG UTI, one image, optional properties null.
    let raw = unsafe {
        CGImageDestinationCreateWithData(data, kind.as_concrete_TypeRef(), 1, ptr::null())
    };
    if raw.is_null() {
        return Err(native("could not create PNG encoder"));
    }
    // SAFETY: non-null owned Create result released after encoding.
    let destination = unsafe { CFType::wrap_under_create_rule(raw) };
    // SAFETY: retained destination, borrowed live image and optional properties.
    unsafe {
        CGImageDestinationAddImage(destination.as_CFTypeRef(), image, ptr::null());
    }
    // SAFETY: retained destination with the promised single image added.
    if !unsafe { CGImageDestinationFinalize(destination.as_CFTypeRef()) } {
        return Err(native("could not finalize screenshot PNG"));
    }
    // SAFETY: retained initialized CFData; no more mutation after Finalize.
    let length = unsafe { CFDataGetLength(data) };
    if length <= 0 || length as usize > 256 * 1024 * 1024 {
        return Err(native("PNG exceeds the 256 MiB limit or is empty"));
    }
    // SAFETY: CFData guarantees readable storage for length bytes while retained.
    Ok(unsafe { std::slice::from_raw_parts(CFDataGetBytePtr(data), length as usize) }.to_vec())
}

pub(crate) fn capture(region: Option<Bounds>) -> BackendResult<PendingScreenshot> {
    preflight()?;
    let bounds = region.unwrap_or_else(|| {
        let rect = CGDisplay::main().bounds();
        Bounds {
            x: rect.origin.x,
            y: rect.origin.y,
            width: rect.size.width,
            height: rect.size.height,
        }
    });
    if ![bounds.x, bounds.y, bounds.width, bounds.height]
        .iter()
        .all(|n| n.is_finite())
        || bounds.width < 1.0
        || bounds.height < 1.0
        || bounds.width * bounds.height > 64_000_000.0
    {
        return Err(native("invalid or oversized screenshot region"));
    }
    let rect = NSRect::new(
        NSPoint::new(bounds.x, bounds.y),
        NSSize::new(bounds.width, bounds.height),
    );
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let sender = Mutex::new(Some(sender));
    let completion = RcBlock::new(move |image: *const c_void, error: *mut NSError| {
        let Some(sender) = sender.lock().ok().and_then(|mut s| s.take()) else {
            return;
        };
        if sender.is_closed() {
            return;
        }
        // SAFETY: nullable NSError/CGImage are borrowed from the native callback.
        // Encode here, sending only owned Rust bytes; native objects never escape.
        let result = unsafe {
            if let Some(error) = error.as_ref() {
                Err(native(error.localizedDescription().to_string()))
            } else if image.is_null() {
                Err(native("screenshot returned no image or error"))
            } else {
                png(image)
            }
        };
        let _ = sender.send(result);
    });
    let class = manager().expect("preflight checked method availability");
    // SAFETY: runtime availability checked; SDK signature is CGRect plus nullable
    // void (^)(CGImageRef, NSError*). AppKit copies the block for asynchronous use.
    unsafe {
        let _: () = msg_send![class, captureImageInRect: rect, completionHandler: &*completion];
    }
    Ok(Box::pin(async move {
        receiver
            .await
            .map_err(|_| native("screenshot callback was dropped"))?
    }))
}

#[cfg(test)]
mod tests {
    #[test]
    fn imageio_encodes_a_synthetic_image_without_screen_access() {
        use core_graphics::{
            base::kCGImageAlphaPremultipliedLast, color_space::CGColorSpace, context::CGContext,
        };
        use foreign_types::ForeignType;
        let context = CGContext::create_bitmap_context(
            None,
            16,
            8,
            8,
            0,
            &CGColorSpace::create_device_rgb(),
            kCGImageAlphaPremultipliedLast,
        );
        let image = context.create_image().unwrap();
        // SAFETY: synthetic owned CGImage stays alive for the entire encoder call.
        let bytes = unsafe { super::png(image.as_ptr().cast()) }.unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&bytes[12..16], b"IHDR");
        assert_eq!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()), 16);
        assert_eq!(u32::from_be_bytes(bytes[20..24].try_into().unwrap()), 8);
    }
}
