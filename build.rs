fn main() {
    println!(
        "cargo:rustc-env=CARGO_PKG_NAME_UPPERCASE={}",
        env!("CARGO_PKG_NAME").to_uppercase().replace("-", "_")
    );
    println!("cargo:rerun-if-changed=Cargo.toml");
}
