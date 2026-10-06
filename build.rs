// the screen is baked into the binary from web/dist. make sure the folder exists so a
// fresh clone still compiles, and rebuild when the screen is rebuilt.
fn main() {
    std::fs::create_dir_all("web/dist").expect("create web/dist");
    println!("cargo:rerun-if-changed=web/dist");
}
