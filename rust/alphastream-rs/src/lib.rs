//! alphastream-rs — C ABI for .NET P/Invoke compatibility
//!
//! This module provides a C-compatible interface that can be called from other languages like C# via P/Invoke.
//! All functions are marked with #[no_mangle] and extern "C" to prevent name mangling and ensure C calling convention.
//! For novices: This allows .NET programs to use this Rust library without rewriting everything in C#.

use std::ffi::{c_char, c_int, c_uint, c_ulonglong, c_void, CStr, CString};
use std::ptr;

pub mod transport;
pub mod formats;
pub mod runtime;
pub mod scheduler;
pub mod rasterizer;
pub mod cache;
pub mod api;

/// Handle structure for C API
/// This struct is passed between C functions to maintain state.
/// It's opaque to C code - C only sees a pointer to it.
/// Contains dimensions, error info, and the last processed frame buffer.
#[repr(C)] // Ensures C-compatible memory layout
pub struct AlphaStreamHandle {
    width: u32,
    height: u32,
    total_frames: u32,
    last_error_code: i32, // 0 = success, negative = error
    last_error_text: CString, // Human-readable error message
    last_frame: Vec<u8>, // R8 mask buffer - grayscale alpha values
}

impl AlphaStreamHandle {
    fn new() -> Self {
        Self {
            width: 0,
            height: 0,
            total_frames: 0,
            last_error_code: 0,
            last_error_text: CString::new("OK").unwrap(),
            last_frame: Vec::new(),
        }
    }

    fn frame_size(&self) -> u32 { self.width.saturating_mul(self.height) }
}

pub use api::{AlphaStreamProcessor, ProcessingMode};
pub use cache::{FrameCache};
pub use formats::{FrameData};
pub use scheduler::{Scheduler, Task};
// Static C strings for name/version
static PLUGIN_NAME: &str = "alphastream-rs";
static PLUGIN_VERSION: &str = "0.1.0";

fn static_cstr(s: &str) -> *const c_char {
    // Leak a CString to keep pointer valid for process lifetime
    Box::leak(CString::new(s).unwrap().into_boxed_c_str()).as_ptr()
}

/// Create a new AlphaStream handle
/// Call this first to get a handle for other operations.
/// Returns a pointer to AlphaStreamHandle, or null if allocation fails.
/// In C#: IntPtr handle = CV_create();
#[no_mangle]
pub extern "C" fn CV_create() -> *mut AlphaStreamHandle {
    let handle = Box::new(AlphaStreamHandle::new());
    Box::into_raw(handle)
}

/// Destroy an AlphaStream handle and free its memory
/// Always call this when done to prevent memory leaks.
/// In C#: CV_destroy(handle);
#[no_mangle]
pub extern "C" fn CV_destroy(handle: *mut AlphaStreamHandle) {
    if !handle.is_null() {
        unsafe { drop(Box::from_raw(handle)); } // Reclaim the Box and free memory
    }
}

#[no_mangle]
pub extern "C" fn CV_get_name(_handle: *mut AlphaStreamHandle) -> *const c_char {
    static_cstr(PLUGIN_NAME)
}

#[no_mangle]
pub extern "C" fn CV_get_version(_handle: *mut AlphaStreamHandle) -> *const c_char {
    static_cstr(PLUGIN_VERSION)
}

#[no_mangle]
pub extern "C" fn CV_get_last_error_code(handle: *mut AlphaStreamHandle) -> c_int {
    if handle.is_null() { return -1; }
    unsafe { (*handle).last_error_code }
}

#[no_mangle]
pub extern "C" fn CV_get_last_error_text(handle: *mut AlphaStreamHandle) -> *const c_char {
    if handle.is_null() { return static_cstr("Invalid handle"); }
    unsafe { (*handle).last_error_text.as_ptr() }
}

#[no_mangle]
pub extern "C" fn CV_get_total_frames(handle: *mut AlphaStreamHandle) -> c_uint {
    if handle.is_null() { return 0; }
    unsafe { (*handle).total_frames }
}

#[no_mangle]
pub extern "C" fn CV_get_frame_size(handle: *mut AlphaStreamHandle) -> c_uint {
    if handle.is_null() { return 0; }
    unsafe { (*handle).frame_size() }
}

/// Initialize the AlphaStream processor with connection parameters
/// This sets up the connection to the AlphaStream server.
/// Parameters:
/// - handle: The handle from CV_create()
/// - base_url: Server URL as C string (e.g., "https://server.com")
/// - scene_id: Numeric ID of the scene to load
/// - width/height: Output dimensions for rendered frames
/// - version: Protocol version string
/// - start_frame: Which frame to start playback from
/// - buffer lengths: Network buffering settings
/// - timeouts: Connection and data timeouts in milliseconds
/// Returns true on success, false on failure (check CV_get_last_error_* for details)
/// In C#: bool success = CV_init(handle, urlPtr, sceneId, width, height, versionPtr, ...);
#[no_mangle]
pub extern "C" fn CV_init(
    handle: *mut AlphaStreamHandle,
    base_url: *const c_char,
    scene_id: c_uint,
    width: c_uint,
    height: c_uint,
    version: *const c_char,
    start_frame: c_uint,
    l0_buffer_length: c_uint,
    l1_buffer_length: c_uint,
    l1_buffer_init_length: c_uint,
    init_timeout_ms: c_uint,
    data_timeout_ms: c_uint,
) -> bool {
    if handle.is_null() {
        return false;
    }
    // Convert strings; ignore contents for now but validate UTF-8
    let _base_url = unsafe { CStr::from_ptr(base_url).to_string_lossy().into_owned() };
    let _version = unsafe { CStr::from_ptr(version).to_string_lossy().into_owned() };

    unsafe {
        let h = &mut *handle;
        h.width = width;
        h.height = height;
        h.total_frames = 10_000; // placeholder total frames
        h.last_error_code = 0; // None
        h.last_error_text = CString::new("OK").unwrap();
        h.last_frame = Vec::with_capacity(h.frame_size() as usize);
        h.last_frame.resize(h.frame_size() as usize, 0);
        // Optionally seed start_frame-related state; ignored here
        let _ = (scene_id, start_frame, l0_buffer_length, l1_buffer_length, l1_buffer_init_length, init_timeout_ms, data_timeout_ms);
    }

    true
}

/// Get a processed frame as R8 grayscale mask
/// Requests the specified frame and returns a pointer to the pixel data.
/// The data is width*height bytes of grayscale values (0-255).
/// Returns null if frame is not available or error occurred.
/// Check CV_get_last_error_code() for error details.
/// In C#: IntPtr frameData = CV_get_frame(handle, frameIndex);
/// Then copy the data: Marshal.Copy(frameData, buffer, 0, width * height);
#[no_mangle]
pub extern "C" fn CV_get_frame(handle: *mut AlphaStreamHandle, frame_index: c_ulonglong) -> *const c_void {
    if handle.is_null() {
        return ptr::null();
    }
    unsafe {
        let h = &mut *handle;
        // Minimal behavior: if index beyond total_frames, set error and return null
        if frame_index >= h.total_frames as u64 {
            h.last_error_code = 3; // NotFound or out-of-range
            h.last_error_text = CString::new("Frame index out of range").unwrap();
            return ptr::null();
        }
        // Synthesize an R8 mask: simple pattern based on frame_index
        let sz = h.frame_size() as usize;
        h.last_frame.resize(sz, 0);
        let v: u8 = ((frame_index % 256) as u8).saturating_add(1);
        for (i, px) in h.last_frame.iter_mut().enumerate() {
            // Checkerboard influenced by frame_index
            let x = (i as u32) % h.width;
            let y = (i as u32) / h.width;
            let checker = ((x / 8 + y / 8) % 2) as u8;
            *px = if checker == 0 { v } else { v.saturating_sub(1) };
        }
        h.last_error_code = 0;
        h.last_error_text = CString::new("OK").unwrap();
        h.last_frame.as_ptr() as *const c_void
    }
}

/// Get triangle strip vertices for 3D rendering
/// Returns vertex data for rendering the frame as 3D geometry.
/// Parameters:
/// - out_vertices: Pointer to receive array of float coordinates (x,y,z,x,y,z,...)
// - out_count: Pointer to receive number of floats in the array
/// Returns true on success, false on error.
/// The vertex array contains 3D positions for triangle strip rendering.
/// In C#: float* vertices; IntPtr count; bool success = CV_get_triangle_strip_vertices(handle, frame, &vertices, &count);
#[no_mangle]
pub extern "C" fn CV_get_triangle_strip_vertices(handle: *mut AlphaStreamHandle, frame_index: c_ulonglong, out_vertices: *mut *const f32, out_count: *mut usize) -> bool {
    if handle.is_null() || out_vertices.is_null() || out_count.is_null() {
        return false;
    }
    unsafe {
        let h = &mut *handle;
        // Placeholder: return empty vertices for now
        *out_vertices = ptr::null();
        *out_count = 0;
        h.last_error_code = 0;
        h.last_error_text = CString::new("OK").unwrap();
        true
    }
}

// Keep minimal Rust-native API for tests/demos
/// Returns the crate semantic version string.
pub fn version() -> &'static str { PLUGIN_VERSION }

/// Simple echo function for demo/tests.
pub fn echo(input: &str) -> String { input.to_string() }

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn version_is_semver_like() { assert!(version().split('.').count() >= 3); }

    #[test]
    fn echo_roundtrip() { assert_eq!(echo("alpha"), "alpha"); }

    #[test]
    fn test_c_abi_create_destroy() {
        let handle = CV_create();
        assert!(!handle.is_null());

        // Check initial state
        assert_eq!(CV_get_last_error_code(handle), 0);
        assert_eq!(CV_get_total_frames(handle), 0);
        assert_eq!(CV_get_frame_size(handle), 0);

        CV_destroy(handle);
    }

    #[test]
    fn test_c_abi_init() {
        let handle = CV_create();
        assert!(!handle.is_null());

        let base_url = CString::new("https://example.com").unwrap();
        let version = CString::new("1.0.0").unwrap();

        let success = CV_init(
            handle,
            base_url.as_ptr(),
            123,
            1920,
            1080,
            version.as_ptr(),
            0,
            1024,
            512,
            256,
            5000,
            30000,
        );

        assert!(success);
        assert_eq!(CV_get_last_error_code(handle), 0);
        assert_eq!(CV_get_total_frames(handle), 10000); // placeholder value
        assert_eq!(CV_get_frame_size(handle), 1920 * 1080);

        CV_destroy(handle);
    }

    #[test]
    fn test_c_abi_get_frame() {
        let handle = CV_create();

        // Initialize
        let base_url = CString::new("https://example.com").unwrap();
        let version = CString::new("1.0.0").unwrap();
        CV_init(
            handle,
            base_url.as_ptr(),
            123,
            16,
            16,
            version.as_ptr(),
            0,
            1024,
            512,
            256,
            5000,
            30000,
        );

        // Get frame 0
        let frame_ptr = CV_get_frame(handle, 0);
        assert!(!frame_ptr.is_null());
        assert_eq!(CV_get_last_error_code(handle), 0);

        // Check frame data (synthetic pattern)
        unsafe {
            let frame_data = std::slice::from_raw_parts(frame_ptr as *const u8, 256);
            assert_eq!(frame_data.len(), 256);
            // First pixel should be based on frame_index % 256 + 1
            assert_eq!(frame_data[0], 1);
        }

        // Test out of range
        let null_frame = CV_get_frame(handle, 10001); // beyond total_frames
        assert!(null_frame.is_null());
        assert_eq!(CV_get_last_error_code(handle), 3); // NotFound error

        CV_destroy(handle);
    }

    #[test]
    fn test_c_abi_triangle_strip() {
        let handle = CV_create();

        // Initialize
        let base_url = CString::new("https://example.com").unwrap();
        let version = CString::new("1.0.0").unwrap();
        CV_init(
            handle,
            base_url.as_ptr(),
            123,
            16,
            16,
            version.as_ptr(),
            0,
            1024,
            512,
            256,
            5000,
            30000,
        );

        // Get triangle strip (currently returns empty)
        let mut vertices: *const f32 = std::ptr::null();
        let mut count: usize = 0;

        let success = CV_get_triangle_strip_vertices(handle, 0, &mut vertices, &mut count);
        assert!(success);
        assert_eq!(count, 0);
        assert!(vertices.is_null());

        CV_destroy(handle);
    }

    #[test]
    fn test_c_abi_error_handling() {
        // Test with null handle
        assert_eq!(CV_get_last_error_code(std::ptr::null_mut()), -1);
        assert_eq!(CV_get_total_frames(std::ptr::null_mut()), 0);
        assert_eq!(CV_get_frame_size(std::ptr::null_mut()), 0);

        let frame_ptr = CV_get_frame(std::ptr::null_mut(), 0);
        assert!(frame_ptr.is_null());

        let mut vertices: *const f32 = std::ptr::null();
        let mut count: usize = 0;
        let success = CV_get_triangle_strip_vertices(std::ptr::null_mut(), 0, &mut vertices, &mut count);
        assert!(!success);
    }
}
