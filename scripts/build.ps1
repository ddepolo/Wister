# Compila Wister en release y arma el instalador NSIS.
#
#   .\scripts\build.ps1            # con Vulkan (GPU)
#   .\scripts\build.ps1 -Cpu       # solo CPU
#   .\scripts\build.ps1 -Config x.json   # mezcla x.json con tauri.conf.json (ver release.ps1)
#
# Las actualizaciones se firman con la clave de %USERPROFILE%\.tauri\wister-actualizaciones.key
# (o la de TAURI_SIGNING_PRIVATE_KEY). Sin ella no se puede compilar: guardala con backup,
# porque si se pierde las versiones instaladas no pueden verificar las nuevas.
#
# El instalador queda en C:\wr\release\bundle\nsis\ (o C:\wt-cpu\... con -Cpu).
# Ver scripts/dev.ps1 para el porqué de cada variable.
#
# Los dos usan instrucciones de CPU fijas (AVX2) en vez de las de esta PC: la build con
# Vulkan también usa el procesador cuando no hay GPU, y con las nativas podría cerrarse
# en otra CPU. Por eso no comparte target con dev.ps1 (C:\wt), que usa las nativas:
# whisper.cpp no se vuelve a compilar solo cuando cambian estas variables.
param([switch]$Cpu, [string]$Config)

$ErrorActionPreference = "Stop"
$env:PATH = "$env:USERPROFILE\.cargo\bin;C:\Program Files\CMake\bin;C:\Program Files\LLVM\bin;$env:PATH"
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
$env:CMAKE_C_FLAGS_RELEASE = "/MD /O2 /Ob2 /DNDEBUG"
$env:CMAKE_CXX_FLAGS_RELEASE = "/MD /O2 /Ob2 /DNDEBUG"
# Instrucciones fijas (como en CI): el binario corre en cualquier x64 con AVX2.
$env:GGML_NATIVE = "OFF"
$env:GGML_AVX = "ON"
$env:GGML_AVX2 = "ON"
$env:GGML_FMA = "ON"
$env:GGML_F16C = "ON"

if (-not $env:TAURI_SIGNING_PRIVATE_KEY) {
    $clave = "$env:USERPROFILE\.tauri\wister-actualizaciones.key"
    if (-not (Test-Path $clave)) {
        throw "Falta la clave para firmar las actualizaciones ($clave): restaurala del backup."
    }
    $env:TAURI_SIGNING_PRIVATE_KEY = $clave
}
$extra = @()
if ($Config) { $extra = @("--config", $Config) }

if ($Cpu) {
    $env:CARGO_TARGET_DIR = "C:\wt-cpu"
    npx tauri build @extra
} else {
    if (-not $env:VULKAN_SDK) {
        $env:VULKAN_SDK = [Environment]::GetEnvironmentVariable("VULKAN_SDK", "Machine")
    }
    $env:PATH = "$env:VULKAN_SDK\Bin;$env:PATH"
    # Target corto por el límite de 260 caracteres de MSBuild (ver dev.ps1).
    $env:CARGO_TARGET_DIR = "C:\wr"
    npx tauri build --features vulkan @extra
}

# PowerShell 5.1 no corta el script cuando falla un programa externo.
if ($LASTEXITCODE) { throw "La compilación falló (código $LASTEXITCODE)." }
