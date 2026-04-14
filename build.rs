fn main() {
    let pkg_name = std::env::var("CARGO_PKG_NAME").expect("CARGO_PKG_NAME no found");
    let prefix = format!("{}_", pkg_name.to_uppercase().replace("-", "_"));

    println!("cargo:rustc-env=CARGO_PKG_NAME_UPPERCASE={}", prefix);

    println!("cargo:rerun-if-changed=Cargo.toml");
}
