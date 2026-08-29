use std::path::Path;

fn main() {
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    if !assets.is_dir() {
        std::fs::create_dir_all(&assets).expect("the embedded asset directory can be created");
    }
    println!("cargo::rerun-if-changed=assets");
}
