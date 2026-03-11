// build.rs
// Configures linker search paths for optional GPU backends.

fn main() {
    // ── ROCm / HIP ────────────────────────────────────────────────────────────
    // When compiling with --features rocm, tell the linker where libamdhip64.so
    // lives.  The path can be overridden via the ROCM_PATH environment variable.
    #[cfg(feature = "rocm")]
    {
        let rocm_path = std::env::var("ROCM_PATH")
            .unwrap_or_else(|_| "/opt/rocm".to_string());

        println!("cargo:rustc-link-search=native={}/lib", rocm_path);
        println!("cargo:rustc-link-lib=dylib=amdhip64");
        println!("cargo:rerun-if-env-changed=ROCM_PATH");
        println!("cargo:rerun-if-changed=build.rs");

        // Validate that the library exists at build time
        let lib_path = format!("{}/lib/libamdhip64.so", rocm_path);
        if !std::path::Path::new(&lib_path).exists() {
            println!(
                "cargo:warning=libamdhip64.so not found at {}.  \
                 Set ROCM_PATH to your ROCm installation directory.",
                lib_path
            );
        }
    }

    // ── CUDA ─────────────────────────────────────────────────────────────────
    // cudarc handles CUDA linking itself; nothing extra needed here.

    // ── OpenCL ───────────────────────────────────────────────────────────────
    // opencl3 handles OpenCL linking itself via the OpenCL ICD loader.
}
