//! Part 21 instance parsing and structural validation (TODO.compat/08
//! wasm surface): ISO-10303-21 exchange structure — header, data
//! sections, entity records — and the schema-facing checks the
//! conformance corpus exercises.
//!
//! Clause anchors:
//! - ISO 10303-21 clause 8/10: header section, FILE_SCHEMA.
//! - ISO 10303-21 clause 11: exchange structure of data sections.
//! - ISO 10303-21 clause 12: entity instance records; a record
//!   keyword shall name an entity (or complex part) of the file
//!   schema.

/// A parsed entity record: `#12=KEYWORD(arg, ...)`, its complex form
/// `#12=(A(1)(2)B(3))`, or a value reference.
#[derive(Debug, Clone, PartialEq)]
pub struct EntityRecord {
    pub id: u64,
    pub keyword: String,
    /// For complex records: the per-part keyword/value lists.
    pub parts: Vec<(String, Vec<Value>)>,
    pub values: Vec<Value>,
    pub complex: bool,
}

/// A lexical value inside a record.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Ref(u64),
    Integer(i64),
    Real(f64),
    String(String),
    Enumeration(String),
    Binary(String),
    Unset,
    Omitted,
    Typed(String, Box<Value>),
    List(Vec<Value>),
    /// Annex I: an external reference `<...>`
    ExternalRef(String),
    /// Annex J: an EXPRESS constant reference `#NAME`
    ConstantRef(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Population {
    pub file_schemas: Vec<String>,
    pub records: Vec<EntityRecord>,
    /// Annex I anchors: `<name> = target;` in source order
    pub anchors: Vec<(String, String)>,
    /// Annex I external references: `#id = <target>;` in source order
    pub references: Vec<(u64, String)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    pub valid: bool,
    pub errors: Vec<String>,
}

impl Population {
    /// Structural validation against a schema's entity inventory:
    /// every record keyword (or complex part) shall resolve (clause
    /// 12), case-insensitively.
    pub fn validate(&self, entities: &[String]) -> Verdict {
        let known: Vec<String> = entities.iter().map(|e| e.to_lowercase()).collect();
        let mut errors = Vec::new();
        for record in &self.records {
            if record.complex {
                for (part, _) in &record.parts {
                    if !known.contains(&part.to_lowercase()) {
                        errors.push(format!(
                            "#{}: unknown complex record part {part}",
                            record.id
                        ));
                    }
                }
            } else if !known.contains(&record.keyword.to_lowercase()) {
                errors.push(format!(
                    "#{}: unknown entity keyword {}",
                    record.id, record.keyword
                ));
            }
        }
        Verdict {
            valid: errors.is_empty(),
            errors,
        }
    }
}

/// Parses an ISO-10303-21 file body: ISO-10303-21 header + DATA
/// section records.
pub fn parse_population(input: &str) -> Result<Population, String> {
    let tokens = lex(input)?;
    let mut parser = P { tokens, pos: 0 };
    parser.parse()
}

struct P {
    tokens: Vec<Token>,
    pos: usize,
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Keyword(String),
    EntityId(u64),
    Integer(i64),
    Real(f64),
    Str(String),
    Enumeration(String),
    Binary(String),
    Unset,
    Omitted,
    LParen,
    RParen,
    Comma,
    Semicolon,
    Assign,
    TypedOpen(String),
    ExternalRef(String),
    ConstantRef(String),
}

impl P {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.pos).cloned();
        if token.is_some() {
            self.pos += 1;
        }
        token
    }

    fn parse(&mut self) -> Result<Population, String> {
        let mut file_schemas = Vec::new();
        let mut records = Vec::new();
        let mut anchors = Vec::new();
        let mut references = Vec::new();
        while let Some(token) = self.next() {
            match token {
                Token::Keyword(keyword) if keyword.eq_ignore_ascii_case("ISO-10303-21") => {}
                Token::Keyword(keyword) if keyword.eq_ignore_ascii_case("HEADER") => {}
                Token::Keyword(keyword) if keyword.eq_ignore_ascii_case("ENDSEC") => {}
                Token::Keyword(keyword) if keyword.eq_ignore_ascii_case("DATA") => {
                    // a named section may space out: DATA ('NAME', ...);
                    if matches!(self.peek(), Some(Token::LParen)) {
                        self.skip_parenthesized(0)?;
                    }
                }
                Token::Keyword(keyword) if keyword.eq_ignore_ascii_case("END-ISO-10303-21") => {}
                Token::TypedOpen(keyword) if keyword.eq_ignore_ascii_case("FILE_SCHEMA") => {
                    file_schemas.extend(self.collect_strings_parenthesized()?);
                }
                Token::TypedOpen(keyword) if keyword.eq_ignore_ascii_case("FILE_NAME") => {
                    self.skip_parenthesized(1)?;
                }
                Token::TypedOpen(keyword) if keyword.eq_ignore_ascii_case("FILE_DESCRIPTION") => {
                    self.skip_parenthesized(1)?;
                }
                Token::TypedOpen(keyword) if keyword.eq_ignore_ascii_case("FILE_POPULATION") => {
                    self.skip_parenthesized(1)?;
                }
                Token::TypedOpen(keyword) if keyword.eq_ignore_ascii_case("DATA") => {
                    // a named data section: DATA ('NAME', ('SCHEMA'));
                    self.skip_parenthesized(1)?;
                }
                Token::Keyword(_) | Token::Semicolon => {}
                Token::TypedOpen(_) => {
                    // any other keyword with arguments (e.g. the
                    // SCHEMA_POPULATION header): skip its balanced list
                    self.skip_parenthesized(1)?;
                }
                Token::Enumeration(mark) if mark.is_empty() => {
                    // the `..` anchor-section markers (annex E/I)
                }
                Token::ExternalRef(name) => {
                    // an anchor item `<name> = target ;` (annex I)
                    self.expect_assign()?;
                    let target = self.capture_anchor_target()?;
                    anchors.push((name, target));
                }
                Token::EntityId(id) => {
                    self.expect_assign()?;
                    if matches!(self.peek(), Some(Token::ExternalRef(target))) {
                        // an external reference, not a record (annex I)
                        let target = match self.next() {
                            Some(Token::ExternalRef(text)) => text,
                            _ => unreachable!(),
                        };
                        references.push((id, target));
                        self.expect_semicolon()?;
                    } else {
                        let record = self.parse_record(id)?;
                        records.push(record);
                    }
                }
                other => {
                    return Err(format!(
                        "unexpected token {other:?} at record level (token {})",
                        self.pos
                    ));
                }
            }
        }
        Ok(Population {
            file_schemas,
            records,
            anchors,
            references,
        })
    }

    /// Skips a balanced parenthesized argument list. `depth0` is 1
    /// when the caller's TypedOpen already carries the keyword's own
    /// '(', 0 when the list starts at a bare LParen.
    fn skip_parenthesized(&mut self, depth0: usize) -> Result<(), String> {
        let mut depth = depth0;
        loop {
            match self.next() {
                Some(Token::LParen) | Some(Token::TypedOpen(_)) => depth += 1,
                Some(Token::RParen) => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(());
                    }
                }
                Some(_) => {}
                None => return Err("unterminated header argument list".to_string()),
            }
        }
    }

    /// Captures an anchor target's source text (`#n`, `$` or an
    /// external reference).
    fn capture_anchor_target(&mut self) -> Result<String, String> {
        let text = match self.next() {
            Some(Token::EntityId(id)) => format!("#{id}"),
            Some(Token::Unset) => "$".to_string(),
            Some(Token::ExternalRef(target)) => format!("<{target}>"),
            other => return Err(format!("unexpected anchor target {other:?}")),
        };
        self.expect_semicolon()?;
        Ok(text)
    }

    fn expect_assign(&mut self) -> Result<(), String> {
        match self.next() {
            Some(Token::Assign) => Ok(()),
            other => Err(format!("expected = after entity id, got {other:?}")),
        }
    }

    fn parse_record(&mut self, id: u64) -> Result<EntityRecord, String> {
        match self.peek() {
            Some(Token::LParen) => self.parse_complex(id),
            Some(Token::TypedOpen(keyword)) => {
                let keyword = keyword.clone();
                self.next();
                let values = self.parse_value_list()?;
                self.expect_semicolon()?;
                Ok(EntityRecord {
                    id,
                    keyword,
                    parts: Vec::new(),
                    values,
                    complex: false,
                })
            }
            other => Err(format!("expected record body for #{id}, got {other:?}")),
        }
    }

    fn parse_complex(&mut self, id: u64) -> Result<EntityRecord, String> {
        self.next(); // consume (
        let mut parts = Vec::new();
        while let Some(token) = self.next() {
            match token {
                Token::TypedOpen(keyword) => {
                    let values = self.parse_value_list()?;
                    parts.push((keyword, values));
                }
                Token::RParen => break,
                other => return Err(format!("unexpected token in complex record: {other:?}")),
            }
        }
        self.expect_semicolon()?;
        Ok(EntityRecord {
            id,
            keyword: String::new(),
            parts,
            values: Vec::new(),
            complex: true,
        })
    }

    fn parse_value_list(&mut self) -> Result<Vec<Value>, String> {
        let mut values = Vec::new();
        loop {
            match self.peek() {
                Some(Token::RParen) | None => {
                    self.next();
                    return Ok(values);
                }
                Some(Token::Comma) => {
                    self.next();
                }
                _ => values.push(self.parse_value()?),
            }
        }
    }

    /// Collects the string literals of a balanced parenthesized
    /// argument list (TypedOpen already carries the keyword's '(').
    fn collect_strings_parenthesized(&mut self) -> Result<Vec<String>, String> {
        let mut strings = Vec::new();
        let mut depth = 1usize;
        while depth > 0 {
            match self.next() {
                Some(Token::LParen) | Some(Token::TypedOpen(_)) => depth += 1,
                Some(Token::RParen) => depth -= 1,
                Some(Token::Str(text)) => strings.push(text),
                Some(_) => {}
                None => return Err("unterminated string list".to_string()),
            }
        }
        Ok(strings)
    }

    fn parse_value(&mut self) -> Result<Value, String> {
        match self.next() {
            Some(Token::EntityId(id)) => Ok(Value::Ref(id)),
            Some(Token::Integer(value)) => Ok(Value::Integer(value)),
            Some(Token::Real(value)) => Ok(Value::Real(value)),
            Some(Token::Str(value)) => Ok(Value::String(value)),
            Some(Token::Enumeration(value)) => Ok(Value::Enumeration(value)),
            Some(Token::Binary(value)) => Ok(Value::Binary(value)),
            Some(Token::Unset) => Ok(Value::Unset),
            Some(Token::Omitted) => Ok(Value::Omitted),
            Some(Token::TypedOpen(keyword)) => {
                // a typed value's payload is the full parameter list
                // up to the keyword's own ')'
                let items = self.parse_value_list()?;
                Ok(Value::Typed(keyword, Box::new(Value::List(items))))
            }
            Some(Token::ExternalRef(target)) => Ok(Value::ExternalRef(target)),
            Some(Token::ConstantRef(name)) => Ok(Value::ConstantRef(name)),
            Some(Token::LParen) => {
                let mut items = Vec::new();
                loop {
                    match self.peek() {
                        Some(Token::RParen) | None => {
                            self.next();
                            break;
                        }
                        Some(Token::Comma) => {
                            self.next();
                        }
                        _ => items.push(self.parse_value()?),
                    }
                }
                Ok(Value::List(items))
            }
            other => Err(format!("unexpected value token {other:?}")),
        }
    }

    fn expect_semicolon(&mut self) -> Result<(), String> {
        match self.next() {
            Some(Token::Semicolon) | None => Ok(()),
            other => Err(format!("expected ; after record, got {other:?}")),
        }
    }
}

fn lex(input: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        match c {
            '/' if input[i..].starts_with("/*") => {
                i = match input[i + 2..].find("*/") {
                    Some(end) => i + 2 + end + 2,
                    None => return Err("unterminated comment".to_string()),
                };
            }
            ' ' | '\t' | '\r' | '\n' => i += 1,
            '(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            ',' => {
                tokens.push(Token::Comma);
                i += 1;
            }
            ';' => {
                tokens.push(Token::Semicolon);
                i += 1;
            }
            '=' => {
                tokens.push(Token::Assign);
                i += 1;
            }
            '$' => {
                tokens.push(Token::Unset);
                i += 1;
            }
            '*' => {
                tokens.push(Token::Omitted);
                i += 1;
            }
            '#' => {
                let next_byte = bytes.get(i + 1).copied().unwrap_or(b' ');
                if next_byte.is_ascii_digit() {
                    let (number, next) = read_number(input, i + 1)?;
                    tokens.push(Token::EntityId(number));
                    i = next;
                } else {
                    // annex J: an EXPRESS constant reference
                    let (word, next) = read_keyword(input, i + 1)?;
                    tokens.push(Token::ConstantRef(word));
                    i = next;
                }
            }
            '\'' => {
                let (text, next) = read_string(input, i + 1)?;
                tokens.push(Token::Str(text));
                i = next;
            }
            '"' => {
                let (bits, next) = read_binary(input, i + 1)?;
                tokens.push(Token::Binary(bits));
                i = next;
            }
            '.' => {
                let (word, next) = read_delimited(input, i + 1, '.')?;
                tokens.push(Token::Enumeration(word));
                i = next;
            }
            '<' => {
                let (target, next) = read_delimited(input, i + 1, '>')?;
                tokens.push(Token::ExternalRef(target));
                i = next;
            }
            '0'..='9' | '+' | '-' => {
                let (token, next) = read_number_or_real(input, i)?;
                tokens.push(token);
                i = next;
            }
            _ if c.is_ascii_alphabetic() => {
                let (word, next) = read_keyword(input, i)?;
                i = next;
                if i < bytes.len() && bytes[i] == b'(' {
                    // TypedOpen carries the keyword's own '(' — the
                    // paren is consumed here and not re-lexed
                    tokens.push(Token::TypedOpen(word));
                    i += 1;
                } else {
                    tokens.push(Token::Keyword(word));
                }
            }
            other => return Err(format!("unexpected character {other:?} at byte {i}")),
        }
    }
    Ok(tokens)
}

fn read_number(input: &str, start: usize) -> Result<(u64, usize), String> {
    let bytes = input.as_bytes();
    let mut end = start;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    input[start..end]
        .parse::<u64>()
        .map(|n| (n, end))
        .map_err(|e| format!("bad entity id: {e}"))
}

fn read_number_or_real(input: &str, start: usize) -> Result<(Token, usize), String> {
    let bytes = input.as_bytes();
    let mut end = start;
    let mut is_real = false;
    if end < bytes.len() && (bytes[end] == b'+' || bytes[end] == b'-') {
        end += 1;
    }
    while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'.') {
        if bytes[end] == b'.' {
            // ".." would be a range; not valid inside a value list
            is_real = true;
        }
        end += 1;
    }
    if end < bytes.len() && (bytes[end] == b'E' || bytes[end] == b'e') {
        is_real = true;
        end += 1;
        if end < bytes.len() && (bytes[end] == b'+' || bytes[end] == b'-') {
            end += 1;
        }
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
    }
    let text = &input[start..end];
    if is_real {
        text.parse::<f64>()
            .map(|v| (Token::Real(v), end))
            .map_err(|e| format!("bad real: {e}"))
    } else {
        text.parse::<i64>()
            .map(|v| (Token::Integer(v), end))
            .map_err(|e| format!("bad integer {text:?} at {start}: {e}"))
    }
}

fn read_string(input: &str, start: usize) -> Result<(String, usize), String> {
    let bytes = input.as_bytes();
    let mut out = String::new();
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                    out.push('\'');
                    i += 2;
                } else {
                    return Ok((out, i + 1));
                }
            }
            b'\\' if i + 1 < bytes.len() => {
                out.push(bytes[i + 1] as char);
                i += 2;
            }
            b => {
                out.push(b as char);
                i += 1;
            }
        }
    }
    Err("unterminated string".to_string())
}

fn read_binary(input: &str, start: usize) -> Result<(String, usize), String> {
    match input[start..].find('"') {
        Some(end) => Ok((input[start..start + end].to_string(), start + end + 1)),
        None => Err("unterminated binary".to_string()),
    }
}

fn read_delimited(input: &str, start: usize, delimiter: char) -> Result<(String, usize), String> {
    match input[start..].find(delimiter) {
        Some(end) => Ok((input[start..start + end].to_string(), start + end + 1)),
        None => Err(format!("unterminated .{delimiter} token")),
    }
}

fn read_keyword(input: &str, start: usize) -> Result<(String, usize), String> {
    let bytes = input.as_bytes();
    let mut end = start;
    // exchange-structure tokens carry hyphens (ISO-10303-21,
    // END-ISO-10303-21); signed numbers never start alphabetic
    while end < bytes.len()
        && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_' || bytes[end] == b'-')
    {
        end += 1;
    }
    Ok((input[start..end].to_string(), end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_annex_h_shape() {
        let source = "ISO-10303-21;\nHEADER;\nFILE_NAME('a','2026-01-01T00:00:00',(''),(''),'','','');\nFILE_SCHEMA(('BASE'));\nENDSEC;\nDATA;\n#1=A(1,2.5,.T.,'x',\"0AFF\");\n#2=B((#1),$,*);\nENDSEC;\nEND-ISO-10303-21;\n";
        let population = parse_population(source).expect("parse");
        assert_eq!(population.file_schemas, vec!["BASE".to_string()]);
        assert_eq!(population.records.len(), 2);
        assert_eq!(population.records[0].keyword, "A");
        assert_eq!(
            population.records[0].values,
            vec![
                Value::Integer(1),
                Value::Real(2.5),
                Value::Enumeration("T".to_string()),
                Value::String("x".to_string()),
                Value::Binary("0AFF".to_string()),
            ]
        );
        assert_eq!(
            population.records[1].values[0],
            Value::List(vec![Value::Ref(1)])
        );
        assert_eq!(population.records[1].values[1], Value::Unset);
        assert_eq!(population.records[1].values[2], Value::Omitted);
    }

    #[test]
    fn parses_multi_schema_and_typed_and_complex_records() {
        let source = "ISO-10303-21;\nHEADER;\nFILE_SCHEMA(('LONGA', 'LONGB'));\nENDSEC;\nDATA;\n#1=CARTESIAN_POINT('p',DIRECTION('d',(1.0,0.0)));\n#2=(A(1)B(2)C(3));\nENDSEC;\nEND-ISO-10303-21;\n";
        let population = parse_population(source).expect("parse");
        assert_eq!(population.file_schemas.len(), 2);
        assert!(!population.records[0].complex);
        match &population.records[0].values[1] {
            Value::Typed(keyword, inner) => {
                assert_eq!(keyword, "DIRECTION");
                assert!(matches!(inner.as_ref(), Value::List(_)));
            }
            other => panic!("expected typed value, got {other:?}"),
        }
        assert!(population.records[1].complex);
        assert_eq!(population.records[1].parts.len(), 3);
        assert_eq!(population.records[1].parts[0].0, "A");
    }

    #[test]
    fn validates_keywords_against_the_schema_inventory() {
        let source = "ISO-10303-21;\nHEADER;\nFILE_SCHEMA(('DEMO'));\nENDSEC;\nDATA;\n#1=point(1.0);\n#2=wat(2.0);\n#3=(point(1.0)wat(2.0));\nENDSEC;\nEND-ISO-10303-21;\n";
        let population = parse_population(source).expect("parse");
        let verdict = population.validate(&["point".to_string(), "line".to_string()]);
        assert!(!verdict.valid);
        assert_eq!(verdict.errors.len(), 2);
        assert!(verdict.errors[0].contains("#2"));
        assert!(verdict.errors[1].contains("#3"));

        let ok = population.validate(&["point".to_string(), "wat".to_string()]);
        assert!(ok.valid);
    }
}
