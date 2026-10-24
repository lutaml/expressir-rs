fn main() {
    for path in std::env::args().skip(1) {
        let src = std::fs::read_to_string(&path).unwrap();
        match expressir_rs::p21::parse_population(&src) {
            Ok(p) => println!(
                "{path}: ok schemas={:?} records={} anchors={} refs={}",
                p.file_schemas,
                p.records.len(),
                p.anchors.len(),
                p.references.len()
            ),
            Err(e) => println!("{path}: ERR {e}"),
        }
    }
}
