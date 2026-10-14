use std::path::PathBuf;

fn main() {
    let object = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("assets/app_icon.o");
    println!("cargo:rustc-link-arg={}", object.display());
    println!("cargo:rerun-if-changed=assets/app_icon.o");
}
