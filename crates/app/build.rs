// 把应用图标（assets/app.res，由 scripts/icon.py 生成）链接进 exe，编译时不需要 Windows SDK
fn main() {
    println!("cargo:rerun-if-changed=assets/app.res");
    println!("cargo:rustc-link-arg-bins={}/assets/app.res", env!("CARGO_MANIFEST_DIR"));
}
