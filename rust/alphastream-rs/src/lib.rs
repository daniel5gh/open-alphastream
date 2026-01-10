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
pub mod testlib;

/// Handle structure for C API
/// This struct is passed between C functions to maintain state.
/// It's opaque to C code - C only sees a pointer to it.
/// Contains dimensions, error info, and the last processed frame buffer.
/// C ABI handle struct encapsulating an optional processor
#[repr(C)]
pub struct AlphaStreamCHandle {
    pub processor: Option<Box<api::AlphaStreamProcessor>>,
    pub last_frame_ptr: *mut u8,
    pub last_vertices_ptr: *mut f32,
    pub last_vertices_len: usize,
    // Future: error state, diagnostics, etc.
}

impl AlphaStreamCHandle {
    pub fn new() -> Self {
        Self {
            processor: None,
            last_frame_ptr: std::ptr::null_mut(),
            last_vertices_ptr: std::ptr::null_mut(),
            last_vertices_len: 0,
        }
    }
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

/// Create a new AlphaStream processor handle (FFI)
/// Call this first to get a processor for all operations.
/// Returns a pointer to AlphaStreamProcessor, or null if allocation fails.
/// In C#: IntPtr handle = CV_create();
#[no_mangle]
pub extern "C" fn CV_create() -> *mut AlphaStreamCHandle {
    Box::into_raw(Box::new(AlphaStreamCHandle::new()))
}

/// Destroy an AlphaStream processor and free its memory
/// Always call this when done to prevent memory leaks.
/// In C#: CV_destroy(handle);
#[no_mangle]
pub extern "C" fn CV_destroy(handle: *mut AlphaStreamCHandle) {
    if !handle.is_null() {
        unsafe { drop(Box::from_raw(handle)); }
    }
}

#[no_mangle]
pub extern "C" fn CV_get_name(_handle: *mut AlphaStreamCHandle) -> *const c_char {
    static_cstr(PLUGIN_NAME)
}

#[no_mangle]
pub extern "C" fn CV_get_version(_handle: *mut AlphaStreamCHandle) -> *const c_char {
    static_cstr(PLUGIN_VERSION)
}

#[no_mangle]
pub extern "C" fn CV_get_last_error_code(handle: *mut AlphaStreamCHandle) -> c_int {
    if handle.is_null() { return -1; }
    0
}

#[no_mangle]
pub extern "C" fn CV_get_last_error_text(_handle: *mut AlphaStreamCHandle) -> *const c_char {
    // TODO: Implement error text retrieval from AlphaStreamProcessor
    static_cstr("OK")
}

#[no_mangle]
pub extern "C" fn CV_get_total_frames(handle: *mut AlphaStreamCHandle) -> c_uint {
    if handle.is_null() { return 0; }
    unsafe {
        let chandle = &mut *handle;
        if let Some(proc) = &chandle.processor {
            // Try to get metadata
            let rt = tokio::runtime::Runtime::new().unwrap();
            match rt.block_on(async { proc.metadata().await }) {
                Ok(meta) => meta.frame_count,
                Err(_) => 0,
            }
        } else {
            0
        }
    }
}

#[no_mangle]
pub extern "C" fn CV_get_frame_size(handle: *mut AlphaStreamCHandle) -> c_uint {
    if handle.is_null() { return 0; }
    unsafe {
        let chandle = &mut *handle;
        if let Some(proc) = &chandle.processor {
            (proc.width() * proc.height()) as c_uint
        } else {
            0
        }
    }
}

/// Initialize the AlphaStream processor with connection parameters
/// This sets up the processor for AlphaStream file or server access.
/// Parameters:
/// - handle: The processor from CV_create()
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
    handle: *mut AlphaStreamCHandle,
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
    // Construct a processor from ASVP file for test/demo
    unsafe {
        let chandle = &mut *handle;
        // For test/demo, use the base_url as a file path
        if let Ok(path) = CStr::from_ptr(base_url).to_str() {
            match api::AlphaStreamProcessor::new_asvp(path, width, height, ProcessingMode::Both) {
                Ok(proc) => {
                    chandle.processor = Some(Box::new(proc));
                    return true;
                }
                Err(_) => return false,
            }
        }
    }
    false
}

/// Get a processed frame as R8 grayscale mask
/// Requests the specified frame and returns a pointer to the pixel data.
/// The data is width*height bytes of grayscale values (0-255).
/// Returns null if frame is not available or error occurred.
/// Check CV_get_last_error_code() for error details.
/// In C#: IntPtr frameData = CV_get_frame(handle, frameIndex);
/// Then copy the data: Marshal.Copy(frameData, buffer, 0, width * height);
#[no_mangle]
pub extern "C" fn CV_get_frame(handle: *mut AlphaStreamCHandle, frame_index: c_ulonglong) -> *const c_void {
    if handle.is_null() { return ptr::null(); }
    unsafe {
        let chandle = &mut *handle;
        if let Some(proc) = &chandle.processor {
            // Use processor.get_frame to retrieve bitmap
            let rt = tokio::runtime::Runtime::new().unwrap();
            return match rt.block_on(async { proc.get_frame(frame_index as usize, proc.width(), proc.height()).await }) {
                Some(bitmap) => {
                    // Free previous buffer
                    if !chandle.last_frame_ptr.is_null() {
                        let _ = Box::from_raw(chandle.last_frame_ptr);
                        chandle.last_frame_ptr = std::ptr::null_mut();
                    }
                    let boxed = bitmap.into_boxed_slice();
                    let ptr = Box::into_raw(boxed) as *mut u8;
                    chandle.last_frame_ptr = ptr;
                    ptr as *const c_void
                }
                None => ptr::null(),
            }
        }
        ptr::null()
    }
}

/// Get triangle strip vertices for 3D rendering
/// Returns vertex data for rendering the frame as 3D geometry.
/// Parameters:
/// - out_vertices: Pointer to receive array of float coordinates (x,y,z,x,y,z,...)
/// - out_count: Pointer to receive number of floats in the array
/// Returns true on success, false on error.
/// The vertex array contains 3D positions for triangle strip rendering.
/// In C#: float* vertices; IntPtr count; bool success = CV_get_triangle_strip_vertices(handle, frame, &vertices, &count);
#[no_mangle]
pub extern "C" fn CV_get_triangle_strip_vertices(handle: *mut AlphaStreamCHandle, frame_index: c_ulonglong, out_vertices: *mut *const f32, out_count: *mut usize) -> bool {
    if handle.is_null() || out_vertices.is_null() || out_count.is_null() {
        return false;
    }
    unsafe {
        let chandle = &mut *handle;
        if let Some(proc) = &chandle.processor {
            // Use processor.get_frame to retrieve bitmap
            let rt = tokio::runtime::Runtime::new().unwrap();
            return match rt.block_on(async { proc.get_triangle_strip_vertices(frame_index as usize).await }) {
                Some(vertices) => {
                    // Free previous buffer
                    if !chandle.last_vertices_ptr.is_null() {
                        let _ = Vec::from_raw_parts(chandle.last_vertices_ptr, chandle.last_vertices_len, chandle.last_vertices_len);
                        chandle.last_vertices_ptr = std::ptr::null_mut();
                        chandle.last_vertices_len = 0;
                    }
                    let boxed = vertices.into_boxed_slice();
                    let len = boxed.len();
                    let ptr = Box::into_raw(boxed);
                    *out_vertices = ptr as *const f32;
                    *out_count = len;
                    chandle.last_vertices_ptr = ptr as *mut f32;
                    chandle.last_vertices_len = len;
                    true
                }
                None => {
                    *out_vertices = ptr::null();
                    *out_count = 0;
                    false
                }
            }
        }
        false
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
    use crate::testlib::create_test_asvp;

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

        // Create a test ASVP file and use its path for base_url
        let test_file = create_test_asvp();
        let test_path = test_file.path().to_str().unwrap();
        let base_url = CString::new(test_path).unwrap();
        let version = CString::new("1.0.0").unwrap();

        let success = CV_init(
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

        assert!(success);
        assert_eq!(CV_get_last_error_code(handle), 0);
        assert_eq!(CV_get_total_frames(handle), 1);
        assert_eq!(CV_get_frame_size(handle), 256);

        CV_destroy(handle);
    }

    #[test]
    fn test_c_abi_get_frame() {
        let handle = CV_create();

        // Initialize with a real test file
        let test_file = create_test_asvp();
        let test_path = test_file.path().to_str().unwrap();
        let base_url = CString::new(test_path).unwrap();
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

        // Get frame 0, trigger processing
        let _ = CV_get_frame(handle, 0);
        // sleep for 500ms to allow frame to be processed
        std::thread::sleep(std::time::Duration::from_millis(500));
        let frame_ptr = CV_get_frame(handle, 0);
        assert!(!frame_ptr.is_null());
        assert_eq!(CV_get_last_error_code(handle), 0);

        // Check frame data
        unsafe {
            let frame_data = std::slice::from_raw_parts(frame_ptr as *const u8, 256);
            assert_eq!(frame_data.len(), 256);
            assert_eq!(frame_data[0], 0);
        }

        // Test out of range
        let null_frame = CV_get_frame(handle, 10001); // beyond total_frames
        assert!(null_frame.is_null());
        // TODO fix once we have error reporting
        // assert_eq!(CV_get_last_error_code(handle), 3); // NotFound error

        CV_destroy(handle);
    }

    #[test]
    fn test_c_abi_triangle_strip() {
        let handle = CV_create();

        // Initialize with a real test file
        let test_file = create_test_asvp();
        let test_path = test_file.path().to_str().unwrap();
        let base_url = CString::new(test_path).unwrap();
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

        // get frame 0, trigger processing
        let _ = CV_get_triangle_strip_vertices(handle, 0, &mut vertices, &mut count);
        // sleep for 500ms to allow frame to be processed
        std::thread::sleep(std::time::Duration::from_millis(500));
        let success = CV_get_triangle_strip_vertices(handle, 0, &mut vertices, &mut count);
        assert_eq!(success, true);
        assert_eq!(count, 0);
        assert!(!vertices.is_null());

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
