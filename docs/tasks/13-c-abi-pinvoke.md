# Task 13 — C ABI / P/Invoke

## Objective
- Stable C ABI compatible with .NET P/Invoke.

## C ABI Specification

### Functions
The following functions are exposed in the C ABI:

```c
void* CV_create();
void CV_destroy(void* handle);
bool CV_init(void* handle, const char* base_url, unsigned int scene_id, unsigned int width, unsigned int height, const char* version, unsigned int start_frame, unsigned int l0_buffer_length, unsigned int l1_buffer_length, unsigned int l1_buffer_init_length, unsigned int init_timeout_ms, unsigned int data_timeout_ms);
const void* CV_get_frame(void* handle, unsigned long long frame_index);
unsigned int CV_get_frame_size(void* handle);
unsigned int CV_get_total_frames(void* handle);
int CV_get_last_error_code(void* handle);
const char* CV_get_last_error_text(void* handle);
```

### Calling Convention
- All functions use the C calling convention (cdecl).
- Strings are ANSI (null-terminated char*).

### Buffer Ownership
- The frame buffer returned by `CV_get_frame` is owned by the library and remains valid until the next call to `CV_get_frame` or `CV_destroy`.
- Callers must copy the data if persistence beyond the next frame retrieval is required.

## Deliverables
- ffi module
- Export map

## Dependencies
- [docs/tasks/12-public-api-facade.md](docs/tasks/12-public-api-facade.md)

## Implementation Checklist
- Pointer lifetimes: Ensure handles are valid until destroyed and null checks are performed.
- Error codes: Map internal errors to appropriate integer codes and provide descriptive text.

## Acceptance Criteria
- .NET interop demo copies frames; error paths verified.

## References
- [rust/alphastream-rs/src/lib.rs](rust/alphastream-rs/src/lib.rs)
- [docs/RUST_IMPLEMENTATION.md](docs/RUST_IMPLEMENTATION.md)
