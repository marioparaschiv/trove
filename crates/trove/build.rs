fn main() {
    // The llama/ggml shared libraries sit beside the binary; without this rpath only `cargo run`/`cargo test` find them.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        println!("cargo:rustc-link-arg-bins=-Wl,-rpath,$ORIGIN");
    }
}
