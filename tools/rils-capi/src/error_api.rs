use super::*;

#[unsafe(no_mangle)]
pub extern "C" fn rils_last_error_code() -> i32 {
    LAST_ERROR.with(|error| error.borrow().code)
}

fn error_slice(value: &str) -> RilsSlice {
    RilsSlice {
        data: value.as_ptr(),
        length: value.len(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rils_last_error_message() -> RilsSlice {
    LAST_ERROR.with(|error| error_slice(&error.borrow().message))
}

#[unsafe(no_mangle)]
pub extern "C" fn rils_last_error_source_name() -> RilsSlice {
    LAST_ERROR.with(|error| error_slice(&error.borrow().source_name))
}

#[unsafe(no_mangle)]
pub extern "C" fn rils_last_error_span_start() -> u64 {
    LAST_ERROR.with(|error| error.borrow().span.start as u64)
}

#[unsafe(no_mangle)]
pub extern "C" fn rils_last_error_span_end() -> u64 {
    LAST_ERROR.with(|error| error.borrow().span.end as u64)
}
