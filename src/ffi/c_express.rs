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

/// Resolves a wire's references in place (#635): fills `base_path` on
/// every reference node exactly like the ruby
/// ResolveReferencesModelVisitor. Returns the resolved wire.
#[no_mangle]
pub extern "C" fn expr_resolve(wire_json: *const c_char) -> *mut c_char {
    let Some(wire_json) = input(wire_json) else {
        set_error("wire_json is NULL");
        return std::ptr::null_mut();
    };
    let mut wire: serde_json::Value = match serde_json::from_str(&wire_json) {
        Ok(value) => value,
        Err(err) => {
            set_error(&format!("wire json: {err}"));
            return std::ptr::null_mut();
        }
    };
    crate::checker::resolve_references(&mut wire);
    out(wire.to_string())
}

/// Parses an ISO-10303-21 exchange structure and re-emits it (#642):
/// the parse → write round trip through the full value model. Returns
/// the canonical text, or NULL with the error slot set.
#[no_mangle]
pub extern "C" fn expr_round_trip_population(source: *const c_char) -> *mut c_char {
    let Some(source) = input(source) else {
        set_error("source is NULL");
        return std::ptr::null_mut();
    };
    match crate::population::round_trip_population(&source) {
        Ok(text) => out(text),
        Err(err) => {
            set_error(&err);
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

/// Scans an EXPRESS source for non-ASCII violations (#643); returns
/// the violation JSON array (run, line, column, char details and
/// replacement suggestions).
#[no_mangle]
pub extern "C" fn expr_scan_ascii(source: *const c_char, check_remarks: i32) -> *mut c_char {
    let Some(source) = input(source) else {
        set_error("source is NULL");
        return std::ptr::null_mut();
    };
    let violations = crate::validate_ascii::scan_source(&source, check_remarks != 0);
    match serde_json::to_string(&violations) {
        Ok(json) => out(json),
        Err(err) => {
            set_error(&format!("serialize: {err}"));
            std::ptr::null_mut()
        }
    }
}

/// Diffs two ISO-10303-21 exchange structures (#642): added, removed
/// and changed instance ids. Returns the diff JSON, or NULL with the
/// error slot set.
#[no_mangle]
pub extern "C" fn expr_diff_population(base: *const c_char, revised: *const c_char) -> *mut c_char {
    let (Some(base), Some(revised)) = (input(base), input(revised)) else {
        set_error("base or revised is NULL");
        return std::ptr::null_mut();
    };
    let base = crate::population::parse_full_population(&base);
    let revised = crate::population::parse_full_population(&revised);
    match (base, revised) {
        (Ok(base), Ok(revised)) => {
            let diff = crate::population::diff_populations(&base, &revised);
            match serde_json::to_string(&diff) {
                Ok(json) => out(json),
                Err(err) => {
                    set_error(&format!("serialize: {err}"));
                    std::ptr::null_mut()
                }
            }
        }
        (Err(err), _) | (_, Err(err)) => {
            set_error(&err);
            std::ptr::null_mut()
        }
    }
}

/// Serializes an ISO-10303-21 exchange structure to the tagged-value
/// JSON wire (#642).
#[no_mangle]
pub extern "C" fn expr_population_json(source: *const c_char) -> *mut c_char {
    let Some(source) = input(source) else {
        set_error("source is NULL");
        return std::ptr::null_mut();
    };
    match crate::population::parse_full_population(&source) {
        Ok(population) => {
            let wire = crate::population::population_to_json_value(&population);
            out(wire.to_string())
        }
        Err(err) => {
            set_error(&err);
            std::ptr::null_mut()
        }
    }
}

/// Types a population's instances against a schema wire (ISO
/// 10303-11 9.2.5, #642). `schema_index` selects the schema among the
/// wire's schemas. Returns the typing report JSON.
#[no_mangle]
pub extern "C" fn expr_type_population(
    schema_wire: *const c_char,
    schema_index: i32,
    source: *const c_char,
) -> *mut c_char {
    let (Some(schema_wire), Some(source)) = (input(schema_wire), input(source)) else {
        set_error("schema_wire or source is NULL");
        return std::ptr::null_mut();
    };
    let wire: serde_json::Value = match serde_json::from_str(&schema_wire) {
        Ok(value) => value,
        Err(err) => {
            set_error(&format!("schema wire: {err}"));
            return std::ptr::null_mut();
        }
    };
    match crate::population::parse_full_population(&source) {
        Ok(population) => {
            let report = crate::p21_typing::type_population(
                &wire,
                schema_index.max(0) as usize,
                &population,
            );
            match serde_json::to_string(&report) {
                Ok(json) => out(json),
                Err(err) => {
                    set_error(&format!("serialize: {err}"));
                    std::ptr::null_mut()
                }
            }
        }
        Err(err) => {
            set_error(&err);
            std::ptr::null_mut()
        }
    }
}

/// Validates a population against a schema wire (ISO 10303-21 clause
/// 12 semantics, #642). `schema_index` selects the schema. Returns
/// the notes JSON.
#[no_mangle]
pub extern "C" fn expr_validate_population_semantic(
    schema_wire: *const c_char,
    schema_index: i32,
    source: *const c_char,
) -> *mut c_char {
    let (Some(schema_wire), Some(source)) = (input(schema_wire), input(source)) else {
        set_error("schema_wire or source is NULL");
        return std::ptr::null_mut();
    };
    let wire: serde_json::Value = match serde_json::from_str(&schema_wire) {
        Ok(value) => value,
        Err(err) => {
            set_error(&format!("schema wire: {err}"));
            return std::ptr::null_mut();
        }
    };
    match crate::population::parse_full_population(&source) {
        Ok(population) => {
            let report =
                crate::p21_validator::validate(&wire, schema_index.max(0) as usize, &population);
            match serde_json::to_string(&report) {
                Ok(json) => out(json),
                Err(err) => {
                    set_error(&format!("serialize: {err}"));
                    std::ptr::null_mut()
                }
            }
        }
        Err(err) => {
            set_error(&err);
            std::ptr::null_mut()
        }
    }
}

/// The documentation-coverage engine (#644): wire JSON + a JSON array
/// of skip expressions in, the per-schema row report out.
#[no_mangle]
pub extern "C" fn expr_coverage(
    wire_json: *const c_char,
    skip_types_json: *const c_char,
) -> *mut c_char {
    let Some(wire_json) = input(wire_json) else {
        set_error("wire_json is NULL");
        return std::ptr::null_mut();
    };
    let Some(skip_types_json) = input(skip_types_json) else {
        set_error("skip_types_json is NULL");
        return std::ptr::null_mut();
    };
    match crate::coverage::coverage_model_json(&wire_json, &skip_types_json) {
        Ok(json) => out(json),
        Err(err) => {
            set_error(&err);
            std::ptr::null_mut()
        }
    }
}

/// The clause 9 uos binding writer (#640): population + schema wires
/// and the options object in, the uos document out.
#[no_mangle]
pub extern "C" fn expr_uos_xml(
    population_json: *const c_char,
    schema_json: *const c_char,
    options_json: *const c_char,
) -> *mut c_char {
    let Some(population_json) = input(population_json) else {
        set_error("population_json is NULL");
        return std::ptr::null_mut();
    };
    let Some(schema_json) = input(schema_json) else {
        set_error("schema_json is NULL");
        return std::ptr::null_mut();
    };
    let Some(options_json) = input(options_json) else {
        set_error("options_json is NULL");
        return std::ptr::null_mut();
    };
    match crate::p28::uos_xml_json(&population_json, &schema_json, &options_json) {
        Ok(xml) => out(xml),
        Err(err) => {
            set_error(&err);
            std::ptr::null_mut()
        }
    }
}

/// The EXPRESS-to-XML-Schema writer (#641): a schema wire in, the
/// xs:schema document out.
#[no_mangle]
pub extern "C" fn expr_xsd_xml(schema_json: *const c_char) -> *mut c_char {
    let Some(schema_json) = input(schema_json) else {
        set_error("schema_json is NULL");
        return std::ptr::null_mut();
    };
    match crate::xsd::xsd_xml_json(&schema_json) {
        Ok(xml) => out(xml),
        Err(err) => {
            set_error(&err);
            std::ptr::null_mut()
        }
    }
}

/// The LSP navigation inventory (#646): a document source in, the
/// declarations + resolved references rows out.
#[no_mangle]
pub extern "C" fn expr_lsp_inventory(source: *const c_char) -> *mut c_char {
    let Some(source) = input(source) else {
        set_error("source is NULL");
        return std::ptr::null_mut();
    };
    match crate::lsp::inventory_json(&source) {
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

/// The concatenated artifact for `root_id` over a resolved repository
/// wire (#636): `{ "ids": [...], "artifact": "..." }` — the sections
/// embed the files the wire names, readable at the caller.
#[no_mangle]
pub extern "C" fn expr_concat_schema(
    repository_wire: *const c_char,
    root_id: *const c_char,
) -> *mut c_char {
    let Some(repository_wire) = input(repository_wire) else {
        set_error("repository_wire is NULL");
        return std::ptr::null_mut();
    };
    let Some(root_id) = input(root_id) else {
        set_error("root_id is NULL");
        return std::ptr::null_mut();
    };
    match crate::wasm::concat_schema_json(repository_wire, root_id) {
        Ok(json) => out(json),
        Err(err) => {
            set_error(&err);
            std::ptr::null_mut()
        }
    }
}

/// The SHTOLO longform over a resolved repository wire (#636): the
/// full stage order. Returns the longform schema wire as JSON.
#[no_mangle]
pub extern "C" fn expr_flatten_schema(
    repository_wire: *const c_char,
    root_id: *const c_char,
    longform_name: *const c_char,
) -> *mut c_char {
    let Some(repository_wire) = input(repository_wire) else {
        set_error("repository_wire is NULL");
        return std::ptr::null_mut();
    };
    let Some(root_id) = input(root_id) else {
        set_error("root_id is NULL");
        return std::ptr::null_mut();
    };
    let Some(longform_name) = input(longform_name) else {
        set_error("longform_name is NULL");
        return std::ptr::null_mut();
    };
    match crate::wasm::flatten_schema_json(repository_wire, root_id, longform_name) {
        Ok(json) => out(json),
        Err(err) => {
            set_error(&err);
            std::ptr::null_mut()
        }
    }
}

/// The P21 recovery envelope over raw exchange-structure text (#627):
/// the population on well-formed input, the failure forensics on
/// malformed input. Returns the envelope as JSON.
#[no_mangle]
pub extern "C" fn expr_parse_population_recovering(source: *const c_char) -> *mut c_char {
    let Some(input) = input(source) else {
        set_error("input is NULL");
        return std::ptr::null_mut();
    };
    match crate::wasm::parse_population_recovering_json(input) {
        Ok(json) => out(json),
        Err(err) => {
            set_error(&err);
            std::ptr::null_mut()
        }
    }
}
