# Compila Wister en release y arma el instalador NSIS.
#
#   .\scripts\build.ps1            # con Vulkan (GPU)
#   .\scripts\build.ps1 -Cpu       # solo CPU
#
# El instalador queda en C:\wt\release\bundle\nsis\ (o C:\wt-cpu\... con -Cpu).
# Ver scripts/dev.ps1 para el porqué de cada variable.
param([switch]$Cpu)

$ErrorActionPreference = "Stop"
$env:PATH = "$env:USERPROFILE\.cargo\bin;C:\Program Files\CMake\bin;C:\Program Files\LLVM\bin;$env:PATH"
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
$env:CMAKE_C_FLAGS_RELEASE = "/MD /O2 /Ob2 /DNDEBUG"
$env:CMAKE_CXX_FLAGS_RELEASE = "/MD /O2 /Ob2 /DNDEBUG"

if ($Cpu) {
    # Instrucciones fijas (como en CI) para que el binario corra en cualquier x64 con AVX2.
    $env:GGML_NATIVE = "OFF"
    $env:GGML_AVX = "ON"
    $env:GGML_AVX2 = "ON"
    $env:GGML_FMA = "ON"
    $env:GGML_F16C = "ON"
    $env:CARGO_TARGET_DIR = "C:\wt-cpu"
    npx tauri build
} else {
    if (-not $env:VULKAN_SDK) {
        $env:VULKAN_SDK = [Environment]::GetEnvironmentVariable("VULKAN_SDK", "Machine")
    }
    $env:PATH = "$env:VULKAN_SDK\Bin;$env:PATH"
    $env:CARGO_TARGET_DIR = "C:\wt"
    npx tauri build --features vulkan
}
