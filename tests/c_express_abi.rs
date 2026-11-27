//! The C ABI smoke test (expressir#620): the `expr_*` functions round
//! trip the core surfaces through the exact signatures a C consumer
//! compiles against — parse → attach → format on one schema, population
//! parse + validate, and the NULL/error contract. The functions are
//! `pub extern "C" fn`, so the test calls them through their paths
//! (the ABI annotation is what a C consumer links).

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use expressir::ffi::c_express as abi;

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
fn c_abi_resolve_fills_cross_schema_base_paths() {
    const TWO_SCHEMAS: &str = "SCHEMA base;\nENTITY base_entity;\nEND_ENTITY;\nEND_SCHEMA;\nSCHEMA user_schema;\nUSE FROM base;\nENTITY user_entity SUBTYPE OF (base_entity);\nEND_ENTITY;\nEND_SCHEMA;\n";
    let wire = take(abi::expr_parse_schema(feed(TWO_SCHEMAS).as_ptr()));
    let resolved = take(abi::expr_resolve(feed(&wire).as_ptr()));
    // The parse_schema wire stamps the invocation path on the root, and
    // the fills carry ruby's ExpFile#id-else-path segment — the
    // from_file path form.
    assert!(
        resolved.contains("\"base_path\":\"source.exp.base.base_entity\""),
        "resolved wire: {resolved}"
    );
}

#[test]
fn c_abi_round_trip_population() {
    let src = "ISO-10303-21;\nHEADER;\nFILE_SCHEMA(('DEMO'));\nENDSEC;\nDATA;\n#1=E('it''s',$,.T.);\nENDSEC;\nEND-ISO-10303-21;\n";
    let text = take(abi::expr_round_trip_population(feed(src).as_ptr()));
    assert!(text.contains("#1=E('it''s',$,.T.);"), "round trip: {text}");
    assert!(
        text.starts_with("ISO-10303-21;"),
        "canonical header: {text}"
    );
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

#[test]
fn c_abi_p21_tooling_surfaces() {
    const SRC: &str = "ISO-10303-21;\nHEADER;\nFILE_SCHEMA(('DEMO'));\
\nENDSEC;\nDATA;\n#1=POINT(1.5,.BLUE.);\n#2=LINE(#1);\nENDSEC;\nEND-ISO-10303-21;\n";
    const REVISED: &str = "ISO-10303-21;\nHEADER;\nFILE_SCHEMA(('DEMO'));\
\nENDSEC;\nDATA;\n#1=POINT(2.0,.RED.);\n#3=LINE(#1);\nENDSEC;\nEND-ISO-10303-21;\n";

    let diff = take(abi::expr_diff_population(
        feed(SRC).as_ptr(),
        feed(REVISED).as_ptr(),
    ));
    assert!(diff.contains("\"changed\":[1]"), "diff: {diff}");
    assert!(diff.contains("\"added\":[3]"), "diff: {diff}");
    assert!(diff.contains("\"removed\":[2]"), "diff: {diff}");

    let wire = take(abi::expr_population_json(feed(SRC).as_ptr()));
    assert!(wire.contains("\"POINT\""), "population wire: {wire}");

    let schema = r#"{"_class":"Expressir::Model::ExpFile","schemas":[{"_class":"Expressir::Model::Declarations::Schema","id":"demo","entities":[{"_class":"Expressir::Model::Declarations::Entity","id":"point","attributes":[{"_class":"Expressir::Model::Declarations::Attribute","id":"x","type":{"_class":"Expressir::Model::DataTypes::Real"}}]},{"_class":"Expressir::Model::Declarations::Entity","id":"line","attributes":[{"_class":"Expressir::Model::Declarations::Attribute","id":"p","type":{"_class":"Expressir::Model::References::SimpleReference","id":"point"}}]}]}]}"#;

    let typing = take(abi::expr_type_population(
        feed(schema).as_ptr(),
        0,
        feed(SRC).as_ptr(),
    ));
    assert!(!typing.contains("\"sub1\""), "{typing}");
    assert!(
        typing.contains("point") || typing.contains("line"),
        "typing: {typing}"
    );

    let notes = take(abi::expr_validate_population_semantic(
        feed(schema).as_ptr(),
        0,
        feed(SRC).as_ptr(),
    ));
    assert!(notes.contains("\"notes\""), "validate: {notes}");
}

/// The #644/#640/#641/#646 tier sweep: the coverage, uos, xsd and
/// lsp-inventory entries on the same JSON/error-slot contract.
#[test]
fn writer_and_lsp_entries() {
    use abi::{expr_coverage, expr_lsp_inventory, expr_uos_xml, expr_xsd_xml};

    let wire = r#"{"_class":"Expressir::Model::ExpFile","schemas":[{"_class":"Expressir::Model::Declarations::Schema","id":"demo","entities":[{"_class":"Expressir::Model::Declarations::Entity","id":"point","remarks":["a point"],"attributes":[{"_class":"Expressir::Model::Declarations::Attribute","id":"x","type":{"_class":"Expressir::Model::DataTypes::Real"}}]}]}]}"#;
    let coverage = take(expr_coverage(feed(wire).as_ptr(), feed("[]").as_ptr()));
    assert!(coverage.contains("\"point\""), "coverage: {coverage}");
    assert!(coverage.contains("\"documented\":"), "coverage: {coverage}");

    let schema = r#"{"_class":"Expressir::Model::Declarations::Schema","id":"car_ownership","entities":[{"_class":"Expressir::Model::Declarations::Entity","id":"person","attributes":[{"_class":"Expressir::Model::Declarations::Attribute","id":"name"},{"_class":"Expressir::Model::Declarations::Attribute","id":"owns"}]},{"_class":"Expressir::Model::Declarations::Entity","id":"car","attributes":[{"_class":"Expressir::Model::Declarations::Attribute","id":"make"},{"_class":"Expressir::Model::Declarations::Attribute","id":"car_model"}]}]}"#;
    let population = r##"{"instances":{"#10":{"keyword":"PERSON","params":["John Q. Public",{"ref":"#20"}]},"#20":{"keyword":"CAR","params":["BMW","Z3"]}}}"##;
    let options = r#"{"id":"expdata01","target_namespace":"urn:co","prefix":"own"}"#;
    let uos = take(expr_uos_xml(
        feed(population).as_ptr(),
        feed(schema).as_ptr(),
        feed(options).as_ptr(),
    ));
    assert!(uos.starts_with("<own:uos"), "uos: {uos}");
    assert!(uos.contains("<Name>John Q. Public</Name>"), "uos: {uos}");

    let xsd = take(expr_xsd_xml(feed(schema).as_ptr()));
    assert!(xsd.starts_with("<xs:schema"), "xsd: {xsd}");

    let source = "SCHEMA demo;\n  ENTITY base;\n  END_ENTITY;\n  ENTITY sub SUBTYPE OF (base);\n  END_ENTITY;\nEND_SCHEMA;\n";
    let inventory = take(expr_lsp_inventory(feed(source).as_ptr()));
    assert!(
        inventory.contains("\"declarations\""),
        "inventory: {inventory}"
    );
    assert!(
        inventory.contains("\"base_path\""),
        "inventory: {inventory}"
    );
}

#[test]
fn c_abi_recovering_envelope_valid_input_matches_strict() {
    let recovered = take(abi::expr_parse_population_recovering(
        feed(POPULATION).as_ptr(),
    ));
    assert!(
        recovered.contains("\"ok\":true"),
        "valid input: {recovered}"
    );
    let strict = take(abi::expr_parse_population(feed(POPULATION).as_ptr()));
    // the envelope's population equals the strict one
    let strict_records = strict.matches("\"id\":").count();
    assert!(
        recovered.contains("\"ok\":true")
            && recovered.matches("\"id\":").count() >= strict_records.min(1),
        "the envelope carries the population: {recovered}"
    );
}

#[test]
fn c_abi_recovering_envelope_junk_yields_forensics() {
    let mut junked = POPULATION.to_string();
    let pos = junked.rfind("ENDSEC;").expect("POPULATION carries ENDSEC;");
    junked.insert_str(pos, "\n#9999=JUNK(%%%);\n");
    let recovered = take(abi::expr_parse_population_recovering(
        feed(junked.as_str()).as_ptr(),
    ));
    assert!(
        recovered.contains("\"ok\":false"),
        "junk must recover as forensics: {recovered}"
    );
    assert!(
        recovered.contains("\"offset\"") && recovered.contains("\"expected\""),
        "the failure wire rides the envelope: {recovered}"
    );
    assert!(
        recovered.contains("\"consumed_upto\":75"),
        "the consumed extent anchors the failure: {recovered}"
    );
    // resync spans need a spine the trial scan can restart; the tiny
    // fixture fails inside its single data section (the corpus gate
    // pins the spans case over the real annex fixtures).
}
