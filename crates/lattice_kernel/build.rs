// build.rs — emit rustc flags that enable AVX-512 / AVX2 when supported
fn main() {
    // Tell rustc to enable target-feature detection for the SIMD paths.
    // The actual feature gates are handled at runtime via `is_x86_feature_detected!`.
    // However, we still need to emit cfg flags so that the unsafe simd blocks compile.
    println!("cargo:rustc-cfg=feature=\"avx512\"");
    println!("cargo:rerun-if-changed=build.rs");
}
