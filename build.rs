fn main() {
    println!("cargo:rerun-if-changed=shim/glibc_compat.c");
    let target = std::env::var("TARGET").unwrap_or_default();
    if std::env::var_os("CARGO_FEATURE_EMBED").is_some() && target.contains("linux-gnu") {
        cc::Build::new().file("shim/glibc_compat.c").flag("-std=c11").cargo_metadata(false).compile("glibc_compat");
        let out = std::env::var("OUT_DIR").unwrap();
        // whole-archive: the C++ objects in llama.cpp reference these, so they must be in before libc is searched
        println!("cargo:rustc-link-arg=-Wl,--whole-archive");
        println!("cargo:rustc-link-arg={out}/libglibc_compat.a");
        println!("cargo:rustc-link-arg=-Wl,--no-whole-archive");
    }
}
