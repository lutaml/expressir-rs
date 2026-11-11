//! The C ABI smoke test (expressir#620): the `expr_*` functions round
//! trip the core surfaces through the exact signatures a C consumer
//! compiles against — parse → attach → format on one schema, population
//! parse + validate, and the NULL/error contract. The functions are
//! `pub extern "C" fn`, so the test calls them through their paths
//! (the ABI annotation is what a C consumer links).

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use expressir_rs::ffi::c_express as abi;

fn take(ptr: *mut c_char) -> String {
    assert!(
        !ptr.is_null(),
        "expr_* returned NULL; last_error={}",
        unsafe { CStr::from_ptr(abi::expr_last_error()) }.to_string_lossy()
    );
    let s = unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned();
    unsafe { abi::expr_string_free(ptr) };
    s
}

fn feed(s: &str) -> CString {
    CString::new(s.replace('\0', "")).expect("interior NUL")
}

const SCHEMA: &str = "SCHEMA demo;\nENTITY e;\n  a : STRING;\n  (*\"demo.e\" the note *)\nEND_ENTITY;\nEND_SCHEMA;\n";
const POPULATION: &str = "ISO-10303-21;\nHEADER;\nFILE_NAME('x');\nENDSEC;\nDATA;\n#1=E('v');\nENDSEC;\nEND-ISO-10303-21;\n";

#[test]
fn c_abi_round_trips_the_core_surfaces() {
    let v = unsafe { CStr::from_ptr(abi::expr_version()) }
        .to_string_lossy()
        .into_owned();
    assert!(!v.is_empty());

    let wire = {
        let c = feed(SCHEMA);
        let ptr = abi::expr_parse_schema(c.as_ptr());
        if ptr.is_null() {
            panic!(
                "parse_schema failed: {}",
                unsafe { CStr::from_ptr(abi::expr_last_error()) }.to_string_lossy()
            );
        }
        take(ptr)
    };
    assert!(wire.contains("\"schemas\""), "wire json: {wire}");

    let attached = take(abi::expr_attach_remarks(
        feed(&wire).as_ptr(),
        feed(SCHEMA).as_ptr(),
        0,
    ));
    // The tagged remark lands in the attached wire's remark store;
    // whether (and how) it renders is the formatter-parity suite's
    // contract.
    assert!(attached.contains("the note"), "attached: {attached}");

    let text = take(abi::expr_format(feed(&attached).as_ptr(), 0));
    assert!(text.contains("SCHEMA demo;"), "format: {text}");
    let stripped = take(abi::expr_format(feed(&attached).as_ptr(), 1));
    assert!(stripped.contains("END_ENTITY;"), "stripped: {stripped}");

    let findings = take(abi::expr_check(feed(&wire).as_ptr(), std::ptr::null()));
    assert!(findings.starts_with('['), "check: {findings}");
}

#[test]
fn c_abi_population_surfaces() {
    let population = take(abi::expr_parse_population(feed(POPULATION).as_ptr()));
    assert!(population.contains("E"), "population: {population}");
    let entities = "[\"E\"]";
    let verdict = take(abi::expr_validate_population(
        feed(entities).as_ptr(),
        feed(POPULATION).as_ptr(),
    ));
    assert!(!verdict.is_empty(), "validate: {verdict}");
}

#[test]
fn c_abi_error_contract_returns_null_and_last_error() {
    let result = abi::expr_parse_schema(std::ptr::null());
    assert!(result.is_null(), "NULL input must return NULL");
    let err = unsafe { CStr::from_ptr(abi::expr_last_error()) }
        .to_string_lossy()
        .into_owned();
    assert!(err.contains("NULL"), "last error: {err}");

    let bad = abi::expr_parse_schema(feed("not express at all").as_ptr());
    assert!(bad.is_null());
    let err2 = unsafe { CStr::from_ptr(abi::expr_last_error()) }
        .to_string_lossy()
        .into_owned();
    assert!(!err2.is_empty(), "error slot must update");
}
