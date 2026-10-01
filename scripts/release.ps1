# Arma una versión para GitHub Releases: compila el instalador y escribe latest.json (lo
# que lee el actualizador, con la firma), notas.md (la sección del CHANGELOG) y
# SHA256SUMS.txt.
#
# Hay un solo instalador: usa la GPU con Vulkan si puede y si no, la CPU. Hasta la 0.3.0
# había otro de solo CPU; esas instalaciones buscan la entrada `windows-x86_64-cpu` de
# latest.json, que apunta al mismo instalador para que se pasen solas a este.
#
#   .\scripts\release.ps1                # deja todo en publicar\v<versión> (en el proyecto)
#   .\scripts\release.ps1 -Prueba        # para probar el actualizador en esta PC
#   .\scripts\release.ps1 -SinCompilar   # reusa el instalador ya compilado
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

$nombre = "Wister_${version}_x64-setup.exe"
$origen = "C:\wr\release\bundle\nsis\$nombre"
if (-not (Test-Path "$origen.sig")) { throw "No está $origen.sig: compilá primero." }
Copy-Item $origen "$salida\$nombre"
$instalador = [ordered]@{
    signature = (Get-Content "$origen.sig" -Raw).Trim()
    url = "$base/$nombre"
}
$plataformas = [ordered]@{
    "windows-x86_64" = $instalador
    "windows-x86_64-cpu" = $instalador
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
