//! The stable C ABI (expressir#620): a handle-free, JSON-in/JSON-out
//! surface over the core operations — schema parse, remark attach,
//! format, check, and the Part 21 population — for FFI tiers that do
//! not link the crate as a Rust dependency (parsanol-rs's `ffi/c.rs`
//! pattern: thread-local error slot, `expr_string_free` for every
//! returned string).
//!
//! Error contract: every function returning a string returns NULL on
//! failure and `expr_last_error()` then holds the message until the
//! next call on the same thread.

use std::cell::RefCell;
use std::ffi::{c_char, CStr, CString};

use crate::model_json::ModelJsonError;

thread_local! {
    static LAST_ERROR: RefCell<CString> = RefCell::new(CString::default());
}

fn set_error(msg: &str) {
    let stored = CString::new(msg).unwrap_or_default();
    LAST_ERROR.with(|slot| *slot.borrow_mut() = stored);
}

fn last_error_ptr() -> *const c_char {
    LAST_ERROR.with(|slot| slot.borrow().as_ptr())
}

fn out(json: String) -> *mut c_char {
    CString::new(json).unwrap_or_default().into_raw()
}

fn input(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        None
    } else {
        Some(
            unsafe { CStr::from_ptr(ptr) }
                .to_string_lossy()
                .into_owned(),
        )
    }
}

/// The crate version.
#[no_mangle]
pub extern "C" fn expr_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr().cast()
}

/// The last error on this thread (stable until the next call).
#[no_mangle]
pub extern "C" fn expr_last_error() -> *const c_char {
    last_error_ptr()
}

/// Parses an EXPRESS schema into the model-hash wire JSON.
#[no_mangle]
pub extern "C" fn expr_parse_schema(source: *const c_char) -> *mut c_char {
    let Some(source) = input(source) else {
        set_error("source is NULL");
        return std::ptr::null_mut();
    };
    match crate::ParsedTree::parse(&source) {
        Ok(tree) => match tree.to_model_json("source.exp") {
            Ok(value) => out(value.to_string()),
            Err(ModelJsonError::MissingNode) => {
                set_error("model json: missing node");
                std::ptr::null_mut()
            }
        },
        Err(err) => {
            set_error(&format!("parse: {err:?}"));
            std::ptr::null_mut()
        }
    }
}

/// Attaches remarks to a wire (tagged + untagged, the core-lane
/// pipeline); `annotations` non-zero enables the EXPRESSdoc
/// distinction. Returns the attached wire JSON.
#[no_mangle]
pub extern "C" fn expr_attach_remarks(
    wire_json: *const c_char,
    source: *const c_char,
    annotations: i32,
) -> *mut c_char {
    let (Some(wire_json), Some(source)) = (input(wire_json), input(source)) else {
        set_error("wire_json or source is NULL");
        return std::ptr::null_mut();
    };
    let mut wire: serde_json::Value = match serde_json::from_str(&wire_json) {
        Ok(value) => value,
        Err(err) => {
            set_error(&format!("wire json: {err}"));
            return std::ptr::null_mut();
        }
    };
    crate::attach::attach_tagged_remarks(&mut wire, &source, annotations != 0);
    crate::attach::attach_untagged_remarks(&mut wire, &source, annotations != 0);
    out(wire.to_string())
}

/// Formats a wire; `no_remarks` non-zero strips remarks.
#[no_mangle]
pub extern "C" fn expr_format(wire_json: *const c_char, no_remarks: i32) -> *mut c_char {
    let Some(wire_json) = input(wire_json) else {
        set_error("wire_json is NULL");
        return std::ptr::null_mut();
    };
    let wire: serde_json::Value = match serde_json::from_str(&wire_json) {
        Ok(value) => value,
        Err(err) => {
            set_error(&format!("wire json: {err}"));
            return std::ptr::null_mut();
        }
    };
    out(crate::formatter::format(&wire, no_remarks != 0))
}

/// Runs the checker over a wire; `dialect` may be NULL. Returns the
/// findings as a JSON array.
#[no_mangle]
pub extern "C" fn expr_check(wire_json: *const c_char, dialect: *const c_char) -> *mut c_char {
    let Some(wire_json) = input(wire_json) else {
        set_error("wire_json is NULL");
        return std::ptr::null_mut();
    };
    let dialect = input(dialect);
    let mut wire: serde_json::Value = match serde_json::from_str(&wire_json) {
        Ok(value) => value,
        Err(err) => {
            set_error(&format!("wire json: {err}"));
            return std::ptr::null_mut();
        }
    };
    let notes = crate::checker::check(&mut wire, dialect.as_deref());
    let findings: Vec<serde_json::Value> = notes.iter().map(|note| note.to_json()).collect();
    match serde_json::to_string(&findings) {
        Ok(json) => out(json),
        Err(err) => {
            set_error(&format!("serialize: {err}"));
            std::ptr::null_mut()
        }
    }
}

/// Parses an ISO-10303-21 population into its structural inventory JSON.
#[no_mangle]
pub extern "C" fn expr_parse_population(source: *const c_char) -> *mut c_char {
    let Some(source) = input(source) else {
        set_error("source is NULL");
        return std::ptr::null_mut();
    };
    match crate::wasm::parse_population_json(source) {
        Ok(json) => out(json),
        Err(err) => {
            set_error(&err);
            std::ptr::null_mut()
        }
    }
}

/// Validates a population's records against a schema's entity
/// inventory; returns the verdict JSON.
#[no_mangle]
pub extern "C" fn expr_validate_population(
    entities_json: *const c_char,
    source: *const c_char,
) -> *mut c_char {
    let (Some(entities_json), Some(source)) = (input(entities_json), input(source)) else {
        set_error("entities_json or source is NULL");
        return std::ptr::null_mut();
    };
    match crate::wasm::validate_population_json(entities_json, source) {
        Ok(json) => out(json),
        Err(err) => {
            set_error(&err);
            std::ptr::null_mut()
        }
    }
}

/// Frees a string returned by any `expr_*` function.
///
/// # Safety
/// `ptr` must be a pointer handed out by this module and not freed
/// before; dereferencing it is the caller's contract, like any free.
#[no_mangle]
pub unsafe extern "C" fn expr_string_free(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(unsafe { CString::from_raw(ptr) });
    }
}
