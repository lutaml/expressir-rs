//! Wire-shape quirk tests: every observable Ruby builder behavior the
//! emitter mirrors deliberately. These pin the contract the expressir
//! parity spec (`parser_core_parity_spec`) holds across both parse
//! paths.

use expressir_rs::ParsedTree;
use serde_json::Value;

fn wire(source: &str) -> Value {
    ParsedTree::parse(source)
        .expect("parse")
        .to_model_json("test.exp")
        .expect("model json")
}

fn schema(v: &Value) -> &Value {
    &v["schemas"][0]
}

#[test]
fn schema_offset_points_at_the_schema_keyword() {
    let prefix = "\n  "; // leading whitespace must be skipped
    let v = wire(&format!("{prefix}SCHEMA s; END_SCHEMA;"));
    assert_eq!(schema(&v)["source_offset"], prefix.len());
}

#[test]
fn where_rule_offset_resolves_to_the_trailing_delimiter() {
    // The Ruby dispatch attaches where rules with the raw wrapper
    // element; with the depth-capped find_slice that resolves to the
    // op_delim's ";", not the expression.
    let src = "SCHEMA s;\nENTITY e; WHERE TRUE; END_ENTITY;\nEND_SCHEMA;\n";
    let v = wire(src);
    let rule = &schema(&v)["entities"][0]["where_rules"][0];
    assert!(rule.get("id").is_none());
    let offset = rule["source_offset"].as_u64().expect("offset") as usize;
    assert_eq!(&src[offset..offset + 1], ";");
}

#[test]
fn schema_version_items_split_like_the_ruby_builder() {
    let src = "SCHEMA s '{iso standard 10303 part(59) version(8)}';\nEND_SCHEMA;\n";
    let v = wire(src);
    let version = &schema(&v)["version"];
    assert_eq!(version["value"], "{iso standard 10303 part(59) version(8)}");
    let items = version["items"].as_array().expect("items");
    fn fields(item: &Value) -> (Option<&str>, Option<&str>) {
        (item["name"].as_str(), item["value"].as_str())
    }
    assert_eq!(fields(&items[0]), (Some("iso"), None));
    assert_eq!(fields(&items[1]), (Some("standard"), None));
    assert_eq!(fields(&items[2]), (None, Some("10303")));
    assert_eq!(fields(&items[3]), (Some("part"), Some("59")));
    assert_eq!(fields(&items[4]), (Some("version"), Some("8")));
}

#[test]
fn single_element_aggregate_drops_repetition() {
    // [4:2] — a single occurrence merges into the holder and dispatches
    // on its first key: bare expression, no AggregateInitializerItem.
    let src = "SCHEMA s;\nFUNCTION f : INTEGER;\n  LOCAL x : INTEGER; END_LOCAL;\n  x := [4:2];\nEND_FUNCTION;\nEND_SCHEMA;\n";
    let v = wire(src);
    let stmt = &schema(&v)["functions"][0]["statements"][0];
    let item = &stmt["expression"]["items"][0];
    assert_eq!(item["_class"], "Expressir::Model::Literals::Integer");
    assert!(item.get("repetition").is_none());
}

#[test]
fn multi_element_aggregate_keeps_repetition() {
    // [1:2, 3:4] — multi-occurrence elements are rule-key wrapped and
    // route through build_element, keeping the repetition.
    let src = "SCHEMA s;\nFUNCTION f : INTEGER;\n  LOCAL x : INTEGER; END_LOCAL;\n  x := [1:2, 3:4];\nEND_FUNCTION;\nEND_SCHEMA;\n";
    let v = wire(src);
    let stmt = &schema(&v)["functions"][0]["statements"][0];
    for item in stmt["expression"]["items"].as_array().expect("items") {
        assert_eq!(
            item["_class"],
            "Expressir::Model::Expressions::AggregateInitializerItem"
        );
        assert!(item.get("repetition").is_some());
    }
}

#[test]
fn subtype_constraint_always_renders_abstract() {
    let src = "SCHEMA s;\nENTITY e; END_ENTITY;\nSUBTYPE_CONSTRAINT c FOR e; END_SUBTYPE_CONSTRAINT;\nEND_SCHEMA;\n";
    let v = wire(src);
    assert_eq!(schema(&v)["subtype_constraints"][0]["abstract"], false);
}

#[test]
fn multi_ref_subtype_of_collects_all_refs() {
    // SUBTYPE OF (a, b) arrives as fragments merged with the op_comma
    // token; every entityRef still lands in subtype_of (GH-341).
    let src = "SCHEMA s;\nENTITY a; END_ENTITY;\nENTITY b; END_ENTITY;\nENTITY c SUBTYPE OF (a, b); END_ENTITY;\nEND_SCHEMA;\n";
    let v = wire(src);
    let c = &schema(&v)["entities"][2];
    assert_eq!(c["id"], "c");
    assert_eq!(c["subtype_of"][0]["id"], "a");
    assert_eq!(c["subtype_of"][1]["id"], "b");
}

#[test]
fn base_path_is_never_emitted() {
    // The Ruby reference resolver owns base_path after hydration.
    let src =
        "SCHEMA s;\nENTITY a; END_ENTITY;\nENTITY b SUBTYPE OF (a); END_ENTITY;\nEND_SCHEMA;\n";
    let v = wire(src);
    let b = &schema(&v)["entities"][1];
    assert_eq!(b["subtype_of"][0]["id"], "a");
    assert!(b["subtype_of"][0].get("base_path").is_none());
}

#[test]
fn interfaces_always_carry_an_items_array() {
    let src = "SCHEMA s;\nREFERENCE FROM other;\nEND_SCHEMA;\n";
    let v = wire(src);
    let interface = &schema(&v)["interfaces"][0];
    assert_eq!(interface["kind"], "REFERENCE");
    assert_eq!(interface["schema"]["id"], "other");
    assert_eq!(interface["items"].as_array().map(Vec::len), Some(0));
}

#[test]
fn general_aggregations_carry_element_modifiers() {
    // Attribute types route through the general aggregation types;
    // OPTIONAL/UNIQUE element modifiers must survive (GH-338).
    let src = "SCHEMA s;\nENTITY thing; END_ENTITY;\nENTITY holder;\nitems : LIST [1:?] OF UNIQUE thing;\narr : ARRAY [1:3] OF OPTIONAL thing;\nEND_ENTITY;\nEND_SCHEMA;\n";
    let v = wire(src);
    let attrs = &schema(&v)["entities"][1]["attributes"];
    assert_eq!(attrs[0]["id"], "items");
    assert_eq!(attrs[0]["type"]["unique"], true);
    assert_eq!(attrs[1]["id"], "arr");
    assert_eq!(attrs[1]["type"]["optional"], true);
}

#[test]
fn unique_rules_drop_phantom_fragments() {
    // The separator repetition can yield a label-less, attribute-less
    // fragment between real rules; it must not surface (GH-340).
    let src = "SCHEMA s;\nENTITY thing;\na : STRING;\nb : STRING;\nUNIQUE\nUR1: a;\nUR2: b;\nEND_ENTITY;\nEND_SCHEMA;\n";
    let v = wire(src);
    let u = &schema(&v)["entities"][0]["unique_rules"];
    assert_eq!(u.as_array().map(Vec::len), Some(2));
    assert_eq!(u[0]["id"], "UR1");
    assert_eq!(u[1]["id"], "UR2");
}
