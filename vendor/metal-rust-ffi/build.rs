fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rerun-if-changed=src/MetalFX/MetalFXMatrixShim.m");
        cc::Build::new()
            .file("src/MetalFX/MetalFXMatrixShim.m")
            .flag("-fobjc-arc")
            .compile("metal_rust_metalfx_matrix");
        println!("cargo:rustc-link-arg=-Wl,-weak_framework,MetalFX");
    }
}
