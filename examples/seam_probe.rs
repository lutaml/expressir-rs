use expressir_rs::ParsedTree;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: seam_probe <file.exp>");
    let src = std::fs::read_to_string(&path).expect("read");
    match ParsedTree::parse(&src) {
        Ok(_) => println!("OK"),
        Err(e) => println!("FAIL {e:?}"),
    }
}
