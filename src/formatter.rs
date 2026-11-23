//! The SourceFormatter over the wire (issue #22): renders an attached
//! model-hash wire back to EXPRESS text, byte-identical to Ruby's
//! `Expressir::Express::Formatter` (formatters/*.rb). Dispatch is a
//! match on the node's `_class`; remark emission mirrors
//! RemarkFormatter/RemarkItemFormatter (tagged remarks, the untagged
//! placement model — leading/trailing/inline with opener/else/
//! otherwise regions — and the block-end multiset subtraction).

use serde_json::Value;
use std::collections::HashMap;
use std::sync::OnceLock;

const INDENT: &str = "  ";

fn cls(node: &Value) -> &str {
    node.get("_class").and_then(Value::as_str).unwrap_or("")
}

fn tail(node: &Value) -> &str {
    let c = cls(node);
    c.rsplit("::").next().unwrap_or(c)
}

fn is(node: &Value, suffix: &str) -> bool {
    cls(node).ends_with(&format!("::{suffix}"))
}

/// An item of an ENUMERATION, or a reference: the items of a SELECT are
/// references, and the attacher places an inline remark on a reference
/// only there.
fn is_list_item(node: &Value) -> bool {
    is(node, "DataTypes::EnumerationItem") || is(node, "References::SimpleReference")
}

fn id_of(node: &Value) -> Option<&str> {
    node.get("id").and_then(Value::as_str)
}

fn empty_array() -> &'static Vec<Value> {
    static EMPTY: OnceLock<Vec<Value>> = OnceLock::new();
    EMPTY.get_or_init(Vec::new)
}

fn arr<'a>(node: &'a Value, name: &str) -> &'a Vec<Value> {
    match node.get(name) {
        Some(Value::Array(items)) => items,
        _ => empty_array(),
    }
}

fn obj<'a>(node: &'a Value, name: &str) -> Option<&'a Value> {
    node.get(name).filter(|v| v.is_object())
}

fn str_fld<'a>(node: &'a Value, name: &str) -> Option<&'a str> {
    node.get(name).and_then(Value::as_str)
}

fn truthy(node: &Value, name: &str) -> bool {
    matches!(node.get(name), Some(Value::Bool(true)))
}

fn value_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

fn attrs_of(class: &str) -> &'static Vec<String> {
    static TABLE: OnceLock<std::collections::HashMap<String, Vec<String>>> = OnceLock::new();
    static EMPTY: Vec<String> = Vec::new();
    let table = TABLE.get_or_init(|| {
        serde_json::from_str::<std::collections::HashMap<String, Vec<String>>>(include_str!(
            "checker_attr_table.json"
        ))
        .expect("checker_attr_table.json parses")
    });
    table.get(class).unwrap_or(&EMPTY)
}

fn index_parents(node: &Value, ptr: &str, parent: &mut HashMap<String, String>) {
    for attr in attrs_of(cls(node)) {
        if attr == "parent" {
            continue;
        }
        match node.get(attr.as_str()) {
            Some(Value::Array(items)) => {
                for (i, item) in items.iter().enumerate() {
                    if item.is_object() {
                        let child = format!("{ptr}/{attr}/{i}");
                        parent.insert(child.clone(), ptr.to_string());
                        index_parents(item, &child, parent);
                    }
                }
            }
            Some(v) if v.is_object() => {
                let child = format!("{ptr}/{attr}");
                parent.insert(child.clone(), ptr.to_string());
                index_parents(v, &child, parent);
            }
            _ => {}
        }
    }
}

struct PathCtx<'a> {
    root: &'a Value,
    parent: HashMap<String, String>,
}

impl<'a> PathCtx<'a> {
    fn new(root: &'a Value) -> Self {
        let mut parent = HashMap::new();
        let count = root
            .get("schemas")
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0);
        for i in 0..count {
            let ptr = format!("/schemas/{i}");
            if let Some(node) = root.pointer(&ptr) {
                index_parents(node, &ptr, &mut parent);
            }
        }
        Self { root, parent }
    }

    fn node(&self, ptr: &str) -> &'a Value {
        static NULL: OnceLock<Value> = OnceLock::new();
        self.root
            .pointer(ptr)
            .unwrap_or_else(|| NULL.get_or_init(|| Value::Null))
    }
}

/// Ruby `ModelElement#path` over the wire.
fn node_path(ctx: &PathCtx, ptr: &str) -> Option<String> {
    let node = ctx.node(ptr);
    if is(node, "Statements::Alias")
        || is(node, "Statements::Repeat")
        || is(node, "Expressions::QueryExpression")
    {
        return None;
    }
    let mut parts: Vec<String> = Vec::new();
    let mut current = ptr.to_string();
    loop {
        let node = ctx.node(&current);
        if !is(node, "References::SimpleReference") {
            if let Some(id) = id_of(node) {
                let is_ipr_shadow = is(node, "Declarations::RemarkItem")
                    && ctx
                        .parent
                        .get(&current)
                        .map(|p| {
                            let pnode = ctx.node(p);
                            is(pnode, "Declarations::InformalPropositionRule")
                                && id_of(pnode) == Some(id)
                        })
                        .unwrap_or(false);
                if !is_ipr_shadow {
                    parts.push(id.to_string());
                }
            }
        }
        match ctx.parent.get(&current) {
            Some(p) if !p.is_empty() => current = p.clone(),
            _ => break,
        }
    }
    if parts.is_empty() {
        None
    } else {
        parts.reverse();
        Some(parts.join("."))
    }
}

/// The reference-hyperlink flavors (expressir#637, slice 2): the
/// mixin faces of the ruby Formatter family. `Source` = plain ids;
/// `Hyperlink` = the stepmod `{{{<<express:...>>}}}` markup
/// (HyperlinkFormatter); `Adoc` = AsciiDoc xref macros
/// (AdocHyperlinkFormatter).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Flavor {
    Source,
    Hyperlink,
    Adoc,
}

/// `head_only` = the SchemaHeadFormatter face: a Schema renders as
/// its head alone.
pub fn format_flavored(root: &Value, path: &str, options_json: &str) -> Result<String, String> {
    let options: Value =
        serde_json::from_str(options_json).map_err(|e| format!("options json: {e}"))?;
    let flavor = match options.get("flavor").and_then(Value::as_str) {
        Some("hyperlink") => Flavor::Hyperlink,
        Some("adoc") => Flavor::Adoc,
        _ => Flavor::Source,
    };
    let no_remarks = options.get("no_remarks").and_then(Value::as_bool) == Some(true);
    let keep = options
        .get("keep_untagged_remarks")
        .and_then(Value::as_bool)
        == Some(true);
    let head_only = options.get("head_only").and_then(Value::as_bool) == Some(true);
    let ctx = PathCtx::new(root);
    let mut f = Formatter {
        ctx: &ctx,
        no_remarks,
        keep_untagged_remarks: keep,
        width: 2,
        flavor,
        head_only,
    };
    if path.is_empty() {
        let schemas = arr(ctx.root, "schemas");
        let body = schemas
            .iter()
            .enumerate()
            .map(|(i, _)| f.dispatch(&format!("/schemas/{i}")))
            .collect::<Vec<_>>()
            .join("\n\n");
        if body.is_empty() {
            return Ok(String::new());
        }
        return Ok(format!("{body}\n"));
    }
    Ok(f.dispatch(path))
}

/// Renders the wire (an ExpFile) to EXPRESS text.
pub fn format(root: &Value, no_remarks: bool) -> String {
    format_with_keep(root, no_remarks, false)
}

/// The `no_remarks` + `keep_untagged_remarks` pair (#703): the plain
/// face strips the annotation blocks but keeps the source's own
/// comments when the flag is set.
pub fn format_with_keep(root: &Value, no_remarks: bool, keep_untagged_remarks: bool) -> String {
    let ctx = PathCtx::new(root);
    let mut f = Formatter {
        ctx: &ctx,
        no_remarks,
        keep_untagged_remarks,
        width: 2,
        flavor: Flavor::Source,
        head_only: false,
    };
    let schemas = arr(ctx.root, "schemas");
    let body = schemas
        .iter()
        .enumerate()
        .map(|(i, _)| f.dispatch(&format!("/schemas/{i}")))
        .collect::<Vec<_>>()
        .join("\n\n");
    if body.is_empty() {
        String::new()
    } else {
        format!("{body}\n")
    }
}

struct Formatter<'a> {
    ctx: &'a PathCtx<'a>,
    no_remarks: bool,
    /// The plain face keeps the source's untagged comments when set
    /// (#703).
    keep_untagged_remarks: bool,
    /// The nesting width (the ruby self.indent() — PrettyFormatter
    /// overrides it with its configured width).
    width: usize,
    flavor: Flavor,
    head_only: bool,
}

impl<'a> Formatter<'a> {
    fn indent(&self, text: &str) -> String {
        let prefix = " ".repeat(self.width);
        text.split('\n')
            .map(|line| format!("{prefix}{line}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// A RemarkInfo from the wire.
struct Remark<'a> {
    text: Option<&'a str>,
    tag: Option<&'a str>,
    format: Option<&'a str>,
    placement: Option<&'a str>,
    region: Option<&'a str>,
}

impl<'a> Remark<'a> {
    fn from(node: &'a Value) -> Self {
        Self {
            text: str_fld(node, "text"),
            tag: str_fld(node, "tag"),
            format: str_fld(node, "format"),
            placement: str_fld(node, "placement"),
            region: str_fld(node, "region"),
        }
    }

    fn tagged(&self) -> bool {
        self.tag.is_some()
    }

    fn tail(&self) -> bool {
        self.format == Some("tail")
    }

    fn tail_annotation(&self) -> bool {
        self.format == Some("tail_annotation")
    }

    fn embedded_annotation(&self) -> bool {
        self.format == Some("embedded_annotation")
    }

    /// Tail placement semantics — annotations sit where tails sit.
    fn tail_like(&self) -> bool {
        self.tail() || self.tail_annotation()
    }

    fn inline(&self) -> bool {
        self.placement == Some("inline")
    }

    fn leading(&self) -> bool {
        self.placement == Some("leading")
    }

    fn opener(&self) -> bool {
        self.inline() && self.region == Some("opener")
    }

    fn inline_region(&self, region: &str) -> bool {
        self.inline() && self.region == Some(region)
    }

    fn trailing_region(&self, region: &str) -> bool {
        self.placement == Some("trailing") && self.region == Some(region)
    }
}

impl<'a> Formatter<'a> {
    fn node(&self, ptr: &str) -> &'a Value {
        self.ctx.node(ptr)
    }

    fn dispatch(&mut self, ptr: &str) -> String {
        let node = self.node(ptr).clone();
        let body = self.format_body(ptr, &node);
        let text = self.place_remarks(body, ptr, &node);
        // A source-parenthesized expression keeps its pairs (#707) —
        // all of them, when the groups were nested.
        let depth = node
            .get("parenthesized")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        if depth > 0 {
            let open = "(".repeat(depth as usize);
            let close = ")".repeat(depth as usize);
            format!("{open}{text}{close}")
        } else {
            text
        }
    }

    fn child(&mut self, base: &str, field: &str) -> String {
        self.dispatch(&format!("{base}/{field}"))
    }

    fn list_join(&mut self, base: &str, field: &str, sep: &str) -> String {
        let count = arr(self.node(base), field).len();
        (0..count)
            .map(|i| self.child(base, &format!("{field}/{i}")))
            .collect::<Vec<_>>()
            .join(sep)
    }

    /// expressir#655: render a list of Variable/Parameter/Attribute
    /// siblings with the grouping the source wrote — `a, b : INTEGER;` as
    /// one declaration. An adjacent same-`declaration_group` run renders
    /// through its lead with the joined sibling names, and only when no
    /// member carries remarks or (for attributes) a renamed supertype
    /// attribute; every other member renders per name through dispatch.
    fn list_join_grouped(&mut self, base: &str, field: &str, sep: &str, attribute: bool) -> String {
        let count = arr(self.node(base), field).len();
        let ptrs: Vec<String> = (0..count).map(|i| format!("{base}/{field}/{i}")).collect();
        self.join_grouped(&ptrs, sep, attribute)
    }

    fn join_grouped(&mut self, ptrs: &[String], sep: &str, attribute: bool) -> String {
        let group_of = |selfish: &Self, p: &str| -> Option<u64> {
            selfish
                .node(p)
                .get("declaration_group")
                .and_then(Value::as_u64)
        };
        let combinable = |selfish: &Self, p: &str| -> bool {
            let n = selfish.node(p);
            arr(n, "remark_items").is_empty()
                && !(attribute && obj(n, "supertype_attribute").is_some())
        };

        let mut out: Vec<String> = Vec::new();
        let mut i = 0;
        while i < ptrs.len() {
            let lead_group = group_of(self, &ptrs[i]);
            let run_end = lead_group.and_then(|g| {
                if i + 1 < ptrs.len() && group_of(self, &ptrs[i + 1]) == Some(g) {
                    let mut j = i + 1;
                    while j < ptrs.len() && group_of(self, &ptrs[j]) == Some(g) {
                        j += 1;
                    }
                    if (i..j).all(|k| combinable(self, &ptrs[k])) {
                        return Some(j);
                    }
                }
                None
            });

            if let (Some(j), Some(_)) = (run_end, lead_group) {
                let ids = (i..j)
                    .map(|k| id_of(self.node(&ptrs[k])).unwrap_or_default().to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                let lead = self.node(&ptrs[i]).clone();
                let t = cls(&lead).rsplit("::").next().unwrap_or("").to_string();
                let leading = self.format_leading_statement_remarks(&lead);
                let rendered = match t.as_str() {
                    "Attribute" | "DerivedAttribute" | "InverseAttribute" => {
                        self.format_attribute_ids(&ptrs[i], &lead, Some(&ids))
                    }
                    "Parameter" => self.format_parameter_ids(&ptrs[i], &lead, Some(&ids)),
                    "Variable" => self.format_variable_ids(&ptrs[i], &lead, Some(&ids)),
                    _ => self.dispatch(&ptrs[i]),
                };
                // The combined declaration does not pass through dispatch.
                out.push(
                    leading
                        .into_iter()
                        .chain(std::iter::once(rendered))
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
                i = j;
            } else {
                out.push(self.dispatch(&ptrs[i]));
                i += 1;
            }
        }
        out.join(sep)
    }

    fn format_body(&mut self, ptr: &str, node: &Value) -> String {
        let class = cls(node).to_string();
        let t = class.rsplit("::").next().unwrap_or("").to_string();
        match t.as_str() {
            "Schema" => self.format_schema(ptr, node),
            "Entity" => self.format_entity(ptr, node),
            "Type" => self.format_type(ptr, node),
            "Function" => self.format_function(ptr, node),
            "Procedure" => self.format_procedure(ptr, node),
            "Rule" => self.format_rule(ptr, node),
            "Interface" => self.format_interface(ptr, node),
            "InterfaceItem" => self.format_interface_item(ptr, node),
            "Constant" => self.format_constant(ptr, node),
            "Attribute" | "DerivedAttribute" | "InverseAttribute" => {
                self.format_attribute(ptr, node)
            }
            "Parameter" => self.format_parameter(ptr, node),
            "Variable" => self.format_variable(ptr, node),
            "UniqueRule" => self.format_unique_rule(ptr, node),
            "WhereRule" => self.format_where_rule(ptr, node),
            "InformalPropositionRule" => {
                let mut out = id_of(node).map(|id| format!("{id}: ")).unwrap_or_default();
                for remark in self.format_remark_items_children(ptr) {
                    out.push('\n');
                    out.push_str(&remark);
                }
                out
            }
            "SubtypeConstraint" => self.format_subtype_constraint(ptr, node),
            "SchemaVersion" => format!("'{}'", str_fld(node, "value").unwrap_or_default()),
            "RemarkItem" => self.format_remark_item(ptr, node),
            "EnumerationItem" => {
                let mut out = id_of(node).unwrap_or_default().to_string();
                for remark in self.format_remark_items_children(ptr) {
                    out.push('\n');
                    out.push_str(&remark);
                }
                out
            }
            _ if class.contains("DataTypes::") => self.format_data_type(ptr, node, &class),
            _ if class.contains("Literals::") => self.format_literal(node),
            _ if class.contains("References::") => self.format_reference(ptr, node, &t),
            // SupertypeExpressions CONTAINS "Expressions::" as a
            // substring — match it before the expression dispatcher.
            _ if class.contains("SupertypeExpressions::") => self.format_supertype(ptr, node, &t),
            _ if class.contains("Expressions::") => self.format_expression(ptr, node, &t),
            _ if class.contains("Statements::") => self.format_statement(ptr, node, &t),
            _ => String::new(),
        }
    }

    // ---- declarations ----

    fn format_schema(&mut self, ptr: &str, node: &Value) -> String {
        if self.head_only {
            return self.format_schema_head(ptr, node);
        }
        let preamble = self.format_preamble_remarks(ptr, &self.indent(INDENT));
        let mut decls: Vec<String> = Vec::new();
        if !arr(node, "constants").is_empty() {
            let inner = self.list_join(ptr, "constants", "\n");
            decls.push(format!("CONSTANT\n{}\nEND_CONSTANT;", self.indent(&inner)));
        }
        // The declarations in the order of the source (#711): the wire
        // records the declaration ids as parsed; schemas without an
        // order (old wires) keep the grouped-by-kind layout.
        let order = arr(node, "declaration_order");
        if order.is_empty() {
            for field in [
                "types",
                "entities",
                "subtype_constraints",
                "functions",
                "rules",
                "procedures",
            ] {
                let count = arr(node, field).len();
                for j in 0..count {
                    decls.push(self.child(ptr, &format!("{field}/{j}")));
                }
            }
        } else {
            let fields = [
                "types",
                "entities",
                "subtype_constraints",
                "functions",
                "rules",
                "procedures",
            ];
            for id in order {
                let Some(id) = id.as_str() else { continue };
                for field in fields {
                    let count = arr(node, field).len();
                    for j in 0..count {
                        let p = format!("{ptr}/{field}/{j}");
                        if id_of(self.node(&p)) == Some(id) {
                            decls.push(self.child(ptr, &format!("{field}/{j}")));
                        }
                    }
                }
            }
        }
        let mut out = vec![self.format_schema_head(ptr, node)];
        if let Some(preamble) = preamble.filter(|p| !p.is_empty()) {
            out.push(String::new());
            out.push(preamble.trim_end().to_string());
            out.push(String::new());
        }
        if !decls.is_empty() {
            out.push(String::new());
            out.push(decls.join("\n\n"));
            out.push(String::new());
        }
        for remark in self.format_schema_remarks(node) {
            out.push(String::new());
            out.push(remark);
        }
        let items = self.format_remark_items_children(ptr);
        let after_item = !items.is_empty() || !self.format_schema_remarks(node).is_empty();
        for remark in items {
            out.push(String::new());
            out.push(remark);
        }
        // Comments after the last declaration, directly above END_SCHEMA.
        // The blank line above them is already there after the last
        // declaration, and is not after an item or in an empty schema.
        let closing = self.format_trailing_region_remarks(node, "declarations");
        if !closing.is_empty() && (decls.is_empty() || after_item) {
            out.push(String::new());
        }
        out.extend(closing);
        out.push(format!("END_SCHEMA;{}", self.format_end_scope_remark(node)));
        out.join("\n")
    }

    // The schema's own tagged description (`(*"schema_id" ... *)`) as a
    // remark block, so a schema-level tagged description survives a
    // source round trip (it re-attaches to the schema). Mirrors the
    // ruby lane's format_schema_remarks byte-for-byte.
    fn format_schema_remarks(&self, node: &Value) -> Vec<String> {
        if self.no_remarks {
            return Vec::new();
        }
        let id = id_of(node).unwrap_or_default();
        arr(node, "remarks")
            .iter()
            .filter_map(|r| r.as_str())
            .filter(|s| !s.is_empty())
            .map(|remark| {
                if remark.contains('\n') {
                    format!("(*\"{id}\"\n{remark}\n*)")
                } else {
                    format!("(*\"{id}\" {remark} *)")
                }
            })
            .collect()
    }

    fn format_schema_head(&mut self, ptr: &str, node: &Value) -> String {
        let mut head = format!("SCHEMA {}", id_of(node).unwrap_or_default());
        if obj(node, "version").is_some() {
            head.push(' ');
            head.push_str(&self.child(ptr, "version"));
        }
        head.push(';');
        let mut out = vec![head];
        if !arr(node, "interfaces").is_empty() {
            out.push(String::new());
            out.push(self.list_join(ptr, "interfaces", "\n\n"));
        }
        out.join("\n")
    }

    fn format_entity(&mut self, ptr: &str, node: &Value) -> String {
        let mut items: Vec<String> = Vec::new();
        for field in ["attributes", "derived_attributes", "inverse_attributes"] {
            for (j, _a) in arr(node, field).iter().enumerate() {
                let item = format!("{ptr}/{field}/{j}");
                if !items.contains(&item) {
                    items.push(item);
                }
            }
        }
        let kind_of = |p: &str| {
            str_fld(self.node(p), "kind")
                .unwrap_or("EXPLICIT")
                .to_string()
        };
        let groups = |kind: &str| -> Vec<String> {
            items
                .iter()
                .filter(|p| kind_of(p) == kind)
                .cloned()
                .collect()
        };
        let explicit = groups("EXPLICIT");
        let derived = groups("DERIVED");
        let inverse = groups("INVERSE");

        let preamble = if !explicit.is_empty() {
            self.format_preamble_remarks(ptr, &self.indent(INDENT))
        } else {
            None
        };

        let mut header = format!("ENTITY {}", id_of(node).unwrap_or_default());
        let abstract_flag = truthy(node, "abstract");
        let has_supertype = obj(node, "supertype_expression").is_some();
        if abstract_flag && has_supertype {
            let rendered = self.child(ptr, "supertype_expression");
            header.push_str(&format!(
                "\n{}",
                self.indent(&format!("ABSTRACT SUPERTYPE OF ({rendered})"))
            ));
        } else if abstract_flag {
            header.push_str(&format!("\n{}", self.indent("ABSTRACT SUPERTYPE")));
        } else if has_supertype {
            let rendered = self.child(ptr, "supertype_expression");
            header.push_str(&format!(
                "\n{}",
                self.indent(&format!("SUPERTYPE OF ({rendered})"))
            ));
        }
        if !arr(node, "subtype_of").is_empty() {
            let subs = self.list_join(ptr, "subtype_of", ", ");
            header.push_str(&format!(
                "\n{}",
                self.indent(&format!("SUBTYPE OF ({subs})"))
            ));
        }
        header.push(';');

        let mut out = vec![header];
        if let Some(preamble) = preamble.filter(|p| !p.is_empty()) {
            out.push(preamble.trim_end().to_string());
        }
        if !explicit.is_empty() {
            let body = self.join_grouped(&explicit, "\n", true);
            out.push(self.indent(&body));
        }
        // A comment after the last member of a section closes it, before
        // the keyword of the next section or END_ENTITY.
        for remark in self.format_trailing_region_remarks(node, "attributes") {
            out.push(self.indent(&remark));
        }
        for (label, group, region) in [
            ("DERIVE", derived, "derived_attributes"),
            ("INVERSE", inverse, "inverse_attributes"),
        ] {
            if !group.is_empty() {
                let body = group
                    .iter()
                    .map(|p| self.dispatch(p))
                    .collect::<Vec<_>>()
                    .join("\n");
                out.push(format!("{label}\n{}", self.indent(&body)));
            }
            for remark in self.format_trailing_region_remarks(node, region) {
                out.push(self.indent(&remark));
            }
        }
        for (label, field) in [("UNIQUE", "unique_rules"), ("WHERE", "where_rules")] {
            if !arr(node, field).is_empty() {
                let body = self.list_join(ptr, field, "\n");
                out.push(format!("{label}\n{}", self.indent(&body)));
            }
            for remark in self.format_trailing_region_remarks(node, field) {
                out.push(self.indent(&remark));
            }
        }
        for remark in self.format_informal_propositions(ptr) {
            out.push(self.indent(&remark));
        }
        for remark in self.format_remark_items_children(ptr) {
            out.push(self.indent(&remark));
        }
        out.push(format!("END_ENTITY;{}", self.format_end_scope_remark(node)));
        out.join("\n")
    }

    /// The entity's informal propositions in the ISO remark-tag form so
    /// the source convention survives the round trip: `--IP1: text`, or
    /// the bare declaration marker `--IP1:` when the proposition
    /// carries no text (the WG12 `-- IP1:` markers). The attach stores
    /// the proposition's texts on its remark item.
    fn format_informal_propositions(&mut self, ptr: &str) -> Vec<String> {
        // The ISO shorthand lines are TAGGED-channel content: their texts
        // ride the proposition's remark item, and the plain form's keep
        // flag is for UNTAGGED comments only (#723) — a tagged
        // description must not reappear as an informal-proposition line.
        if self.no_remarks {
            return Vec::new();
        }
        let items_ptr = format!("{ptr}/informal_propositions");
        let count = self.node(&items_ptr).as_array().map(Vec::len).unwrap_or(0);
        (0..count)
            .map(|i| {
                let ip_ptr = format!("{items_ptr}/{i}");
                let id = id_of(self.node(&ip_ptr)).unwrap_or_default();
                let mut texts: Vec<String> = arr(self.node(&ip_ptr), "remarks")
                    .iter()
                    .filter_map(|r| r.as_str())
                    .filter(|r| !r.is_empty())
                    .map(str::to_string)
                    .collect();
                let ri_ptr = format!("{ip_ptr}/remark_items");
                let ri_count = self.node(&ri_ptr).as_array().map(Vec::len).unwrap_or(0);
                for j in 0..ri_count {
                    let ri = format!("{ri_ptr}/{j}");
                    for r in arr(self.node(&ri), "remarks") {
                        if let Some(t) = r.as_str() {
                            if !t.is_empty() {
                                texts.push(t.to_string());
                            }
                        }
                    }
                }
                if texts.is_empty() {
                    format!("--{id}:")
                } else {
                    // A multi-line text cannot ride the shorthand (#691)
                    // — it renders as the path-tagged block the source
                    // corpora write.
                    let tag = format!(
                        "{}.wr:{id}",
                        node_path(self.ctx, &ip_ptr).unwrap_or_default()
                    );
                    texts
                        .iter()
                        .map(|t| {
                            if t.contains('\n') {
                                format!(
                                    "(*\"{tag}\"
{t}
*)"
                                )
                            } else {
                                format!("--{id}: {t}")
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            })
            .collect()
    }

    fn format_type(&mut self, ptr: &str, node: &Value) -> String {
        // The remark of the last item of an ENUMERATION or of a SELECT.
        let last_item = arr(node.get("underlying_type").unwrap_or(&Value::Null), "items")
            .last()
            .filter(|item| is_list_item(item))
            .map(|item| self.formatted_inline_remarks(item, &|r: &Remark| r.region.is_none()))
            .unwrap_or_default();
        let mut out = vec![format!(
            "TYPE {} = {};{last_item}",
            id_of(node).unwrap_or_default(),
            self.child(ptr, "underlying_type")
        )];
        // A comment after the definition closes it, before WHERE or END_TYPE;
        // one after the last rule closes the type.
        for remark in self.format_trailing_region_remarks(node, "underlying_type") {
            out.push(self.indent(&remark));
        }
        if !arr(node, "where_rules").is_empty() {
            let body = self.list_join(ptr, "where_rules", "\n");
            out.push(format!("WHERE\n{}", self.indent(&body)));
        }
        for remark in self.format_trailing_region_remarks(node, "where_rules") {
            out.push(self.indent(&remark));
        }
        for remark in self.format_remark_items_children(ptr) {
            out.push(self.indent(&remark));
        }
        out.push(format!("END_TYPE;{}", self.format_end_scope_remark(node)));
        // The enumeration items render inline inside ENUMERATION OF
        // (...) and never reach the dispatch, so their tagged
        // descriptions render here, after the TYPE (#694).
        if !self.no_remarks {
            let items_ptr = format!("{ptr}/underlying_type/items");
            let item_count = self.node(&items_ptr).as_array().map(Vec::len).unwrap_or(0);
            for i in 0..item_count {
                let item_ptr = format!("{items_ptr}/{i}");
                for block in self.owner_tagged_blocks(&item_ptr) {
                    out.push(String::new());
                    out.push(block);
                }
            }
        }
        out.join("\n")
    }

    fn format_function(&mut self, ptr: &str, node: &Value) -> String {
        self.format_callable(ptr, node, "FUNCTION", "END_FUNCTION", true)
    }

    fn format_procedure(&mut self, ptr: &str, node: &Value) -> String {
        self.format_callable(ptr, node, "PROCEDURE", "END_PROCEDURE", false)
    }

    fn format_callable(
        &mut self,
        ptr: &str,
        node: &Value,
        keyword: &str,
        end_keyword: &str,
        is_function: bool,
    ) -> String {
        let id = id_of(node).unwrap_or_default();
        let mut head = format!("{keyword} {id}");
        if node
            .get("parameters")
            .map(|v| !v.is_null())
            .unwrap_or(false)
        {
            let param_indent = " ".repeat(format!("{keyword} {id}(").len());
            let count = arr(node, "parameters").len();
            let param_ptrs: Vec<String> = (0..count)
                .map(|i| format!("{ptr}/parameters/{i}"))
                .collect();
            let items = self.join_grouped(&param_ptrs, &format!(";\n{param_indent}"), false);
            head.push_str(&format!("({items})"));
        }
        if is_function {
            head.push_str(&format!(" : {};", self.child(ptr, "return_type")));
        } else {
            head.push(';');
        }

        let mut out = vec![head];
        for field in [
            "types",
            "entities",
            "subtype_constraints",
            "functions",
            "procedures",
        ] {
            if !arr(node, field).is_empty() {
                let body = self.list_join(ptr, field, "\n");
                out.push(self.indent(&body));
            }
        }
        if !arr(node, "constants").is_empty() {
            let inner = self.list_join(ptr, "constants", "\n");
            out.push(self.indent(&format!("CONSTANT\n{}\nEND_CONSTANT;", self.indent(&inner))));
        }
        // A comment on its own line before LOCAL closes the declarations.
        for remark in self.format_trailing_region_remarks(node, "declarations") {
            out.push(self.indent(&remark));
        }
        if !arr(node, "variables").is_empty() {
            let mut inner = self.list_join_grouped(ptr, "variables", "\n", false);
            for remark in self.format_trailing_region_remarks(node, "variables") {
                inner.push('\n');
                inner.push_str(&remark);
            }
            out.push(self.indent(&format!("LOCAL\n{}\nEND_LOCAL;", self.indent(&inner))));
        }
        if !arr(node, "statements").is_empty() {
            let body = self.list_join(ptr, "statements", "\n");
            out.push(self.indent(&body));
        }
        for remark in self.format_trailing_region_remarks(node, "statements") {
            out.push(self.indent(&remark));
        }
        for remark in self.format_remark_items_children(ptr) {
            out.push(self.indent(&remark));
        }
        out.push(format!(
            "{end_keyword};{}",
            self.format_end_scope_remark(node)
        ));
        out.join("\n")
    }

    fn format_rule(&mut self, ptr: &str, node: &Value) -> String {
        let applies = self.list_join(ptr, "applies_to", ", ");
        let mut out = vec![format!(
            "RULE {} FOR ({});",
            id_of(node).unwrap_or_default(),
            applies
        )];
        for field in [
            "types",
            "entities",
            "subtype_constraints",
            "functions",
            "procedures",
        ] {
            if !arr(node, field).is_empty() {
                let body = self.list_join(ptr, field, "\n");
                out.push(self.indent(&body));
            }
        }
        if !arr(node, "constants").is_empty() {
            let inner = self.list_join(ptr, "constants", "\n");
            out.push(self.indent(&format!("CONSTANT\n{}\nEND_CONSTANT;", self.indent(&inner))));
        }
        // A comment on its own line before LOCAL closes the declarations.
        for remark in self.format_trailing_region_remarks(node, "declarations") {
            out.push(self.indent(&remark));
        }
        if !arr(node, "variables").is_empty() {
            let mut inner = self.list_join_grouped(ptr, "variables", "\n", false);
            for remark in self.format_trailing_region_remarks(node, "variables") {
                inner.push('\n');
                inner.push_str(&remark);
            }
            out.push(self.indent(&format!("LOCAL\n{}\nEND_LOCAL;", self.indent(&inner))));
        }
        let stmt_count = arr(node, "statements").len();
        if stmt_count > 0 {
            let mut kept: Vec<String> = Vec::new();
            for j in 0..stmt_count {
                let stmt_ptr = format!("{ptr}/statements/{j}");
                if self.carries_placed_remark(&stmt_ptr) {
                    kept.push(stmt_ptr);
                    continue;
                }
                let stmt = self.node(&stmt_ptr).clone();
                if is(&stmt, "Statements::Alias") || is(&stmt, "Statements::Repeat") {
                    let all_null = arr(&stmt, "statements")
                        .iter()
                        .all(|s| is(s, "Statements::Null"));
                    if all_null {
                        continue;
                    }
                } else if is(&stmt, "Statements::Null") {
                    continue;
                }
                kept.push(stmt_ptr);
            }
            if !kept.is_empty() {
                let body = kept
                    .iter()
                    .map(|p| self.dispatch(p))
                    .collect::<Vec<_>>()
                    .join("\n");
                out.push(self.indent(&body));
            }
        }
        for remark in self.format_trailing_region_remarks(node, "statements") {
            out.push(self.indent(&remark));
        }
        if !arr(node, "where_rules").is_empty() {
            let body = self.list_join(ptr, "where_rules", "\n");
            out.push(format!("WHERE\n{}", self.indent(&body)));
        }
        for remark in self.format_trailing_region_remarks(node, "where_rules") {
            out.push(self.indent(&remark));
        }
        for remark in self.format_remark_items_children(ptr) {
            out.push(self.indent(&remark));
        }
        out.push(format!("END_RULE;{}", self.format_end_scope_remark(node)));
        out.join("\n")
    }

    fn carries_placed_remark(&self, ptr: &str) -> bool {
        if self.no_remarks && !self.keep_untagged_remarks {
            return false;
        }
        arr(self.node(ptr), "untagged_remarks")
            .iter()
            .any(|r| Remark::from(r).placement.is_some())
    }

    fn format_interface(&mut self, ptr: &str, node: &Value) -> String {
        let kind = match str_fld(node, "kind") {
            Some("USE") => "USE",
            _ => "REFERENCE",
        };
        let mut out = format!("{kind} FROM {}", self.child(ptr, "schema"));
        if !arr(node, "items").is_empty() {
            let item_indent = " ".repeat("(".len());
            let count = arr(node, "items").len();
            let items = (0..count)
                .map(|i| self.child(ptr, &format!("items/{i}")))
                .collect::<Vec<_>>()
                .join(&format!(",\n{item_indent}"));
            // A remark that followed the opening parenthesis keeps it on
            // the clause's first line: `USE FROM x ( -- why`.
            let after_parenthesis =
                self.formatted_inline_remarks(node, &|r: &Remark| r.inline_region("items"));
            if after_parenthesis.is_empty() {
                out.push_str(&format!("\n{};", self.indent(&format!("({items})"))));
            } else {
                let list = self.indent(&format!("{item_indent}{items})"));
                out.push_str(&format!(" ({after_parenthesis}\n{list};"));
            }
        } else {
            out.push(';');
        }
        out
    }

    fn format_interface_item(&mut self, ptr: &str, node: &Value) -> String {
        let mut out = self.child(ptr, "ref");
        if let Some(id) = id_of(node) {
            out.push_str(&format!(" AS {id}"));
        }
        for remark in self.format_remark_items_children(ptr) {
            out.push('\n');
            out.push_str(&remark);
        }
        out
    }

    fn format_constant(&mut self, ptr: &str, node: &Value) -> String {
        let mut out = format!(
            "{} : {} := {};",
            id_of(node).unwrap_or_default(),
            self.child(ptr, "type"),
            self.child(ptr, "expression")
        );
        // A comment that trailed the constant on its line goes back on
        // it (#735).
        out.push_str(&self.format_inline_tail_remark(node));
        for remark in self.format_remark_items_children(ptr) {
            out.push('\n');
            out.push_str(&remark);
        }
        out
    }

    fn format_attribute(&mut self, ptr: &str, node: &Value) -> String {
        self.format_attribute_ids(ptr, node, None)
    }

    /// expressir#655: `ids` carries the joined sibling names when this
    /// attribute leads a multi-name source declaration.
    fn format_attribute_ids(&mut self, ptr: &str, node: &Value, ids: Option<&str>) -> String {
        let mut out = String::new();
        if obj(node, "supertype_attribute").is_some() {
            out.push_str(&self.child(ptr, "supertype_attribute"));
            out.push(' ');
            let supertype_attr_id = obj(node, "supertype_attribute")
                .and_then(|sa| obj(sa, "attribute"))
                .and_then(id_of);
            if supertype_attr_id.map(|attr_id| id_of(node) != Some(attr_id)) == Some(true) {
                out.push_str(&format!("RENAMED {} ", id_of(node).unwrap_or_default()));
            }
        } else if id_of(node).is_some() {
            let names = ids.unwrap_or_else(|| id_of(node).unwrap_or_default());
            out.push_str(&format!("{names} "));
        }
        out.push(':');
        if truthy(node, "optional") {
            out.push_str(" OPTIONAL");
        }
        out.push(' ');
        out.push_str(&self.child(ptr, "type"));
        match str_fld(node, "kind") {
            Some("DERIVED") => out.push_str(&format!(" := {}", self.child(ptr, "expression"))),
            Some("INVERSE") => out.push_str(&format!(" FOR {}", self.child(ptr, "expression"))),
            _ => {}
        }
        out.push(';');
        out.push_str(&self.format_inline_tail_remark(node));
        let items = self.format_remark_items_children(ptr);
        for remark in items {
            out.push('\n');
            out.push_str(&remark);
        }
        out
    }

    fn format_parameter(&mut self, ptr: &str, node: &Value) -> String {
        self.format_parameter_ids(ptr, node, None)
    }

    fn format_parameter_ids(&mut self, ptr: &str, node: &Value, ids: Option<&str>) -> String {
        let var = if truthy(node, "var") { "VAR " } else { "" };
        let names = ids.unwrap_or_else(|| id_of(node).unwrap_or_default());
        let mut out = format!("{var}{names} : {}", self.child(ptr, "type"));
        for remark in self.format_remark_items_children(ptr) {
            out.push('\n');
            out.push_str(&remark);
        }
        out
    }

    fn format_variable(&mut self, ptr: &str, node: &Value) -> String {
        self.format_variable_ids(ptr, node, None)
    }

    fn format_variable_ids(&mut self, ptr: &str, node: &Value, ids: Option<&str>) -> String {
        let expr = if obj(node, "expression").is_some() {
            format!(" := {}", self.child(ptr, "expression"))
        } else {
            String::new()
        };
        let names = ids.unwrap_or_else(|| id_of(node).unwrap_or_default());
        let mut out = format!("{names} : {}{expr};", self.child(ptr, "type"));
        for remark in self.format_remark_items_children(ptr) {
            out.push('\n');
            out.push_str(&remark);
        }
        out
    }

    fn format_unique_rule(&mut self, ptr: &str, node: &Value) -> String {
        let label = id_of(node).map(|id| format!("{id}: ")).unwrap_or_default();
        let mut out = format!("{label}{};", self.list_join(ptr, "attributes", ", "));
        // A comment that trailed the unique rule on its line goes back
        // on it (#735).
        out.push_str(&self.format_inline_tail_remark(node));
        for remark in self.format_remark_items_children(ptr) {
            out.push('\n');
            out.push_str(&remark);
        }
        out
    }

    fn format_where_rule(&mut self, ptr: &str, node: &Value) -> String {
        // A remark after the label (`wr1 : -- why`, the expression on the
        // next line) stays on the label's line: place_remarks puts it at
        // the end of the first line, so the expression goes on a line of
        // its own.
        let after_label = !self
            .formatted_inline_remarks(node, &|r: &Remark| r.opener())
            .is_empty();
        let expression = self.child(ptr, "expression");
        // A rule without a label has no first line for that remark. When
        // another remark ends the rule's line, the two would share it and
        // the second would become part of the first: the first line is
        // left empty and takes the remark, on a line of its own.
        let own_line = after_label
            && !self
                .formatted_inline_remarks(node, &|r: &Remark| r.region.is_none())
                .is_empty();
        let mut out = match (id_of(node), after_label) {
            (Some(id), true) => format!("{id}:\n{};", self.indent(&expression)),
            (Some(id), false) => format!("{id}: {expression};"),
            (None, _) if own_line => format!("\n{expression};"),
            (None, _) => format!("{expression};"),
        };
        for remark in self.format_remark_items_children(ptr) {
            out.push('\n');
            out.push_str(&remark);
        }
        out
    }

    fn format_subtype_constraint(&mut self, ptr: &str, node: &Value) -> String {
        let mut out = vec![format!(
            "SUBTYPE_CONSTRAINT {} FOR {};",
            id_of(node).unwrap_or_default(),
            self.child(ptr, "applies_to")
        )];
        if truthy(node, "abstract") {
            out.push(self.indent("ABSTRACT SUPERTYPE;"));
        }
        if !arr(node, "total_over").is_empty() {
            let items = self.list_join(ptr, "total_over", ", ");
            out.push(self.indent(&format!("TOTAL_OVER({items});")));
        }
        if obj(node, "supertype_expression").is_some() {
            let rendered = self.child(ptr, "supertype_expression");
            out.push(self.indent(&format!("{rendered};")));
        }
        for remark in self.format_remark_items_children(ptr) {
            out.push(self.indent(&remark));
        }
        out.push(format!(
            "END_SUBTYPE_CONSTRAINT;{}",
            self.format_end_scope_remark(node)
        ));
        out.join("\n")
    }

    fn format_remark_item(&self, ptr: &str, node: &Value) -> String {
        // no_remarks strips everything the RemarkFormatter would emit —
        // the Ruby formatter guards every emission point (remark_formatter.rb),
        // and the lane-parity fuzz (expressir#624) catches the leak.
        if self.no_remarks {
            return String::new();
        }
        let remarks: Vec<String> = arr(node, "remarks")
            .iter()
            .filter_map(|r| r.as_str().map(str::to_string))
            .collect();
        if remarks.is_empty() {
            return String::new();
        }
        // Rule-context items (WHERE/UNIQUE/IP) historically carry bare
        // tags — `--"unusual_placement" text` — while WG12-style items
        // carry the full path. Preserve each owner's form.
        let owner = self
            .ctx
            .parent
            .get(ptr)
            .map(|p| tail(self.node(p)))
            .unwrap_or_default();
        let rule_context = matches!(
            owner,
            "WhereRule" | "UniqueRule" | "InformalPropositionRule"
        );
        // A referable `__note:id` has no rule to anchor it in a bare tail
        // tag (it would reattach to the schema), so it carries the path.
        let bare = rule_context && !id_of(node).is_some_and(|id| id.starts_with("__"));
        let path = if bare {
            id_of(node).map(str::to_string)
        } else {
            node_path(self.ctx, ptr)
        };
        // Remark items are WG12 annotation blocks: the embedded form,
        // a block per remark, matching the source convention of the
        // annotated corpora (the conservation pairs are embedded, so
        // tail-form emission reads as phantom gains). Rule-context
        // owners keep the historic bare-tag tail form.
        let tag = path.unwrap_or_default();
        if rule_context {
            return remarks
                .iter()
                .map(|r| {
                    if r.contains('\n') {
                        // A multi-line remark cannot ride the `--` tail
                        // form: its continuation lines would leak as
                        // bare source text the parser rejects (#691).
                        format!("(*\"{tag}\"\n{r}\n*)")
                    } else {
                        format!("--\"{tag}\" {r}")
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
        }
        remarks
            .iter()
            .map(|r| {
                if r.contains('\n') {
                    format!("(*\"{tag}\"\n{r}\n*)")
                } else {
                    format!("(*\"{tag}\" {r} *)")
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Every `remark_items` child of `ptr`, rendered (the tagged
    /// remark-item blocks — WG12 `(*"path" ... *)` annotations).
    fn format_remark_items_children(&mut self, ptr: &str) -> Vec<String> {
        let count = arr(self.node(ptr), "remark_items").len();
        (0..count)
            .map(|i| self.child(ptr, &format!("remark_items/{i}")))
            .filter(|s| !s.is_empty())
            .collect()
    }

    // ---- data types ----

    fn format_data_type(&mut self, ptr: &str, node: &Value, class: &str) -> String {
        match class.rsplit("::").next().unwrap_or("") {
            "Integer" => "INTEGER".to_string(),
            "Number" => "NUMBER".to_string(),
            "Boolean" => "BOOLEAN".to_string(),
            "Logical" => "LOGICAL".to_string(),
            "Real" => {
                if obj(node, "precision").is_some() {
                    format!("REAL ({})", self.child(ptr, "precision"))
                } else {
                    "REAL".to_string()
                }
            }
            "String" => {
                let mut out = "STRING".to_string();
                if obj(node, "width").is_some() {
                    out.push_str(&format!(" ({})", self.child(ptr, "width")));
                }
                if truthy(node, "fixed") {
                    out.push_str(" FIXED");
                }
                out
            }
            "Binary" => {
                let mut out = "BINARY".to_string();
                if obj(node, "width").is_some() {
                    out.push_str(&format!(" ({})", self.child(ptr, "width")));
                }
                if truthy(node, "fixed") {
                    out.push_str(" FIXED");
                }
                out
            }
            "List" => self.format_bound_type(ptr, "LIST", false),
            "Set" => self.format_bound_type(ptr, "SET", false),
            "Bag" => self.format_bound_type(ptr, "BAG", false),
            "Array" => self.format_bound_type(ptr, "ARRAY", true),
            "Enumeration" => self.format_enumeration(ptr, node),
            "Select" => self.format_select(ptr, node),
            "Aggregate" => {
                if obj(node, "base_type").is_some() {
                    format!("AGGREGATE OF {}", self.child(ptr, "base_type"))
                } else {
                    "AGGREGATE".to_string()
                }
            }
            "Generic" | "GenericEntity" => {
                let keyword = if class.ends_with("GenericEntity") {
                    "GENERIC_ENTITY"
                } else {
                    "GENERIC"
                };
                match id_of(node) {
                    Some(id) => format!("{keyword} : {id}"),
                    None => keyword.to_string(),
                }
            }
            _ => String::new(),
        }
    }

    fn format_bound_type(&mut self, ptr: &str, keyword: &str, is_array: bool) -> String {
        let node = self.node(ptr).clone();
        let mut out = keyword.to_string();
        if obj(&node, "bound1").is_some() && obj(&node, "bound2").is_some() {
            out.push_str(&format!(
                " [{}:{}]",
                self.child(ptr, "bound1"),
                self.child(ptr, "bound2")
            ));
        }
        out.push_str(" OF");
        if is_array {
            if truthy(&node, "optional") {
                out.push_str(" OPTIONAL");
            }
            if truthy(&node, "unique") {
                out.push_str(" UNIQUE");
            }
        } else if truthy(&node, "unique") {
            out.push_str(" UNIQUE");
        }
        out.push(' ');
        out.push_str(&self.child(ptr, "base_type"));
        out
    }

    fn format_enumeration(&mut self, ptr: &str, node: &Value) -> String {
        let mut out = String::new();
        if truthy(node, "extensible") {
            out.push_str("EXTENSIBLE ");
        }
        out.push_str("ENUMERATION");
        if obj(node, "based_on").is_some() {
            out.push_str(&format!(" BASED_ON {}", self.child(ptr, "based_on")));
            if !arr(node, "items").is_empty() {
                let items = self.indent_items(ptr, "items");
                out.push_str(&format!(" WITH\n{items}"));
            }
        } else if !arr(node, "items").is_empty() {
            out.push_str(" OF\n");
            out.push_str(&self.indent_items(ptr, "items"));
        }
        out
    }

    fn format_select(&mut self, ptr: &str, node: &Value) -> String {
        let mut out = String::new();
        if truthy(node, "extensible") {
            out.push_str("EXTENSIBLE ");
        }
        if truthy(node, "generic_entity") {
            out.push_str("GENERIC_ENTITY ");
        }
        out.push_str("SELECT");
        if obj(node, "based_on").is_some() {
            out.push_str(&format!(" BASED_ON {}", self.child(ptr, "based_on")));
            if !arr(node, "items").is_empty() {
                let items = self.indent_items(ptr, "items");
                out.push_str(&format!(" WITH\n{items}"));
            }
        } else if !arr(node, "items").is_empty() {
            out.push('\n');
            out.push_str(&self.indent_items(ptr, "items"));
        }
        out
    }

    /// `(item,\n  item)` with the one-space item indent, already wrapped
    /// by `self.indent(...)` — the shape every enumeration/select site emits.
    fn indent_items(&mut self, ptr: &str, field: &str) -> String {
        let item_indent = " ".repeat("(".len());
        let count = arr(self.node(ptr), field).len();
        // A remark that follows an item on its line is written after the
        // comma. The remark of the last item is written after the `;` of
        // the TYPE (`format_type`): before it, a tail remark would take
        // the `)` and the `;` with it. The remarks that stand on lines of
        // their own above an item come back from `child` above the item:
        // each remark and the item start at the depth of the items, and
        // the lines inside a remark of several lines are left as they are.
        let items = (0..count)
            .map(|i| {
                let item_ptr = format!("{field}/{i}");
                let item = self.node(&format!("{ptr}/{item_ptr}")).clone();
                let mut text = self.child(ptr, &item_ptr);
                let leading = self.format_leading_statement_remarks(&item);
                let above = format!("{}\n", leading.join("\n"));
                if let Some(own) = text.strip_prefix(&above).filter(|_| !leading.is_empty()) {
                    text = leading
                        .iter()
                        .map(String::as_str)
                        .chain([own])
                        .collect::<Vec<_>>()
                        .join(&format!("\n{item_indent}"));
                }
                if i + 1 == count {
                    return text;
                }
                let remark = if is_list_item(&item) {
                    self.formatted_inline_remarks(&item, &|r: &Remark| r.region.is_none())
                } else {
                    String::new()
                };
                format!("{text},{remark}")
            })
            .collect::<Vec<_>>()
            .join(&format!("\n{item_indent}"));
        self.indent(&format!("({items})"))
    }

    // ---- literals + references ----

    fn format_literal(&self, node: &Value) -> String {
        let t = tail(node);
        match t {
            "Logical" => match str_fld(node, "value") {
                Some("TRUE") => "TRUE".to_string(),
                Some("FALSE") => "FALSE".to_string(),
                _ => "UNKNOWN".to_string(),
            },
            "String" => {
                let value = node.get("value").map(value_text).unwrap_or_default();
                if truthy(node, "encoded") {
                    format!("\"{value}\"")
                } else {
                    format!("'{value}'")
                }
            }
            _ => node.get("value").map(value_text).unwrap_or_default(),
        }
    }

    fn format_reference(&mut self, ptr: &str, node: &Value, t: &str) -> String {
        match t {
            "SimpleReference" => {
                let id = id_of(node).unwrap_or_default().to_string();
                if self.flavor == Flavor::Source {
                    return id;
                }
                let Some(base_path) = str_fld(node, "base_path") else {
                    return id;
                };
                if self.flavor == Flavor::Adoc {
                    let parts: Vec<&str> = base_path.split('.').collect();
                    if parts.len() < 2 {
                        return id;
                    }
                    return format!("<<{},{}>>", parts[parts.len() - 2..].join("."), id);
                }
                // The ruby walk: the closest node with a path (the
                // reference itself); skip when it shares the
                // reference's first two path parts (same schema).
                let mut parts: Vec<&str> = Vec::new();
                let mut current = ptr.to_string();
                loop {
                    let owner = self.ctx.node(&current);
                    if !is(owner, "References::SimpleReference") {
                        parts.push(id_of(owner).unwrap_or(""));
                    }
                    match self.ctx.parent.get(&current) {
                        Some(parent) if !parent.is_empty() => current = parent.clone(),
                        _ => break,
                    }
                }
                // The walk stops at the schema (the parent map's top);
                // the ExpFile root contributes id-ELSE-path (the ruby
                // path rule), giving the leading part the fills use.
                if current.starts_with("/schemas/") {
                    parts.push(
                        id_of(self.ctx.root)
                            .or_else(|| str_fld(self.ctx.root, "path"))
                            .unwrap_or(""),
                    );
                }
                parts.reverse();
                // The comparison is on the path STRINGS split by '.'
                // (a dotted file name splits on both sides alike).
                let path = parts.join(".");
                let path_parts: Vec<&str> = path.split('.').collect();
                let base_parts: Vec<&str> = base_path.split('.').collect();
                let same_head = path_parts.len().min(2) == base_parts.len().min(2)
                    && path_parts[..path_parts.len().min(2)]
                        == base_parts[..base_parts.len().min(2)];
                if same_head {
                    id
                } else {
                    format!("{{{{<<express:{base_path},{id}>>}}}}")
                }
            }
            "AttributeReference" => format!(
                "{}.{}",
                self.child(ptr, "ref"),
                self.child(ptr, "attribute")
            ),
            "GroupReference" => {
                format!("{}\\{}", self.child(ptr, "ref"), self.child(ptr, "entity"))
            }
            "IndexReference" => {
                let idx2 = if obj(node, "index2").is_some() {
                    format!(":{}", self.child(ptr, "index2"))
                } else {
                    String::new()
                };
                format!(
                    "{}[{}{}]",
                    self.child(ptr, "ref"),
                    self.child(ptr, "index1"),
                    idx2
                )
            }
            _ => String::new(),
        }
    }

    // ---- expressions ----

    /// The relational tier is non-associative: its same-precedence
    /// children keep parentheses on both sides (expressir#651).
    const RELATIONAL_PRECEDENCE: i32 = 4;

    fn operator_precedence(operator: &str) -> i32 {
        match operator {
            "EXPONENTIATION" => 1,
            "MULTIPLICATION" | "REAL_DIVISION" | "INTEGER_DIVISION" | "MODULO" | "AND"
            | "COMBINE" => 2,
            "SUBTRACTION" | "ADDITION" | "OR" | "XOR" => 3,
            "EQUAL"
            | "NOT_EQUAL"
            | "LESS_THAN_OR_EQUAL"
            | "GREATER_THAN_OR_EQUAL"
            | "LESS_THAN"
            | "GREATER_THAN"
            | "INSTANCE_EQUAL"
            | "INSTANCE_NOT_EQUAL"
            | "IN"
            | "LIKE" => 4,
            _ => 0,
        }
    }

    fn format_expression(&mut self, ptr: &str, node: &Value, t: &str) -> String {
        match t {
            "BinaryExpression" => {
                let operator = str_fld(node, "operator").unwrap_or("").to_string();
                let precedence = Self::operator_precedence(&operator);
                // The minimal-paren policy (expressir#651 — the SC4UTILITI-199
                // normalization contract): EXPRESS's arithmetic and logical
                // operators are left-associative, so a same-precedence child
                // needs no parentheses on the left (`(a+b)+c` == `a+b+c`) and
                // same-or-looser children need them on the right
                // (`a+(b+c)` keeps its parens). The relational tier
                // (=, <>, <, >, <=, >=, :=:, :<>:, IN, LIKE) is
                // NON-associative — a chain re-parses to a different tree or
                // not at all — so same-precedence children keep parentheses
                // on BOTH sides. Looser children always get parentheses.
                // Higher number = looser binding.
                let relational = precedence == Self::RELATIONAL_PRECEDENCE;
                let wraps = |field: &str, min_precedence: i32, same_side: bool| {
                    obj(node, field)
                        .map(|o| {
                            // A source-parenthesized operand carries its
                            // own pairs (#707); the printer adds none.
                            if o.get("parenthesized").and_then(Value::as_i64).unwrap_or(0) > 0 {
                                return false;
                            }
                            is(o, "Expressions::BinaryExpression") && {
                                let child =
                                    Self::operator_precedence(str_fld(o, "operator").unwrap_or(""));
                                child >= min_precedence
                                    || (relational && same_side && child == precedence)
                            }
                        })
                        .unwrap_or(false)
                };
                let op1 = wraps("operand1", precedence + 1, true);
                let op2 = wraps("operand2", precedence, true);
                let spelling = match operator.as_str() {
                    "ADDITION" => "+",
                    "AND" => "AND",
                    "COMBINE" => "||",
                    "EQUAL" => "=",
                    "EXPONENTIATION" => "**",
                    "GREATER_THAN" => ">",
                    "GREATER_THAN_OR_EQUAL" => ">=",
                    "IN" => "IN",
                    "INSTANCE_EQUAL" => ":=:",
                    "INSTANCE_NOT_EQUAL" => ":<>:",
                    "INTEGER_DIVISION" => "DIV",
                    "LESS_THAN" => "<",
                    "LESS_THAN_OR_EQUAL" => "<=",
                    "LIKE" => "LIKE",
                    "MODULO" => "MOD",
                    "MULTIPLICATION" => "*",
                    "NOT_EQUAL" => "<>",
                    "OR" => "OR",
                    "REAL_DIVISION" => "/",
                    "SUBTRACTION" => "-",
                    "XOR" => "XOR",
                    other => other,
                };
                let left = if op1 {
                    format!("({})", self.child(ptr, "operand1"))
                } else {
                    self.child(ptr, "operand1")
                };
                let right = if op2 {
                    format!("({})", self.child(ptr, "operand2"))
                } else {
                    self.child(ptr, "operand2")
                };
                format!("{left} {spelling} {right}")
            }
            "UnaryExpression" => {
                let operator = str_fld(node, "operator").unwrap_or("").to_string();
                let operand = self.node(&format!("{ptr}/operand")).clone();
                let needs_parens = operand
                    .get("parenthesized")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    == 0
                    && (is(&operand, "Expressions::BinaryExpression")
                        || is(&operand, "Expressions::Interval")
                        || is(&operand, "Expressions::UnaryExpression"));
                let sign = match operator.as_str() {
                    "MINUS" => "-",
                    "NOT" => "NOT",
                    "PLUS" => "+",
                    other => other,
                };
                let space = if operator == "NOT" { " " } else { "" };
                let inner = self.child(ptr, "operand");
                if needs_parens {
                    format!("{sign}{space}({inner})")
                } else {
                    format!("{sign}{space}{inner}")
                }
            }
            "Interval" => {
                let op = |v: Option<&str>| {
                    match v {
                        Some("LESS_THAN") => "<",
                        _ => "<=",
                    }
                    .to_string()
                };
                format!(
                    "{{{} {} {} {} {}}}",
                    self.child(ptr, "low"),
                    op(str_fld(node, "operator1")),
                    self.child(ptr, "item"),
                    op(str_fld(node, "operator2")),
                    self.child(ptr, "high"),
                )
            }
            "AggregateInitializer" => {
                let items = self.list_join(ptr, "items", ", ");
                format!("[{items}]")
            }
            "AggregateInitializerItem" => {
                format!(
                    "{}:{}",
                    self.child(ptr, "expression"),
                    self.child(ptr, "repetition")
                )
            }
            "EntityConstructor" => {
                let params = self.list_join(ptr, "parameters", ", ");
                format!("{}({})", self.child(ptr, "entity"), params)
            }
            "FunctionCall" => {
                let params = if !node
                    .get("parameters")
                    .map(|v| !v.is_null())
                    .unwrap_or(false)
                {
                    String::new()
                } else {
                    format!("({})", self.list_join(ptr, "parameters", ", "))
                };
                format!("{}{}", self.child(ptr, "function"), params)
            }
            "QueryExpression" => {
                let remarks = self.format_remarks(ptr, node);
                let remark_block = if remarks.is_empty() {
                    String::new()
                } else {
                    format!("\n{}\n", remarks.join("\n"))
                };
                format!(
                    "QUERY({} <* {} | {}{})",
                    id_of(node).unwrap_or_default(),
                    self.child(ptr, "aggregate_source"),
                    self.child(ptr, "expression"),
                    remark_block
                )
            }
            _ => String::new(),
        }
    }

    // ---- statements ----

    fn format_statement(&mut self, ptr: &str, node: &Value, t: &str) -> String {
        match t {
            "Alias" => {
                let mut out = vec![format!(
                    "ALIAS {} FOR {};",
                    id_of(node).unwrap_or_default(),
                    self.child(ptr, "expression")
                )];
                if !arr(node, "statements").is_empty() {
                    let body = self.list_join(ptr, "statements", "\n");
                    out.push(self.indent(&body));
                }
                for remark in self.format_block_end_remarks(ptr, node) {
                    out.push(self.indent(&remark));
                }
                out.push("END_ALIAS;".to_string());
                out.join("\n")
            }
            "Assignment" => format!(
                "{} := {};",
                self.child(ptr, "ref"),
                self.child(ptr, "expression")
            ),
            "Case" => {
                let mut out = vec![format!("CASE {} OF", self.child(ptr, "expression"))];
                let action_count = arr(node, "actions").len();
                for j in 0..action_count {
                    out.push(self.child(ptr, &format!("actions/{j}")));
                }
                for remark in self.format_trailing_region_remarks(node, "action_statements") {
                    out.push(self.indent(&remark));
                }
                if obj(node, "otherwise_statement").is_some() {
                    let mid = self.format_mid_keyword_remarks(node, "otherwise");
                    out.push(format!("OTHERWISE :{mid}"));
                    let rendered = self.child(ptr, "otherwise_statement");
                    out.push(self.indent(&rendered));
                }
                for remark in self.format_trailing_region_remarks(node, "otherwise_statements") {
                    out.push(self.indent(&remark));
                }
                out.push("END_CASE;".to_string());
                out.join("\n")
            }
            "CaseAction" => {
                let label_remark = self.case_label_remark(ptr);
                // The remarks written before the label (own-line comments
                // between two actions) stand above it.
                let above =
                    self.format_before_label_remarks(self.node(&format!("{ptr}/statement")));
                let head = format!(
                    "{} :{}\n{}",
                    self.list_join(ptr, "labels", ", "),
                    label_remark,
                    {
                        let rendered = self.child(ptr, "statement");
                        self.indent(&rendered)
                    }
                );
                above
                    .into_iter()
                    .chain([head])
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            "Compound" => {
                let mut out = vec!["BEGIN".to_string()];
                if !arr(node, "statements").is_empty() {
                    let body = self.list_join(ptr, "statements", "\n");
                    out.push(self.indent(&body));
                }
                for remark in self.format_trailing_region_remarks(node, "statements") {
                    out.push(self.indent(&remark));
                }
                out.push("END;".to_string());
                out.join("\n")
            }
            "Escape" => "ESCAPE;".to_string(),
            "If" => {
                let has_else = !arr(node, "else_statements").is_empty();
                let then_trailing: Vec<String> = self
                    .format_trailing_region_remarks(node, "statements")
                    .into_iter()
                    .map(|x| self.indent(&x))
                    .collect();
                let else_trailing: Vec<String> = self
                    .format_trailing_region_remarks(node, "else_statements")
                    .into_iter()
                    .map(|x| self.indent(&x))
                    .collect();
                let mut out = vec![format!("IF {} THEN", self.child(ptr, "expression"))];
                if !arr(node, "statements").is_empty() {
                    let body = self.list_join(ptr, "statements", "\n");
                    out.push(self.indent(&body));
                }
                if has_else {
                    out.extend(then_trailing.clone());
                    let mid = self.format_mid_keyword_remarks(node, "else");
                    let else_body = self.list_join(ptr, "else_statements", "\n");
                    out.push(format!("ELSE{mid}\n{}", self.indent(&else_body)));
                    out.extend(else_trailing);
                } else {
                    out.extend(then_trailing);
                }
                out.push("END_IF;".to_string());
                out.join("\n")
            }
            "Null" => ";".to_string(),
            "ProcedureCall" => {
                let params = if arr(node, "parameters").is_empty() {
                    String::new()
                } else {
                    format!("({})", self.list_join(ptr, "parameters", ", "))
                };
                format!("{}{};", self.child(ptr, "procedure"), params)
            }
            "Repeat" => {
                let mut head = "REPEAT".to_string();
                if let Some(id) = id_of(node) {
                    head.push_str(&format!(
                        " {id} := {} TO {}",
                        self.child(ptr, "bound1"),
                        self.child(ptr, "bound2")
                    ));
                    if obj(node, "increment").is_some() {
                        head.push_str(&format!(" BY {}", self.child(ptr, "increment")));
                    }
                }
                if obj(node, "while_expression").is_some() {
                    head.push_str(&format!(" WHILE {}", self.child(ptr, "while_expression")));
                }
                if obj(node, "until_expression").is_some() {
                    head.push_str(&format!(" UNTIL {}", self.child(ptr, "until_expression")));
                }
                head.push(';');
                let mut out = vec![head];
                if !arr(node, "statements").is_empty() {
                    let body = self.list_join(ptr, "statements", "\n");
                    out.push(self.indent(&body));
                }
                for remark in self.format_block_end_remarks(ptr, node) {
                    out.push(self.indent(&remark));
                }
                out.push("END_REPEAT;".to_string());
                out.join("\n")
            }
            "Return" => {
                let expr = if obj(node, "expression").is_some() {
                    format!(" ({})", self.child(ptr, "expression"))
                } else {
                    String::new()
                };
                format!("RETURN{expr};")
            }
            "Skip" => "SKIP;".to_string(),
            _ => String::new(),
        }
    }

    fn format_supertype(&mut self, ptr: &str, node: &Value, t: &str) -> String {
        match t {
            "BinarySupertypeExpression" => {
                let precedence = |operator: &str| match operator {
                    "AND" => 1,
                    "ANDOR" => 2,
                    _ => 0,
                };
                let operator = str_fld(node, "operator").unwrap_or("").to_string();
                // The minimal-paren policy (expressir#651): left child needs
                // parens only when strictly looser; right child when
                // same-or-looser (left-associative operators).
                let wraps = |field: &str, min_precedence: i32| {
                    obj(node, field)
                        .map(|o| {
                            is(o, "SupertypeExpressions::BinarySupertypeExpression")
                                && precedence(str_fld(o, "operator").unwrap_or(""))
                                    >= min_precedence
                        })
                        .unwrap_or(false)
                };
                let op1 = wraps("operand1", precedence(&operator) + 1);
                let op2 = wraps("operand2", precedence(&operator));
                let spelling = operator.as_str();
                let left = if op1 {
                    format!("({})", self.child(ptr, "operand1"))
                } else {
                    self.child(ptr, "operand1")
                };
                let right = if op2 {
                    format!("({})", self.child(ptr, "operand2"))
                } else {
                    self.child(ptr, "operand2")
                };
                format!("{left} {spelling} {right}")
            }
            "OneofSupertypeExpression" => {
                let operands = self.list_join(ptr, "operands", ", ");
                format!("ONEOF({operands})")
            }
            _ => String::new(),
        }
    }

    // ---- remarks ----

    fn remark_target(&self, ptr: &str, node: &Value) -> String {
        node_path(self.ctx, ptr)
            .or_else(|| id_of(node).map(str::to_string))
            .unwrap_or_default()
    }

    fn format_tagged_remark(&self, ptr: &str, node: &Value, remark: &str) -> String {
        let target = self.remark_target(ptr, node);
        if remark.contains('\n') {
            format!("(*\"{target}\"\n{remark}*)")
        } else if node_path(self.ctx, ptr).is_none() && id_of(node).unwrap_or("").contains("IP") {
            format!("--{} {}", id_of(node).unwrap_or_default(), remark)
        } else {
            format!("--\"{target}\" {remark}")
        }
    }

    fn format_untagged_remark(&self, remark: &Remark) -> String {
        let Some(text) = remark.text.filter(|t| !t.is_empty()) else {
            return String::new();
        };
        let formatted = if remark.tagged() {
            format!("\"{}\" {text}", remark.tag.unwrap_or_default())
        } else {
            text.to_string()
        };
        // A multi-line remark always renders as a block: the `--` /
        // `---` tail forms would leak its continuation lines into the
        // source as bare text the parser rejects (#691).
        if text.contains('\n') {
            if remark.embedded_annotation() {
                format!("(**\n{formatted}\n**)")
            } else {
                format!("(*\n{formatted}\n*)")
            }
        } else if remark.tail_annotation() {
            format!("--- {formatted}")
        } else if remark.tail() {
            format!("-- {formatted}")
        } else if remark.embedded_annotation() {
            format!("(** {formatted} *)")
        } else {
            format!("(* {formatted} *)")
        }
    }

    fn format_inline_tail_remark(&self, node: &Value) -> String {
        if self.no_remarks && !self.keep_untagged_remarks {
            return String::new();
        }
        let remark = arr(node, "untagged_remarks").first().map(Remark::from);
        let Some(remark) = remark else {
            return String::new();
        };
        // Placement-carrying remarks render through their own paths
        // (leading above the node, inline after the statement text);
        // this appends the legacy tail.
        if remark.placement.is_some() {
            return String::new();
        }
        let Some(text) = remark.text.filter(|t| !t.is_empty()) else {
            return String::new();
        };
        let formatted = if remark.tagged() {
            format!("\"{}\" {text}", remark.tag.unwrap_or_default())
        } else {
            text.to_string()
        };
        if text.contains('\n') {
            // A multi-line remark cannot ride the `--` tail forms: its
            // continuation lines would leak as bare source text (#691).
            // The block form keeps them inside a remark opener.
            if remark.embedded_annotation() {
                format!(" (**\n{formatted}\n**)")
            } else {
                format!(" (*\n{formatted}\n*)")
            }
        } else if remark.tail_annotation() {
            format!(" --- {formatted}")
        } else if remark.tail() {
            format!(" -- {formatted}")
        } else if remark.embedded_annotation() {
            format!(" (** {formatted} *)")
        } else {
            format!(" (* {formatted} *)")
        }
    }

    fn format_end_scope_remark(&self, node: &Value) -> String {
        if self.no_remarks && !self.keep_untagged_remarks {
            return String::new();
        }
        // Every legacy (unplaced) tail remark belongs after the END_*
        // keyword — writing only the last dropped the others (the WG12
        // `-- IP1:` markers). Remarks carrying an explicit placement
        // render through their placement-aware emitters.
        let lines: Vec<String> = arr(node, "untagged_remarks")
            .iter()
            .map(Remark::from)
            .filter(|r| r.placement.is_none() && r.tail_like())
            .filter_map(|r| {
                let text = r.text?;
                if text.is_empty() || text.contains('\n') {
                    // A multi-line text cannot ride the tail forms (#691).
                    return None;
                }
                let formatted = if r.tagged() {
                    format!("\"{}\" {text}", r.tag.unwrap_or_default())
                } else {
                    text.to_string()
                };
                Some(if r.tail_annotation() {
                    format!("--- {formatted}")
                } else {
                    format!("-- {formatted}")
                })
            })
            .collect();
        if lines.is_empty() {
            String::new()
        } else {
            // The first remark rides the END_ line; the rest follow
            // below it.
            format!(" {}", lines.join("\n"))
        }
    }

    fn format_preamble_remarks(&self, ptr: &str, indent_str: &str) -> Option<String> {
        if self.no_remarks && !self.keep_untagged_remarks {
            return None;
        }
        let node = self.node(ptr);
        let mut remarks: Vec<Remark> = arr(node, "untagged_remarks")
            .iter()
            .map(Remark::from)
            // Leading remarks render above the node (place_remarks) and
            // every legacy tail remark renders after the END_ keyword;
            // emitting either in the preamble would write them twice.
            .filter(|r| !r.leading())
            // Trailing remarks render before the closing keyword.
            .filter(|r| r.placement != Some("trailing"))
            .filter(|r| !(r.placement.is_none() && r.tail_like()))
            .filter(|r| r.text.map(|t| !t.is_empty()).unwrap_or(false))
            .collect();
        if remarks.is_empty() {
            return None;
        }
        let is_scope_container = matches!(
            tail(node),
            "Schema" | "Entity" | "Type" | "Function" | "Procedure" | "Rule"
        );
        if is_scope_container && remarks.last().unwrap().tail_like() {
            remarks.pop();
        }
        if remarks.is_empty() {
            return None;
        }
        Some(
            remarks
                .iter()
                .map(|remark| self.format_preamble_remark(remark, indent_str))
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }

    fn format_preamble_remark(&self, remark: &Remark, indent_str: &str) -> String {
        let text = remark.text.unwrap_or_default();
        let text = if remark.tagged() {
            format!("\"{}\" {text}", remark.tag.unwrap_or_default())
        } else {
            text.to_string()
        };
        if remark.tail_annotation() {
            format!("{indent_str}--- {text}")
        } else if remark.tail() {
            format!("{indent_str}-- {text}")
        } else if remark.embedded_annotation() && text.contains('\n') {
            let lines: Vec<String> = text
                .split('\n')
                .map(|line| format!("{indent_str}  {}", line.trim()))
                .collect();
            format!("{indent_str}(**\n{}\n{indent_str}**)", lines.join("\n"))
        } else if remark.embedded_annotation() {
            format!("{indent_str}(** {text} *)")
        } else if text.contains('\n') {
            let lines: Vec<String> = text
                .split('\n')
                .map(|line| format!("{indent_str}  {}", line.trim()))
                .collect();
            format!("{indent_str}(*\n{}\n{indent_str}*)", lines.join("\n"))
        } else {
            format!("{indent_str}(* {text} *)")
        }
    }

    fn format_trailing_region_remarks(&self, node: &Value, region: &str) -> Vec<String> {
        if self.no_remarks && !self.keep_untagged_remarks {
            return Vec::new();
        }
        arr(node, "untagged_remarks")
            .iter()
            .map(Remark::from)
            .filter(|r| r.trailing_region(region))
            .map(|r| self.format_untagged_remark(&r))
            .filter(|t| !t.is_empty())
            .collect()
    }

    // A remark trailing a bare case label (`1 : -- why`, the action on the
    // next line) is owned by the CASE with the label region; it belongs
    // back on the label line of the action it precedes (#693). Matched by
    // offset when the remark carries one; otherwise by order — the k-th
    // label remark precedes the k-th action.
    fn case_label_remark(&self, ptr: &str) -> String {
        if self.no_remarks && !self.keep_untagged_remarks {
            return String::new();
        }
        let Some((case_ptr, action_index)) = ptr
            .rsplit_once("/actions/")
            .map(|(case, idx)| (case.to_string(), idx.parse::<usize>().ok()))
            .map(|(case, idx)| (case, idx.unwrap_or(usize::MAX)))
        else {
            return String::new();
        };
        let case_node = self.node(&case_ptr);
        let label_remarks: Vec<&Value> = arr(case_node, "untagged_remarks")
            .iter()
            .filter(|r| {
                r.get("placement").and_then(|p| p.as_str()) == Some("inline")
                    && r.get("region").and_then(|g| g.as_str()) == Some("label")
            })
            .collect();
        if label_remarks.is_empty() {
            return String::new();
        }
        let remark = if label_remarks
            .first()
            .and_then(|r| r.get("source_offset"))
            .is_some()
        {
            let anchor = self
                .node(ptr)
                .get("labels")
                .and_then(|l| l.as_array())
                .and_then(|l| l.first())
                .and_then(|l| l.get("source_offset"))
                .and_then(|o| o.as_u64());
            let Some(anchor) = anchor else {
                return String::new();
            };
            let bound = (action_index + 1..)
                .map(|i| self.node(&format!("{case_ptr}/actions/{i}")))
                .take_while(|n| !n.is_null())
                .filter_map(|n| {
                    n.get("labels")
                        .and_then(|l| l.as_array())
                        .and_then(|l| l.first())
                        .and_then(|l| l.get("source_offset"))
                        .and_then(|o| o.as_u64())
                })
                .min();
            match label_remarks
                .iter()
                .find(|r| {
                    r.get("source_offset")
                        .and_then(|o| o.as_u64())
                        .is_some_and(|o| o >= anchor && bound.is_none_or(|b| o < b))
                })
                .or_else(|| label_remarks.first())
            {
                Some(r) => *r,
                None => return String::new(),
            }
        } else {
            match label_remarks.get(action_index) {
                Some(r) => *r,
                None => return String::new(),
            }
        };
        let formatted = self.format_untagged_remark(&Remark::from(remark));
        if formatted.is_empty() {
            String::new()
        } else {
            format!(" {formatted}")
        }
    }

    fn format_mid_keyword_remarks(&self, node: &Value, region: &str) -> String {
        self.formatted_inline_remarks(node, &|r: &Remark| r.inline_region(region))
    }

    fn formatted_inline_remarks(&self, node: &Value, accept: &dyn Fn(&Remark) -> bool) -> String {
        if self.no_remarks && !self.keep_untagged_remarks {
            return String::new();
        }
        arr(node, "untagged_remarks")
            .iter()
            .map(Remark::from)
            .filter(|r| r.inline() && accept(r))
            .map(|r| self.format_untagged_remark(&r))
            .filter(|t| !t.is_empty())
            .map(|t| format!(" {t}"))
            .collect::<Vec<_>>()
            .join("")
    }

    fn format_leading_statement_remarks(&self, node: &Value) -> Vec<String> {
        if self.no_remarks && !self.keep_untagged_remarks {
            return Vec::new();
        }
        arr(node, "untagged_remarks")
            .iter()
            .map(Remark::from)
            .filter(|r| r.leading() && r.region != Some("before_label"))
            .map(|r| self.format_untagged_remark(&r))
            .filter(|t| !t.is_empty())
            .collect()
    }

    /// The leading remarks of a CASE action's statement written before the
    /// label of the action; the CASE action writes them above the label.
    fn format_before_label_remarks(&self, node: &Value) -> Vec<String> {
        if self.no_remarks && !self.keep_untagged_remarks {
            return Vec::new();
        }
        arr(node, "untagged_remarks")
            .iter()
            .map(Remark::from)
            .filter(|r| r.leading() && r.region == Some("before_label"))
            .map(|r| self.format_untagged_remark(&r))
            .filter(|t| !t.is_empty())
            .collect()
    }

    /// Block-end emission for ALIAS/REPEAT bodies: tagged remarks are
    /// stored as bare text in node.remarks alongside a mirror of every
    /// untagged text (add_remark dual-store), so "genuinely tagged" is
    /// derivable only by multiset-subtracting the untagged texts.
    fn format_block_end_remarks(&mut self, ptr: &str, node: &Value) -> Vec<String> {
        if self.no_remarks && !self.keep_untagged_remarks {
            return Vec::new();
        }
        let untagged: Vec<Remark> = arr(node, "untagged_remarks")
            .iter()
            .map(Remark::from)
            .collect();
        let mut remaining: HashMap<String, i32> = HashMap::new();
        for r in &untagged {
            *remaining
                .entry(r.text.unwrap_or_default().to_string())
                .or_insert(0) += 1;
        }
        let tagged: Vec<String> = arr(node, "remarks")
            .iter()
            .filter_map(|r| r.as_str())
            // Ruby's `reject { consume if remaining positive }` — a
            // text is genuinely tagged when the untagged mirror does
            // NOT cover it.
            .filter(|text| {
                let count = remaining.entry(text.to_string()).or_insert(0);
                if *count > 0 {
                    *count -= 1;
                    false
                } else {
                    true
                }
            })
            .map(|text| self.format_tagged_remark(ptr, node, text))
            .collect();
        let rest: Vec<String> = untagged
            .iter()
            .filter(|r| !r.leading() && !r.inline())
            .map(|r| self.format_untagged_remark(r))
            .filter(|t| !t.is_empty())
            .collect();
        let mut out = tagged;
        out.extend(rest);
        out
    }

    fn format_remarks(&mut self, ptr: &str, node: &Value) -> Vec<String> {
        let mut out = Vec::new();
        if !self.no_remarks {
            for text in arr(node, "remarks").iter().filter_map(|r| r.as_str()) {
                out.push(self.format_tagged_remark(ptr, node, text));
            }
            let t = tail(node);
            let skip_untagged = matches!(
                t,
                "Attribute"
                    | "DerivedAttribute"
                    | "InverseAttribute"
                    | "EnumerationItem"
                    | "Schema"
                    | "Entity"
                    | "Type"
                    | "Function"
                    | "Procedure"
                    | "Rule"
            );
            if !skip_untagged {
                for remark in arr(node, "untagged_remarks").iter() {
                    out.push(self.format_untagged_remark(&Remark::from(remark)));
                }
            }
        }
        out
    }

    // ---- placement glue (Formatter#format's place_remarks) ----

    fn place_remarks(&mut self, mut formatted: String, ptr: &str, node: &Value) -> String {
        // An item of an ENUMERATION or of a SELECT is followed by a
        // comma or by the `)` and `;` of its TYPE: its list and its TYPE
        // write the remark after those (`indent_items`, `format_type`).
        let inline = if is_list_item(node) {
            String::new()
        } else {
            self.formatted_inline_remarks(node, &|r: &Remark| r.region.is_none())
        };
        if !inline.is_empty() {
            formatted.push_str(&inline);
        }
        let opener = self.formatted_inline_remarks(node, &|r: &Remark| r.opener());
        if !opener.is_empty() {
            formatted = match formatted.split_once('\n') {
                // An empty first line takes the remark as a line of its own.
                Some(("", rest)) => format!("{}\n{rest}", opener.trim_start()),
                Some((head, rest)) => format!("{head}{opener}\n{rest}"),
                None => format!("{formatted}{opener}"),
            };
        }
        // The owner's own tagged descriptions (`(*"path" text *)`, the
        // #679 storage contract) render as blocks after the declaration
        // (#694) — every owner class except the SCHEMA (its own emitter
        // renders them inside the body) and the EnumerationItem (inline
        // in ENUMERATION OF; the TYPE renders its items).
        if !self.no_remarks && !is_owner_tagged_skip(node) {
            for block in self.owner_tagged_blocks(ptr) {
                formatted.push_str("\n\n");
                formatted.push_str(&block);
            }
        }
        let leading = self.format_leading_statement_remarks(node);
        if leading.is_empty() {
            formatted
        } else {
            let mut lines = leading;
            lines.push(formatted);
            lines.join("\n")
        }
    }

    // The owner's tagged remark texts (`remarks` string array) as WG12
    // blocks tagged with the node's schema-relative path — the same
    // blocks the RemarkItem emitter uses.
    fn owner_tagged_blocks(&self, ptr: &str) -> Vec<String> {
        let tag = node_path(self.ctx, ptr).unwrap_or_default();
        if tag.is_empty() {
            // A node whose path does not resolve renders its texts
            // through the remark-item channel instead — an empty tag
            // would write a `(*""` block.
            return Vec::new();
        }
        arr(self.node(ptr), "remarks")
            .iter()
            .filter_map(|r| r.as_str())
            .filter(|r| !r.is_empty())
            .map(|r| {
                if r.contains('\n') {
                    format!("(*\"{tag}\"\n{r}\n*)")
                } else {
                    format!("(*\"{tag}\" {r} *)")
                }
            })
            .collect()
    }
}

// Owners whose tagged descriptions another emitter renders: the SCHEMA
// carries its own inside the body, the EnumerationItem's belong to its
// TYPE, and the RemarkItem renders itself (#694).
fn is_owner_tagged_skip(node: &Value) -> bool {
    is(node, "Schema") || is(node, "EnumerationItem") || is(node, "RemarkItem")
}

// ---------------------------------------------------------------------
// The PrettyFormatter (issue #637): the ELF pretty-printer — a port of
// Expressir::Express::PrettyFormatter. It reuses the source Formatter
// above (dispatch, grouped runs, the remark emitters) and applies the
// pretty overrides: multi-line entity headers, aligned constant
// blocks, wrapped function/procedure parameter lists, scope bodies,
// the file preamble + provenance block, and the configured indent
// width.
// ---------------------------------------------------------------------

/// The ruby PrettyFormatter::Configuration (defaults included).
#[derive(Clone)]
pub struct PrettyOptions {
    pub indent: usize,
    pub line_length: Option<u64>,
    pub provenance_enabled: bool,
    pub provenance_name: String,
    pub provenance_version: String,
    pub no_remarks: bool,
    pub keep_untagged_remarks: bool,
}

impl Default for PrettyOptions {
    fn default() -> Self {
        PrettyOptions {
            indent: 4,
            line_length: None,
            provenance_enabled: true,
            provenance_name: "Expressir".to_string(),
            provenance_version: String::new(),
            no_remarks: false,
            keep_untagged_remarks: false,
        }
    }
}

impl PrettyOptions {
    fn from_json(wire: &Value) -> Result<PrettyOptions, String> {
        let mut options = PrettyOptions::default();
        if let Some(indent) = wire.get("indent").and_then(Value::as_u64) {
            options.indent = indent as usize;
        }
        if let Some(length) = wire.get("line_length") {
            options.line_length = length.as_u64();
        }
        if let Some(provenance) = wire.get("provenance").and_then(Value::as_bool) {
            options.provenance_enabled = provenance;
        }
        if let Some(name) = wire.get("provenance_name").and_then(Value::as_str) {
            options.provenance_name = name.to_string();
        }
        if let Some(version) = wire.get("provenance_version").and_then(Value::as_str) {
            options.provenance_version = version.to_string();
        }
        if let Some(no_remarks) = wire.get("no_remarks").and_then(Value::as_bool) {
            options.no_remarks = no_remarks;
        }
        Ok(options)
    }
}

/// The delegation entry: the model-hash wire + a node path ("" = the
/// ExpFile root) + the options JSON in, the pretty text out.
pub fn pretty_format(root: &Value, path: &str, options_json: &str) -> Result<String, String> {
    let options = PrettyOptions::from_json(
        &serde_json::from_str(options_json).map_err(|e| format!("options json: {e}"))?,
    )?;
    let ctx = PathCtx::new(root);
    let mut f = Formatter {
        ctx: &ctx,
        no_remarks: options.no_remarks,
        keep_untagged_remarks: options.keep_untagged_remarks,
        width: options.indent,
        flavor: Flavor::Source,
        head_only: false,
    };
    let pretty = PrettyFormatter { options };
    if path.is_empty() {
        return Ok(pretty.format_file(&mut f));
    }
    let node = f.node(path).clone();
    Ok(match cls(&node).rsplit("::").next().unwrap_or("") {
        "Schema" => pretty.format_schema(&mut f, path, &node),
        "Entity" => pretty.format_entity(&mut f, path, &node),
        "Function" => pretty.format_function(&mut f, path, &node),
        "Procedure" => pretty.format_procedure(&mut f, path, &node),
        "Rule" => pretty.format_rule(&mut f, path, &node),
        _ => f.dispatch(path),
    })
}

struct PrettyFormatter {
    options: PrettyOptions,
}

impl PrettyFormatter {
    fn indent_str(&self) -> String {
        " ".repeat(self.options.indent)
    }

    /// The ruby PrettyFormatter#indent: every line prefixed.
    fn indent(&self, text: &str) -> String {
        let prefix = self.indent_str();
        text.split('\n')
            .map(|line| format!("{prefix}{line}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn provenance(&self) -> Option<String> {
        if !self.options.provenance_enabled {
            return None;
        }
        let mut params = vec![format!("indent: {}", self.options.indent)];
        if let Some(length) = self.options.line_length {
            params.push(format!("line_length: {length}"));
        }
        Some(
            [
                "(*".to_string(),
                format!(
                    "  Generated by: {} version {}",
                    self.options.provenance_name, self.options.provenance_version
                ),
                format!("  Format parameters: {}", params.join(", ")),
                "*)".to_string(),
            ]
            .join("\n"),
        )
    }

    /// The file face (the ruby format_exp_file): preamble, provenance,
    /// schemas joined with a blank line, trailing newline.
    fn format_file(&self, f: &mut Formatter) -> String {
        let mut out: Vec<String> = Vec::new();
        if !f.no_remarks {
            let remarks: Vec<&Value> = arr(f.ctx.root, "untagged_remarks")
                .iter()
                .filter(|remark| !self.provenance_remark(remark))
                .collect();
            if !remarks.is_empty() {
                for remark in remarks {
                    out.push(self.preamble_remark(remark));
                }
                out.push(String::new());
            }
        }
        if let Some(provenance) = self.provenance() {
            out.push(provenance);
            out.push(String::new());
        }
        let schemas = arr(f.ctx.root, "schemas");
        let body = schemas
            .iter()
            .enumerate()
            .map(|(i, _)| {
                let ptr = format!("/schemas/{i}");
                let node = f.node(&ptr).clone();
                self.format_schema(f, &ptr, &node)
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        if !body.is_empty() {
            out.push(body);
        }
        if out.is_empty() {
            String::new()
        } else {
            format!("{}\n", out.join("\n"))
        }
    }

    fn provenance_remark(&self, remark: &Value) -> bool {
        str_fld(remark, "text").is_some_and(|text| {
            text.trim_start().starts_with("Generated by:")
                || text.trim_start().starts_with("Format parameters:")
        })
    }

    /// The single preamble remark (the ruby format_preamble_remark).
    fn preamble_remark(&self, remark: &Value) -> String {
        let Some(text) = str_fld(remark, "text") else {
            return String::new();
        };
        if text.is_empty() {
            return String::new();
        }
        let tagged = str_fld(remark, "tag")
            .filter(|tag| !tag.is_empty())
            .map(|tag| format!("\"{tag}\" {text}"))
            .unwrap_or_else(|| text.to_string());
        if str_fld(remark, "format") == Some("tail") {
            format!("-- {tagged}")
        } else if tagged.contains('\n') {
            ["(*".to_string(), tagged, "*)".to_string()].join("\n")
        } else {
            format!("(* {tagged} *)")
        }
    }

    /// The pretty registry: everything the ruby PrettyFormatter
    /// overrides routes through here — format(x) in the ruby
    /// pretty emitters is the overridden dispatch.
    fn pretty_dispatch(&self, f: &mut Formatter, ptr: &str) -> String {
        let node = f.node(ptr).clone();
        match cls(&node).rsplit("::").next().unwrap_or("") {
            "Schema" => self.format_schema(f, ptr, &node),
            "Entity" => self.format_entity(f, ptr, &node),
            "Function" => self.format_function(f, ptr, &node),
            "Procedure" => self.format_procedure(f, ptr, &node),
            "Rule" => self.format_rule(f, ptr, &node),
            _ => f.dispatch(ptr),
        }
    }

    fn schema_declarations(&self, f: &mut Formatter, ptr: &str, node: &Value) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let constants = arr(node, "constants");
        if !constants.is_empty() {
            let count = constants.len();
            let ptrs: Vec<String> = (0..count).map(|i| format!("{ptr}/constants/{i}")).collect();
            out.push(
                [
                    "CONSTANT".to_string(),
                    self.indent(&self.constant_block(f, &ptrs)),
                    "END_CONSTANT;".to_string(),
                ]
                .join("\n"),
            );
        }
        for field in [
            "types",
            "entities",
            "subtype_constraints",
            "functions",
            "rules",
            "procedures",
        ] {
            let items = arr(node, field);
            for i in 0..items.len() {
                out.push(self.pretty_dispatch(f, &format!("{ptr}/{field}/{i}")));
            }
        }
        out
    }

    /// The aligned constant block (the ruby format_constant_block).
    fn constant_block(&self, f: &mut Formatter, ptrs: &[String]) -> String {
        let names: Vec<&str> = ptrs
            .iter()
            .map(|p| id_of(f.node(p)).unwrap_or_default())
            .collect();
        let types: Vec<String> = ptrs.iter().map(|p| f.child(p, "type")).collect();
        let name_width = names.iter().map(|n| n.len()).max().unwrap_or(0);
        let type_width = types.iter().map(|t| t.len()).max().unwrap_or(0);
        ptrs.iter()
            .enumerate()
            .map(|(i, p)| {
                let name_pad = " ".repeat(name_width - names[i].len());
                let type_pad = " ".repeat(type_width - types[i].len());
                // A comment on a line of its own above the constant leads it.
                let constant = format!(
                    "{}{} : {}{} := {};",
                    names[i],
                    name_pad,
                    types[i],
                    type_pad,
                    f.child(p, "expression")
                );
                let mut lines = f.format_leading_statement_remarks(f.node(p));
                lines.push(constant);
                lines.join("\n")
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn format_schema(&self, f: &mut Formatter, ptr: &str, node: &Value) -> String {
        let indent_str = self.indent_str();
        let preamble = f.format_preamble_remarks(ptr, &indent_str);
        let declarations = self.schema_declarations(f, ptr, node);
        let mut out = vec![f.format_schema_head(ptr, node)];
        if let Some(preamble) = preamble.filter(|p| !p.is_empty()) {
            out.push(String::new());
            out.push(preamble.trim_end().to_string());
        }
        if !declarations.is_empty() {
            out.push(String::new());
            out.push(declarations.join("\n\n"));
            out.push(String::new());
        }
        let closing = f.format_trailing_region_remarks(node, "declarations");
        if !closing.is_empty() && declarations.is_empty() {
            out.push(String::new());
        }
        out.extend(closing);
        out.push(format!("END_SCHEMA;{}", f.format_end_scope_remark(node)));
        out.join("\n")
    }

    /// Attribute items of an entity: explicit, then derived, then
    /// inverse (the ruby entity_attribute_items selection by kind).
    fn attribute_sections(
        &self,
        f: &mut Formatter,
        ptr: &str,
        node: &Value,
    ) -> (Vec<String>, Vec<String>, Vec<String>) {
        let attributes = arr(node, "attributes");
        let count = attributes.len();
        let mut explicit: Vec<String> = Vec::new();
        let mut derived: Vec<String> = Vec::new();
        let mut inverse: Vec<String> = Vec::new();
        for i in 0..count {
            let child = format!("{ptr}/attributes/{i}");
            let kind = str_fld(f.node(&child), "kind").unwrap_or("EXPLICIT");
            match kind {
                "DERIVED" => derived.push(f.dispatch(&child)),
                "INVERSE" => inverse.push(f.dispatch(&child)),
                _ => explicit.push(child),
            }
        }
        let explicit_rendered = if explicit.is_empty() {
            Vec::new()
        } else {
            vec![self.indent(&f.join_grouped(&explicit, "\n", true))]
        };
        (explicit_rendered, derived, inverse)
    }

    fn format_entity(&self, f: &mut Formatter, ptr: &str, node: &Value) -> String {
        let (explicit, derived, inverse) = self.attribute_sections(f, ptr, node);
        let indent_str = self.indent_str();
        let mut header = format!("ENTITY {}", id_of(node).unwrap_or_default());
        let abstract_flag = node.get("abstract").and_then(Value::as_bool) == Some(true);
        let supertype = obj(node, "supertype_expression");
        if abstract_flag && supertype.is_none() {
            header.push_str(&format!("\n{}ABSTRACT SUPERTYPE", indent_str));
        } else if abstract_flag {
            header.push_str(&format!(
                "\n{}ABSTRACT SUPERTYPE OF ({})",
                indent_str,
                f.child(ptr, "supertype_expression")
            ));
        } else if supertype.is_some() {
            header.push_str(&format!(
                "\n{}SUPERTYPE OF ({})",
                indent_str,
                f.child(ptr, "supertype_expression")
            ));
        }
        let subtype_of = arr(node, "subtype_of");
        if !subtype_of.is_empty() {
            let refs: Vec<String> = (0..subtype_of.len())
                .map(|i| f.child(ptr, &format!("subtype_of/{i}")))
                .collect();
            header.push_str(&format!("\n{}SUBTYPE OF ({})", indent_str, refs.join(", ")));
        }
        header.push(';');
        let preamble = f.format_preamble_remarks(ptr, &indent_str);
        let mut out = vec![header];
        if let Some(preamble) = preamble.filter(|p| !p.is_empty()) {
            out.push(preamble.trim_end().to_string());
        }
        out.extend(explicit);
        for remark in f.format_trailing_region_remarks(node, "attributes") {
            out.push(self.indent(&remark));
        }
        for (keyword, rendered, region) in [
            ("DERIVE", &derived, "derived_attributes"),
            ("INVERSE", &inverse, "inverse_attributes"),
        ] {
            if !rendered.is_empty() {
                out.push(keyword.to_string());
                out.push(self.indent(&rendered.join("\n")));
            }
            for remark in f.format_trailing_region_remarks(node, region) {
                out.push(self.indent(&remark));
            }
        }
        for (field, keyword) in [("unique_rules", "UNIQUE"), ("where_rules", "WHERE")] {
            let items = arr(node, field);
            if !items.is_empty() {
                let rendered: Vec<String> = (0..items.len())
                    .map(|i| f.dispatch(&format!("{ptr}/{field}/{i}")))
                    .collect();
                out.push(keyword.to_string());
                out.push(self.indent(&rendered.join("\n")));
            }
            for remark in f.format_trailing_region_remarks(node, field) {
                out.push(self.indent(&remark));
            }
        }
        out.push(format!("END_ENTITY;{}", f.format_end_scope_remark(node)));
        out.join("\n")
    }

    /// The shared scope body (types, entities, subtype constraints,
    /// nested functions/procedures, constants, locals, statements).
    fn scope_body(
        &self,
        f: &mut Formatter,
        ptr: &str,
        node: &Value,
        tail_region: &str,
    ) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for field in [
            "types",
            "entities",
            "subtype_constraints",
            "functions",
            "procedures",
        ] {
            let items = arr(node, field);
            if !items.is_empty() {
                let rendered: Vec<String> = (0..items.len())
                    .map(|i| {
                        let item = format!("{ptr}/{field}/{i}");
                        // The pretty emitters of an entity, a function and a
                        // procedure write no leading remark; the others go
                        // through dispatch, which does.
                        let mut lines = match field {
                            "entities" | "functions" | "procedures" => {
                                f.format_leading_statement_remarks(f.node(&item))
                            }
                            _ => Vec::new(),
                        };
                        lines.push(self.pretty_dispatch(f, &item));
                        lines.join("\n")
                    })
                    .collect();
                out.push(self.indent(&rendered.join("\n")));
            }
        }
        let constants = arr(node, "constants");
        if !constants.is_empty() {
            let count = constants.len();
            let ptrs: Vec<String> = (0..count).map(|i| format!("{ptr}/constants/{i}")).collect();
            out.push(
                self.indent(
                    &[
                        "CONSTANT".to_string(),
                        self.indent(&self.constant_block(f, &ptrs)),
                        "END_CONSTANT;".to_string(),
                    ]
                    .join("\n"),
                ),
            );
        }
        // A comment on its own line before LOCAL closes the declarations.
        for remark in f.format_trailing_region_remarks(node, "declarations") {
            out.push(self.indent(&remark));
        }
        let variables = arr(node, "variables");
        if !variables.is_empty() {
            let count = variables.len();
            let ptrs: Vec<String> = (0..count).map(|i| format!("{ptr}/variables/{i}")).collect();
            let mut inner = f.join_grouped(&ptrs, "\n", false);
            for remark in f.format_trailing_region_remarks(node, "variables") {
                inner.push('\n');
                inner.push_str(&remark);
            }
            out.push(
                self.indent(
                    &[
                        "LOCAL".to_string(),
                        self.indent(&inner),
                        "END_LOCAL;".to_string(),
                    ]
                    .join("\n"),
                ),
            );
        }
        let statements = arr(node, "statements");
        if !statements.is_empty() {
            let rendered: Vec<String> = (0..statements.len())
                .map(|i| f.dispatch(&format!("{ptr}/statements/{i}")))
                .collect();
            out.push(self.indent(&rendered.join("\n")));
        }
        for remark in f.format_trailing_region_remarks(node, tail_region) {
            out.push(self.indent(&remark));
        }
        out
    }

    fn parameter_list(&self, f: &mut Formatter, ptr: &str, node: &Value) -> String {
        let parameters = arr(node, "parameters");
        let count = parameters.len();
        let ptrs: Vec<String> = (0..count)
            .map(|i| format!("{ptr}/parameters/{i}"))
            .collect();
        let id = id_of(node).unwrap_or_default();
        let keyword = cls(node).rsplit("::").next().unwrap_or("").to_uppercase();
        let pad = " ".repeat(format!("{keyword} {id}(").len());
        f.join_grouped(&ptrs, &format!(";\n{pad}"), false)
    }

    fn format_function(&self, f: &mut Formatter, ptr: &str, node: &Value) -> String {
        let id = id_of(node).unwrap_or_default();
        let mut header = format!("FUNCTION {id}");
        if !arr(node, "parameters").is_empty() {
            header.push_str(&format!("({})", self.parameter_list(f, ptr, node)));
        }
        header.push_str(&format!(" : {};", f.child(ptr, "return_type")));
        let mut out = vec![header];
        out.extend(self.scope_body(f, ptr, node, "statements"));
        out.push(format!("END_FUNCTION;{}", f.format_end_scope_remark(node)));
        out.join("\n")
    }

    fn format_procedure(&self, f: &mut Formatter, ptr: &str, node: &Value) -> String {
        let id = id_of(node).unwrap_or_default();
        let mut header = format!("PROCEDURE {id}");
        if !arr(node, "parameters").is_empty() {
            header.push_str(&format!("({})", self.parameter_list(f, ptr, node)));
        }
        header.push(';');
        let mut out = vec![header];
        out.extend(self.scope_body(f, ptr, node, "statements"));
        out.push(format!("END_PROCEDURE;{}", f.format_end_scope_remark(node)));
        out.join("\n")
    }

    fn format_rule(&self, f: &mut Formatter, ptr: &str, node: &Value) -> String {
        let id = id_of(node).unwrap_or_default();
        let applies: Vec<String> = (0..arr(node, "applies_to").len())
            .map(|i| f.child(ptr, &format!("applies_to/{i}")))
            .collect();
        let mut out = vec![format!("RULE {id} FOR ({});", applies.join(", "))];
        out.extend(self.scope_body(f, ptr, node, "statements"));
        let where_rules = arr(node, "where_rules");
        if !where_rules.is_empty() {
            let rendered: Vec<String> = (0..where_rules.len())
                .map(|i| f.dispatch(&format!("{ptr}/where_rules/{i}")))
                .collect();
            out.push("WHERE".to_string());
            out.push(self.indent(&rendered.join("\n")));
        }
        for remark in f.format_trailing_region_remarks(node, "where_rules") {
            out.push(self.indent(&remark));
        }
        out.push(format!("END_RULE;{}", f.format_end_scope_remark(node)));
        out.join("\n")
    }
}
