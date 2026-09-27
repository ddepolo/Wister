# Corre la app en modo desarrollo con el entorno que necesita whisper.cpp en Windows.
#
#   .\scripts\dev.ps1            # con Vulkan (GPU)
#   .\scripts\dev.ps1 -Cpu       # solo CPU
#
# Ver docs/fase-0.md: sin los CMAKE_*_FLAGS_RELEASE whisper.cpp queda sin /O2, y con
# Vulkan hace falta un target corto por el límite de 260 caracteres de MSBuild.
param([switch]$Cpu)

$env:PATH = "$env:USERPROFILE\.cargo\bin;C:\Program Files\CMake\bin;C:\Program Files\LLVM\bin;$env:PATH"
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
$env:CMAKE_C_FLAGS_RELEASE = "/MD /O2 /Ob2 /DNDEBUG"
$env:CMAKE_CXX_FLAGS_RELEASE = "/MD /O2 /Ob2 /DNDEBUG"

if ($Cpu) {
    npx tauri dev
} else {
    if (-not $env:VULKAN_SDK) {
        $env:VULKAN_SDK = [Environment]::GetEnvironmentVariable("VULKAN_SDK", "Machine")
    }
    $env:PATH = "$env:VULKAN_SDK\Bin;$env:PATH"
    $env:CARGO_TARGET_DIR = "C:\wt"
    npx tauri dev --features vulkan
}
