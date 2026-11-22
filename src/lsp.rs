//! The LSP document-symbols engine (#646 PR-1): a port of the ruby
//! `Document#symbols` walk (NodePositionIndex semantics) over a native
//! parse of the document text. The delegation hands back rows in BYTE
//! terms — 1-based span lines plus the selection byte window — and the
//! ruby Document converts them through its own position math (LSP
//! UTF-16 positions stay transport-side). The parity gate compares the
//! rows against the ruby walk; the differential caught nothing so far,
//! which is the point of keeping it.

use serde_json::Value;

/// The ruby Document::SYMBOL_KINDS class → LSP symbol kind.
fn symbol_kind(class: &str) -> Option<u8> {
    match class {
        "Expressir::Model::Declarations::Schema" => Some(3),
        "Expressir::Model::Declarations::Entity" => Some(5),
        "Expressir::Model::Declarations::Type" => Some(26),
        "Expressir::Model::Declarations::Function" => Some(12),
        "Expressir::Model::Declarations::Procedure" => Some(6),
        "Expressir::Model::Declarations::Rule" => Some(11),
        "Expressir::Model::Declarations::Constant" => Some(14),
        "Expressir::Model::Declarations::Attribute" => Some(8),
        "Expressir::Model::Declarations::Variable" => Some(13),
        _ => None,
    }
}

fn class_of(node: &Value) -> &str {
    node.get("_class").and_then(Value::as_str).unwrap_or("")
}

/// The ruby LineMap: 1-based line numbers over byte offsets.
struct LineMap {
    starts: Vec<usize>,
}

impl LineMap {
    fn new(bytes: &[u8]) -> Self {
        let mut starts = vec![0usize];
        let mut index = 0usize;
        while let Some(found) = bytes[index..].iter().position(|b| *b == b'\n') {
            index += found + 1;
            starts.push(index);
        }
        LineMap { starts }
    }

    fn line_number(&self, byte: usize) -> usize {
        match self.starts.binary_search(&byte) {
            Ok(i) => i + 1,
            Err(i) => i,
        }
    }
}

/// One positioned node in DFS order, mirroring the ruby entry shape.
struct Entry {
    class: String,
    id: Option<String>,
    is_attribute: bool,
    is_scope: bool,
    valid: bool,
    start: u64,
    end: u64,
    end_line: usize,
    parent: Option<usize>,
}

/// The declaration-level leaves the ruby grammar lane positions with
/// named spans — the children whose ends raise their scope (items,
/// references and data-type captures are not positioned there and
/// never raise).
fn is_raiser_class(class: &str) -> bool {
    is_scope_class(class)
        || matches!(
            class,
            "Expressir::Model::Declarations::Attribute"
                | "Expressir::Model::Declarations::WhereRule"
                | "Expressir::Model::Declarations::UniqueRule"
                | "Expressir::Model::Declarations::InformalPropositionRule"
                | "Expressir::Model::Declarations::Parameter"
                | "Expressir::Model::Declarations::Variable"
        )
}

/// The classes whose declarations own an END keyword — the scope
/// containers the ruby propagation exists to fix.
fn is_scope_class(class: &str) -> bool {
    matches!(
        class,
        "Expressir::Model::Declarations::Schema"
            | "Expressir::Model::Declarations::Entity"
            | "Expressir::Model::Declarations::Type"
            | "Expressir::Model::Declarations::Function"
            | "Expressir::Model::Declarations::Procedure"
            | "Expressir::Model::Declarations::Rule"
            | "Expressir::Model::Declarations::Constant"
            | "Expressir::Model::Declarations::Variable"
    )
}

/// The ruby position_valid?: byte-0 starts are only accepted for
/// declaration-keyword-leading slices.
fn position_valid(source: &str) -> bool {
    const KEYWORDS: &[&str] = &[
        "SCHEMA",
        "ENTITY",
        "TYPE",
        "FUNCTION",
        "PROCEDURE",
        "RULE",
        "CONSTANT",
        "VARIABLE",
        "USE",
        "REFERENCE",
        "END_SCHEMA",
        "END_ENTITY",
        "END_TYPE",
        "END_FUNCTION",
        "END_PROCEDURE",
        "END_RULE",
        "END_CONSTANT",
        "END_VARIABLE",
    ];
    KEYWORDS.iter().any(|keyword| source.starts_with(keyword))
}

/// The ruby name_offset: search a 1024-byte window from the node's
/// offset for `id` with word boundaries, case-insensitive; the match
/// position is the WINDOW's character index (the ruby regex runs on a
/// UTF-8 slice), mirrored exactly.
pub(crate) fn name_offset(window: &[u8], offset: usize, id: &str) -> Option<usize> {
    let id_bytes = id.as_bytes();
    if id_bytes.is_empty() || id_bytes.len() > window.len() {
        return None;
    }
    let boundary = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    for start in 0..=(window.len() - id_bytes.len()) {
        if !window[start..start + id_bytes.len()].eq_ignore_ascii_case(id_bytes) {
            continue;
        }
        let before_ok = start == 0 || !boundary(window[start - 1]);
        let end = start + id_bytes.len();
        let after_ok = end == window.len() || !boundary(window[end]);
        if before_ok && after_ok {
            let char_index = String::from_utf8_lossy(&window[..start]).chars().count();
            return Some(offset + char_index);
        }
    }
    None
}

/// The ruby attacher extends every positioned node's span through its
/// own tail remarks' first lines (extend_source_slices, #712); the
/// symbols read spans off the ATTACHED wire, so the pass rides here.
fn extend_spans_through_remarks(node: &mut Value, bytes: &[u8]) {
    let Value::Object(map) = node else { return };
    let start = map.get("source_offset").and_then(Value::as_u64);
    let len = map.get("source_length").and_then(Value::as_u64);
    if let (Some(start), Some(len)) = (start, len) {
        let mut end = start + len;
        if let Some(Value::Array(remarks)) = map.get("untagged_remarks") {
            for remark in remarks {
                let Some(ro) = remark.get("source_offset").and_then(Value::as_u64) else {
                    continue;
                };
                if ro <= start {
                    continue;
                }
                let ro = ro as usize;
                if ro >= bytes.len() {
                    continue;
                }
                let remark_end = match bytes[ro..].iter().position(|&b| b == b'\n') {
                    Some(i) => ro + i + 1,
                    None => bytes.len(),
                };
                end = end.max(remark_end as u64);
            }
        }
        let extended = end - start;
        if extended > len {
            map.insert("source_length".into(), Value::from(extended));
            map.insert("source_end_offset".into(), Value::from(end));
            if (end as usize) <= bytes.len() && (start as usize) <= bytes.len() {
                map.insert(
                    "source".into(),
                    Value::String(
                        String::from_utf8_lossy(&bytes[start as usize..end as usize]).to_string(),
                    ),
                );
            }
        }
    }
    for (_k, v) in map.iter_mut() {
        match v {
            Value::Array(items) => items
                .iter_mut()
                .for_each(|c| extend_spans_through_remarks(c, bytes)),
            Value::Object(_) => extend_spans_through_remarks(v, bytes),
            _ => {}
        }
    }
}

fn walk(node: &Value, parent: Option<usize>, rows: &mut Vec<Entry>) {
    let Value::Object(map) = node else { return };
    let class = class_of(node).to_string();
    // Attached remarks are not symbols and must not raise their owner's
    // span: a tail remark's offset sits past the owner's END_ keyword
    // (#693), and walking it as a child would extend the symbol by a line.
    if class == "Expressir::Model::RemarkInfo" {
        return;
    }
    if !class.is_empty() {
        let start = map.get("source_offset").and_then(Value::as_u64);
        let end = map.get("source_end_offset").and_then(Value::as_u64);
        if let (Some(start), Some(end)) = (start, end) {
            let source = map.get("source").and_then(Value::as_str).unwrap_or("");
            let valid = start > 0 || (!source.is_empty() && position_valid(source));
            rows.push(Entry {
                is_attribute: class == "Expressir::Model::Declarations::Attribute",
                is_scope: is_scope_class(&class),
                id: map.get("id").and_then(Value::as_str).map(str::to_string),
                valid,
                start,
                end,
                end_line: 0,
                class,
                parent,
            });
            let parent_index = Some(rows.len() - 1);
            for value in map.values() {
                walk_value(value, parent_index, rows);
            }
            return;
        }
        let parent_index = if rows.last().is_some() {
            // Unpositioned nodes still parent their subtree to the
            // nearest positioned ancestor for the walk, but the ruby
            // index keeps them line-less; their subtree parents to
            // this node only when it was recorded.
            None
        } else {
            parent
        };
        for value in map.values() {
            walk_value(value, parent_index, rows);
        }
        return;
    }
    for value in map.values() {
        walk_value(value, parent, rows);
    }
}

fn walk_value(value: &Value, parent: Option<usize>, rows: &mut Vec<Entry>) {
    match value {
        Value::Object(_) => walk(value, parent, rows),
        Value::Array(items) => {
            for item in items {
                walk_value(item, parent, rows);
            }
        }
        _ => {}
    }
}

/// The ruby propagate_scope_ends: a reverse DFS pass settling each
/// scope's effective end as the SMALLER of its own capture line (the
/// seed — which can overshoot into the END keyword + trivia on some
/// grammar shapes) and the maximum effective end of its
/// declaration-level descendants (the ruby lane positions only
/// declaration-level nodes — leaf type/reference/item captures carry
/// no offset there and never raise).
fn propagate(entries: &mut [Entry], seeds: &[usize]) {
    // effective[i] settles in reverse DFS order (children before
    // parents), so a nested scope's final end is available when its
    // parent reads it.
    let mut effective: Vec<usize> = seeds.to_vec();
    for i in (0..entries.len()).rev() {
        let child_end = effective[i];
        if let Some(parent) = entries[i].parent {
            if is_raiser_class(&entries[i].class) && child_end > effective[parent] {
                effective[parent] = child_end;
            }
        }
        if entries[i].is_scope && effective[i] > seeds[i] {
            // The capture seed bounds the scope: overshooting
            // descendants do not extend a scope past its own span.
            effective[i] = seeds[i];
        }
        entries[i].end_line = effective[i];
    }
}

/// The ruby `span_length`: the captured length; the wire always
/// carries source_length on positioned nodes, so end = start + length
/// (identical to source_end_offset here, kept explicit for the
/// contract).
pub fn symbols(source: &str) -> Result<Vec<serde_json::Value>, String> {
    let tree = crate::ParsedTree::parse(source).map_err(|e| format!("parse: {e:?}"))?;
    let mut wire = tree
        .to_model_json("document.exp")
        .map_err(|e| format!("wire: {e:?}"))?;
    // The ruby lane's spans come off the ATTACHED model: the remark
    // attacher extends positioned nodes through their tail remarks'
    // first lines (#712), and the branch's legacy-tail attach widens
    // scope spans accordingly. Attach, extend, then measure.
    crate::attach::attach_tagged_remarks(&mut wire, source, false);
    crate::attach::attach_untagged_remarks(&mut wire, source, false);
    extend_spans_through_remarks(&mut wire, source.as_bytes());
    let bytes = source.as_bytes();
    let lines = LineMap::new(bytes);

    let mut entries: Vec<Entry> = Vec::new();
    walk(&wire, None, &mut entries);
    // The ruby grammar lane's capture convention: non-scope nodes
    // carry name/line tokens (the end_line lands on the declaration's
    // first line even when the declaration wraps), and scopes carry a
    // span that ends on their declaration's last line — although some
    // grammar shapes' captures overshoot into the END keyword + trivia
    // the ruby span excludes. Seed from the capture, raise from the
    // declaration-level children, and take the SMALLER of the two for
    // scopes: a scope's true end is its last declaration-level child
    // when the capture overshot, its own span otherwise.
    // The ruby span end = source_offset + source_length, no trim: the
    // same-line tail remark's line-end extension lands the scope one
    // line past its END keyword (extend_source_slices, #712), and the
    // trim ate exactly that line.
    let seeds: Vec<usize> = entries
        .iter()
        .map(|entry| lines.line_number((entry.end as usize).max(entry.start as usize)))
        .collect();
    propagate(&mut entries, &seeds);

    let mut rows: Vec<serde_json::Value> = Vec::new();
    for entry in &entries {
        let Some(kind) = symbol_kind(&entry.class) else {
            continue;
        };
        let Some(id) = entry.id.clone() else {
            continue;
        };
        if !entry.valid {
            continue;
        }
        let line = lines.line_number(entry.start as usize);
        if line == 0 {
            continue;
        }
        let sel_start = if entry.is_attribute {
            entry.start as usize
        } else {
            let window_end = ((entry.start as usize) + 1024).min(bytes.len());
            name_offset(
                &bytes[entry.start as usize..window_end],
                entry.start as usize,
                &id,
            )
            .unwrap_or(entry.start as usize)
        };
        rows.push(serde_json::json!({
            "name": id,
            "kind": kind,
            "span_line": line,
            "span_end_line": entry.end_line,
            "sel_start": sel_start,
            "sel_len": id.len(),
        }));
    }
    // The ruby sorts by position, stable to the walk order.
    rows.sort_by_key(|row| row["sel_start"].as_u64().unwrap_or(u64::MAX));
    Ok(rows)
}

/// The delegation entry: document text in, symbol rows out.
pub fn symbols_json(source: &str) -> Result<String, String> {
    let rows = symbols(source)?;
    serde_json::to_string(&rows).map_err(|e| format!("rows json: {e}"))
}

/// The #646 PR-2 inventory: one native pass over a document producing
/// the rows the LSP navigation runs on — declarations (id, class,
/// selection byte, the fill's dot path) and references (selection
/// byte, id, the resolved base_path). The wire is resolved first
/// (checker::resolve_references — the same fill the delegation uses),
/// so a reference's base_path and the declaration paths come from one
/// machinery and match by construction; the ruby Workspace keeps the
/// cross-document interface pass on top of these rows.
pub fn inventory(source: &str) -> Result<serde_json::Value, String> {
    let tree = crate::ParsedTree::parse(source).map_err(|e| format!("parse: {e:?}"))?;
    let mut wire = tree
        .to_model_json("document.exp")
        .map_err(|e| format!("wire: {e:?}"))?;
    crate::checker::resolve_references(&mut wire);
    let ctx = crate::checker::Ctx::new(&wire);
    let bytes = source.as_bytes();

    let mut declarations: Vec<serde_json::Value> = Vec::new();
    let mut references: Vec<serde_json::Value> = Vec::new();
    walk_inventory(
        &wire,
        String::new(),
        bytes,
        &ctx,
        &mut declarations,
        &mut references,
    );

    Ok(serde_json::json!({
        "declarations": declarations,
        "references": references,
    }))
}

/// The delegation entry: document text in, the inventory JSON out.
pub fn inventory_json(source: &str) -> Result<String, String> {
    let inventory = inventory(source)?;
    serde_json::to_string(&inventory).map_err(|e| format!("inventory json: {e}"))
}

fn walk_inventory(
    node: &Value,
    ptr: String,
    bytes: &[u8],
    ctx: &crate::checker::Ctx<'_>,
    declarations: &mut Vec<serde_json::Value>,
    references: &mut Vec<serde_json::Value>,
) {
    let Value::Object(map) = node else { return };
    let class = class_of(node).to_string();
    if class.ends_with("::SimpleReference") {
        // The ruby walk keeps EVERY reference; one without a capture
        // offset still gets a location through name_offset's window
        // search from byte 0 (source_offset.to_i), though it can never
        // match a token (the nil offset never equals a byte).
        if let Some(id) = map.get("id").and_then(Value::as_str) {
            let sel = match map.get("source_offset").and_then(Value::as_u64) {
                Some(offset) => {
                    let window_end = ((offset as usize) + 1024).min(bytes.len());
                    name_offset(&bytes[offset as usize..window_end], offset as usize, id)
                        .unwrap_or(offset as usize)
                }
                None => name_offset(&bytes[..1024.min(bytes.len())], 0, id).unwrap_or(0),
            };
            references.push(serde_json::json!({
                "sel_start": sel,
                "offset": map.get("source_offset").and_then(Value::as_u64),
                "id": id,
                "base_path": map.get("base_path").and_then(Value::as_str),
            }));
        }
    } else if symbol_kind(&class).is_some() {
        // The ruby walk keeps EVERY id-bearing symbol declaration; one
        // without a capture offset still gets a selection through
        // name_offset's window search from byte 0 (source_offset.to_i).
        if let Some(id) = map.get("id").and_then(Value::as_str) {
            // The ruby Document#name_offset convention: Attribute
            // declarations select from the CAPTURE START verbatim (a
            // redeclared `SELF\parent.attr` selects at SELF, not at
            // the name); everything else searches the id occurrence.
            let sel = if class == "Expressir::Model::Declarations::Attribute" {
                match map.get("source_offset").and_then(Value::as_u64) {
                    Some(offset) => offset as usize,
                    None => 0,
                }
            } else {
                match map.get("source_offset").and_then(Value::as_u64) {
                    Some(offset) => {
                        let window_end = ((offset as usize) + 1024).min(bytes.len());
                        name_offset(&bytes[offset as usize..window_end], offset as usize, id)
                            .unwrap_or(offset as usize)
                    }
                    None => name_offset(&bytes[..1024.min(bytes.len())], 0, id).unwrap_or(0),
                }
            };
            declarations.push(serde_json::json!({
                "id": id,
                "class": class,
                "sel_start": sel,
                "path": ctx.node_path(&ptr),
            }));
        }
    }
    for (key, value) in map {
        if key == "_class" {
            continue;
        }
        walk_value_inventory(
            value,
            &format!("{ptr}/{key}"),
            bytes,
            ctx,
            declarations,
            references,
        );
    }
}

fn walk_value_inventory(
    value: &Value,
    ptr: &str,
    bytes: &[u8],
    ctx: &crate::checker::Ctx<'_>,
    declarations: &mut Vec<serde_json::Value>,
    references: &mut Vec<serde_json::Value>,
) {
    match value {
        Value::Object(_) => {
            walk_inventory(value, ptr.to_string(), bytes, ctx, declarations, references)
        }
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                walk_value_inventory(
                    item,
                    &format!("{ptr}/{i}"),
                    bytes,
                    ctx,
                    declarations,
                    references,
                );
            }
        }
        _ => {}
    }
}

/// The #646 PR-3 rename edits for one document: the positions to
/// replace when the declaration at `path` is renamed — the
/// declaration's own selection plus every resolved reference's — as
/// byte rows. The ruby Workspace orchestrates the per-document calls,
/// adds the cross-document interface pass, and runs the parse
/// verification probes; this engine carries the local match set.
pub fn rename_edits(source: &str, path: &str) -> Result<Vec<serde_json::Value>, String> {
    let inventory = inventory(source)?;
    let mut edits: Vec<serde_json::Value> = Vec::new();
    let mut target_id: Option<String> = None;
    for declaration in inventory["declarations"]
        .as_array()
        .cloned()
        .unwrap_or_default()
    {
        if declaration["path"].as_str() == Some(path) {
            target_id = declaration["id"].as_str().map(str::to_string);
            edits.push(serde_json::json!({
                "sel_start": declaration["sel_start"],
                "sel_len": declaration["id"].as_str().map(str::len).unwrap_or(0),
            }));
        }
    }
    // The ruby rename_references precondition: only references whose
    // OWN id matches the declaration's (case-insensitively) are
    // edited — a renamed interface alias (`shared_part AS item`)
    // keeps its local name.
    for reference in inventory["references"]
        .as_array()
        .cloned()
        .unwrap_or_default()
    {
        let id_matches = target_id
            .as_deref()
            .zip(reference["id"].as_str())
            .is_some_and(|(target, id)| id.eq_ignore_ascii_case(target));
        if reference["base_path"].as_str() == Some(path) && id_matches {
            edits.push(serde_json::json!({
                "sel_start": reference["sel_start"],
                "sel_len": reference["id"].as_str().map(str::len).unwrap_or(0),
            }));
        }
    }
    edits.sort_by_key(|edit| edit["sel_start"].as_u64().unwrap_or(u64::MAX));
    edits.dedup_by(|a, b| a["sel_start"] == b["sel_start"]);
    Ok(edits)
}

/// The delegation entry: source + path in, the edit rows out.
pub fn rename_edits_json(source: &str, path: &str) -> Result<String, String> {
    let edits = rename_edits(source, path)?;
    serde_json::to_string(&edits).map_err(|e| format!("edits json: {e}"))
}

#[cfg(test)]
mod tests {
    use super::inventory_json;
    use super::rename_edits_json;
    use super::symbols_json;

    #[test]
    fn inventories_declarations_and_resolved_references() {
        let source = "SCHEMA demo;\n  ENTITY base;\n  END_ENTITY;\n  ENTITY sub SUBTYPE OF (base);\n    name : STRING;\n  END_ENTITY;\nEND_SCHEMA;\n";
        let inventory: serde_json::Value =
            serde_json::from_str(&inventory_json(source).expect("inventory")).expect("json");
        let declarations = inventory["declarations"].as_array().expect("decls");
        let ids: Vec<&str> = declarations
            .iter()
            .map(|d| d["id"].as_str().expect("id"))
            .collect();
        assert_eq!(ids, vec!["demo", "base", "sub", "name"], "{declarations:?}");

        // The subtype's reference resolved onto the base entity's path.
        let references = inventory["references"].as_array().expect("refs");
        let base_ref = references
            .iter()
            .find(|r| r["id"] == "base")
            .expect("the subtype reference");
        let base_path = base_ref["base_path"].as_str().expect("resolved");
        let target = declarations
            .iter()
            .find(|d| d["path"] == base_path)
            .expect("the declaration at the base_path");
        assert_eq!(target["id"], "base");

        // The selection window lands on the identifier itself.
        let sub = declarations.iter().find(|d| d["id"] == "sub").unwrap();
        let at = sub["sel_start"].as_u64().expect("sel");
        assert_eq!(
            &source[at as usize..at as usize + 3],
            "sub",
            "the selection points at the identifier"
        );
    }

    #[test]
    fn rename_edits_cover_the_declaration_and_its_references() {
        let source = "SCHEMA demo;\n  ENTITY base;\n  END_ENTITY;\n  ENTITY sub SUBTYPE OF (base);\n    name : base;\n  END_ENTITY;\nEND_SCHEMA;\n";
        let edits: serde_json::Value = serde_json::from_str(
            &rename_edits_json(source, "document.exp.demo.base").expect("edits"),
        )
        .expect("json");
        let rows = edits.as_array().expect("rows");
        // Three distinct positions: the declaration, the SUBTYPE OF
        // reference, and the attribute type reference — each stamped
        // with its own capture offset.
        assert_eq!(rows.len(), 3, "the declaration + both references");
        let at = rows[0]["sel_start"].as_u64().expect("sel");
        assert_eq!(&source[at as usize..at as usize + 4], "base");
    }

    #[test]
    fn inventories_declarations_with_spans() {
        // `freedoms` wraps onto the next line; the ruby walk's
        // attribute span is the name token, so its end_line stays on
        // the declaration's first line.
        let source = "SCHEMA demo;\n  ENTITY point;\n    freedoms : SET [1:?] OF\n      other_thing;\n  END_ENTITY;\nEND_SCHEMA;\n";
        let rows: Vec<serde_json::Value> =
            serde_json::from_str(&symbols_json(source).expect("rows")).expect("array");
        let names: Vec<&str> = rows
            .iter()
            .map(|row| row["name"].as_str().expect("name"))
            .collect();
        assert_eq!(names, vec!["demo", "point", "freedoms"], "{rows:?}");
        let attr_row = &rows[2];
        assert_eq!(attr_row["span_line"], 3);
        assert_eq!(
            attr_row["span_end_line"], 3,
            "the wrapped attribute's span stays on its first line"
        );
    }
}
