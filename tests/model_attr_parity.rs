//! The model attr-table completeness oracle (expressir#634, PR-1).
//!
//! expressir generates `assets/model_attr_table.json` from its model tree
//! (lutaml attribute declarations, ObjectSpace-collected, deterministically
//! ordered). The checker's wire view (`src/checker_attr_table.json`) must
//! be a subset: every wire class it lists exists in the generated table
//! with no rogue attributes, and the classes the checker view does not
//! carry are exactly the documented quirk set below. expressir CI
//! regenerates the table; the no-drift sync fails this repo's CI when the
//! asset lags the model.

use serde_json::Value;

/// The classes the checker's wire view deliberately does not carry, with
/// the reason each is absent.
const DOCUMENTED_QUIRKS: &[(&str, &str)] = &[
    (
        "Expressir::Model::ModelElement",
        "the abstract base: its attrs are flattened into every leaf entry \
         of the checker view",
    ),
    (
        "Expressir::Model::RemarkInfo",
        "emitted inline by the attach pass (attach.rs add_remark_tagged), \
         not a checker-walked node class",
    ),
];

fn generated_table() -> Value {
    serde_json::from_str(include_str!("../assets/model_attr_table.json"))
        .expect("assets/model_attr_table.json parses")
}

fn checker_table() -> Value {
    serde_json::from_str(include_str!("../src/checker_attr_table.json"))
        .expect("src/checker_attr_table.json parses")
}

#[test]
fn every_checker_class_exists_in_the_generated_model() {
    let generated = generated_table();
    let checker = checker_table();

    let mut missing = Vec::new();
    for class in checker.as_object().expect("checker object").keys() {
        if !generated.as_object().unwrap().contains_key(class) {
            missing.push(class.clone());
        }
    }
    assert!(
        missing.is_empty(),
        "checker view lists classes the ruby model does not define: {missing:?}"
    );
}

#[test]
fn checker_attrs_are_a_subset_of_the_model_attrs() {
    let generated = generated_table();
    let checker = checker_table();

    let mut rogue: Vec<String> = Vec::new();
    for (class, attrs) in checker.as_object().unwrap() {
        let model_attrs = generated
            .as_object()
            .unwrap()
            .get(class)
            .and_then(Value::as_object)
            .unwrap_or_else(|| panic!("{class} missing from the generated table"));
        for attr in attrs.as_array().unwrap() {
            let name = attr.as_str().unwrap();
            if !model_attrs.contains_key(name) {
                rogue.push(format!("{class}#{name}"));
            }
        }
    }
    assert!(
        rogue.is_empty(),
        "checker view lists attrs the ruby model does not declare: {rogue:?}"
    );
}

#[test]
fn unwired_model_classes_are_exactly_the_documented_quirks() {
    let generated = generated_table();
    let checker = checker_table();

    let unwired: Vec<String> = generated
        .as_object()
        .unwrap()
        .keys()
        .filter(|c| !checker.as_object().unwrap().contains_key(*c))
        .cloned()
        .collect();
    let mut unwired = unwired;
    unwired.sort();

    let mut documented: Vec<&str> = DOCUMENTED_QUIRKS.iter().map(|(c, _)| *c).collect();
    documented.sort_unstable();

    if unwired != documented {
        panic!(
            "the unwired-class set drifted from DOCUMENTED_QUIRKS.\n\
             unwired: {unwired:?}\n\
             documented: {documented:?}\n\
             Every wire class the checker view does not carry needs an \
             owner-signed reason there (expressir#634)."
        );
    }

    // The quirk reasons must name real classes.
    for (class, reason) in DOCUMENTED_QUIRKS {
        assert!(
            generated.as_object().unwrap().contains_key(*class),
            "DOCUMENTED_QUIRKS names {class}, which the model does not define"
        );
        assert!(!reason.is_empty(), "{class} needs a reason");
    }
}
