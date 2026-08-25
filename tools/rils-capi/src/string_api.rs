use super::*;

fn string_status_entry(function: impl FnOnce() -> i32) -> i32 {
    clear_error();
    match catch_unwind(AssertUnwindSafe(function)) {
        Ok(status) => status,
        Err(_) => fail(
            RILS_STATUS_PANIC,
            "Rust panic caught at the Rils string ABI boundary",
            "",
            Span::default(),
        ),
    }
}

#[unsafe(no_mangle)]
/// Creates a thread-bound UTF-8 string handle and transfers ownership to the caller.
///
/// # Safety
///
/// `utf8` must remain readable and `out_string` writable for this call.
pub unsafe extern "C" fn rils_string_create(utf8: RilsSlice, out_string: *mut Handle) -> i32 {
    string_status_entry(|| {
        if out_string.is_null() {
            return fail(
                RILS_STATUS_INVALID_ARGUMENT,
                "string output handle is null",
                "",
                Span::default(),
            );
        }
        let value = match unsafe { read_utf8(utf8, "string value") } {
            Ok(value) => value.to_owned(),
            Err(status) => return status,
        };
        let handle = match insert_string(value) {
            Ok(handle) => handle,
            Err(status) => return status,
        };
        unsafe { out_string.write(handle) };
        RILS_STATUS_OK
    })
}

#[unsafe(no_mangle)]
/// Returns the UTF-8 byte length of a string handle.
///
/// # Safety
///
/// `out_size` must be writable for this call.
pub unsafe extern "C" fn rils_string_size(string: Handle, out_size: *mut usize) -> i32 {
    string_status_entry(|| {
        if out_size.is_null() || !string_handle_is_current_thread(string) {
            return fail(
                if out_size.is_null() {
                    RILS_STATUS_INVALID_ARGUMENT
                } else {
                    RILS_STATUS_INVALID_HANDLE
                },
                "invalid string handle or output size pointer",
                "",
                Span::default(),
            );
        }
        STATE.with(|state| {
            let state = state.borrow();
            let Some(value) = state.strings.get(&string) else {
                return fail(
                    RILS_STATUS_INVALID_HANDLE,
                    "invalid, consumed, or destroyed string handle",
                    "",
                    Span::default(),
                );
            };
            unsafe { out_size.write(value.len()) };
            RILS_STATUS_OK
        })
    })
}

#[unsafe(no_mangle)]
/// Copies UTF-8 bytes from a string handle into caller-owned storage.
///
/// # Safety
///
/// A non-empty `buffer` must be writable for `buffer_capacity` bytes and
/// `out_written` must be writable for this call.
pub unsafe extern "C" fn rils_string_write(
    string: Handle,
    buffer: *mut u8,
    buffer_capacity: usize,
    out_written: *mut usize,
) -> i32 {
    string_status_entry(|| {
        if out_written.is_null() || !string_handle_is_current_thread(string) {
            return fail(
                if out_written.is_null() {
                    RILS_STATUS_INVALID_ARGUMENT
                } else {
                    RILS_STATUS_INVALID_HANDLE
                },
                "invalid string handle or output count pointer",
                "",
                Span::default(),
            );
        }
        STATE.with(|state| {
            let state = state.borrow();
            let Some(value) = state.strings.get(&string) else {
                return fail(
                    RILS_STATUS_INVALID_HANDLE,
                    "invalid, consumed, or destroyed string handle",
                    "",
                    Span::default(),
                );
            };
            if value.len() > buffer_capacity || (!value.is_empty() && buffer.is_null()) {
                return fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "string output buffer is too small or null",
                    "",
                    Span::default(),
                );
            }
            if !value.is_empty() {
                unsafe {
                    ptr::copy_nonoverlapping(value.as_ptr(), buffer, value.len());
                }
            }
            unsafe { out_written.write(value.len()) };
            RILS_STATUS_OK
        })
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn rils_string_destroy(string: Handle) -> i32 {
    string_status_entry(|| match take_string(string) {
        Ok(_) => RILS_STATUS_OK,
        Err(status) => status,
    })
}
