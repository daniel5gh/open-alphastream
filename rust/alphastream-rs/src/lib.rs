//! alphastream-rs — C ABI for .NET P/Invoke compatibility
use std::ffi::{c_char, c_int, c_uint, c_ulonglong, c_void, CStr, CString};
use std::ptr;

mod transport;
mod formats;
mod runtime;
mod scheduler;
mod rasterizer;
mod cache;
mod api;

#[repr(C)]
pub struct AlphaStreamHandle {
    width: u32,
    height: u32,
    total_frames: u32,
    last_error_code: i32,
    last_error_text: CString,
    last_frame: Vec<u8>, // R8 mask buffer
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

// Static C strings for name/version
static PLUGIN_NAME: &str = "alphastream-rs";
static PLUGIN_VERSION: &str = "0.1.0";

fn static_cstr(s: &str) -> *const c_char {
    // Leak a CString to keep pointer valid for process lifetime
    Box::leak(CString::new(s).unwrap().into_boxed_c_str()).as_ptr()
}

#[no_mangle]
pub extern "C" fn CV_create() -> *mut AlphaStreamHandle {
    let handle = Box::new(AlphaStreamHandle::new());
    Box::into_raw(handle)
}

#[no_mangle]
pub extern "C" fn CV_destroy(handle: *mut AlphaStreamHandle) {
    if !handle.is_null() {
        unsafe { drop(Box::from_raw(handle)); }
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

    #[test]
    fn version_is_semver_like() { assert!(version().split('.').count() >= 3); }

    #[test]
    fn echo_roundtrip() { assert_eq!(echo("alpha"), "alpha"); }
}
