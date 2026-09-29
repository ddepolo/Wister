# Arma una versión para GitHub Releases: compila los dos instaladores (Vulkan y solo
# CPU) y escribe latest.json (lo que lee el actualizador, con las firmas), notas.md
# (la sección del CHANGELOG) y SHA256SUMS.txt.
#
#   .\scripts\release.ps1                # deja todo en publicar\v<versión> (en el proyecto)
#   .\scripts\release.ps1 -Prueba        # para probar el actualizador en esta PC
#   .\scripts\release.ps1 -SinCompilar   # reusa los instaladores ya compilados
#
# -Prueba compila con scripts\actualizacion-prueba.json, que apunta el actualizador a
# http://127.0.0.1:8765, y deja los archivos en publicar\prueba-v<versión>. Para
# servirlos: python -m http.server 8765 --directory <esa carpeta>.
param([switch]$Prueba, [switch]$SinCompilar)

$ErrorActionPreference = "Stop"
$raiz = Split-Path $PSScriptRoot
$version = (Select-String -Path "$raiz\Cargo.toml" -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
$tag = "v$version"

if ($Prueba) {
    $config = "$PSScriptRoot\actualizacion-prueba.json"
    $base = "http://127.0.0.1:8765"
    $salida = "$raiz\publicar\prueba-$tag"
} else {
    $config = ""
    $base = "https://github.com/ddepolo/Wister/releases/download/$tag"
    $salida = "$raiz\publicar\$tag"
}

if (-not $SinCompilar) {
    & "$PSScriptRoot\build.ps1" -Config $config
    & "$PSScriptRoot\build.ps1" -Cpu -Config $config
}

# Las notas son la sección de esta versión en el CHANGELOG (hasta la siguiente `## [`
# o hasta los links del final).
$changelog = (Get-Content "$raiz\CHANGELOG.md" -Raw -Encoding UTF8) -replace "`r", ""
$seccion = [regex]::Match($changelog, "(?ms)^## \[$([regex]::Escape($version))\][^\n]*\n(.*?)(?=^## \[|^\[)")
if ($seccion.Success) {
    $notas = $seccion.Groups[1].Value.Trim()
} elseif ($Prueba) {
    $notas = "Versión de prueba del actualizador."
} else {
    throw "Falta la sección [$version] en CHANGELOG.md."
}

if (Test-Path $salida) { Remove-Item -Recurse -Force $salida }
New-Item -ItemType Directory -Force $salida | Out-Null

$variantes = @(
    @{ Clave = "windows-x86_64"; Target = "C:\wr"; Nombre = "Wister_${version}_x64-setup.exe" },
    @{ Clave = "windows-x86_64-cpu"; Target = "C:\wt-cpu"; Nombre = "Wister_${version}_x64-cpu-setup.exe" }
)
$plataformas = [ordered]@{}
foreach ($v in $variantes) {
    $origen = "$($v.Target)\release\bundle\nsis\Wister_${version}_x64-setup.exe"
    if (-not (Test-Path "$origen.sig")) { throw "No está $origen.sig: compilá primero." }
    Copy-Item $origen "$salida\$($v.Nombre)"
    $plataformas[$v.Clave] = [ordered]@{
        signature = (Get-Content "$origen.sig" -Raw).Trim()
        url = "$base/$($v.Nombre)"
    }
}

# Sin BOM: el actualizador lee el JSON tal cual.
$utf8 = New-Object System.Text.UTF8Encoding $false
$latest = [ordered]@{
    version = $version
    notes = $notas
    pub_date = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
    platforms = $plataformas
}
[IO.File]::WriteAllText("$salida\latest.json", ($latest | ConvertTo-Json -Depth 5), $utf8)
[IO.File]::WriteAllText("$salida\notas.md", $notas + "`n", $utf8)

$sumas = Get-ChildItem $salida -Filter *.exe | Sort-Object Name | ForEach-Object {
    "{0}  {1}" -f (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower(), $_.Name
}
[IO.File]::WriteAllText("$salida\SHA256SUMS.txt", ($sumas -join "`n") + "`n", $utf8)

Write-Host ""
Write-Host "Listo: $salida"
Get-ChildItem $salida | ForEach-Object { Write-Host ("  {0,-40} {1,8:N1} MB" -f $_.Name, ($_.Length / 1MB)) }
