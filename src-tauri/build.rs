fn main() {
    // `vulkan-1.dll` viene con los drivers de video: si el ejecutable la importa de forma
    // normal, en una PC sin drivers ni siquiera abre. Se carga recién cuando whisper.cpp
    // la usa, y si no está, `gpu.rs` hace que siga con la CPU.
    let vulkan = std::env::var_os("CARGO_FEATURE_VULKAN").is_some();
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|e| e == "msvc");
    if vulkan && msvc {
        println!("cargo:rustc-link-arg-bins=/DELAYLOAD:vulkan-1.dll");
        println!("cargo:rustc-link-arg-bins=delayimp.lib");
    }
    tauri_build::build()
}
