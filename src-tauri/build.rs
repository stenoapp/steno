fn main() {
    // The Swift bridges inside apple-cf / screencapturekit are built with a
    // deployment target that requires the Swift Concurrency back-compat
    // dylib at runtime. macOS exposes that dylib via the dyld shared cache
    // as `/usr/lib/swift/libswift_Concurrency.dylib`, but `cargo build`
    // does not add `/usr/lib/swift` to the binary's rpath. Without this
    // explicit rpath, the dev binary fails to launch with:
    //   dyld: Library not loaded: @rpath/libswift_Concurrency.dylib
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    }
    tauri_build::build()
}
