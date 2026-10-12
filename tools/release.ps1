# Fabrique une version de PaceWall : numéro de version, tests, programme optimisé, fichier d'installation,
# empreinte, fichier « latest.json » lu par la recherche de mises à jour, commit et étiquette git.
#
#   powershell -ExecutionPolicy Bypass -File tools\release.ps1 -Version 0.9.1
#
# Avant de lancer : ajouter dans CHANGELOG.md une section « ## 0.9.1 » qui décrit la version.
# Résultat dans dist\ : PaceWall-Setup-0.9.1.exe et latest.json, à joindre à la version sur GitHub.
# -NoGit : ne fait ni commit ni étiquette (pour essayer).
param(
    [Parameter(Mandatory = $true)][string]$Version,
    [switch]$NoGit
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$utf8 = New-Object System.Text.UTF8Encoding($false)

function Fail($message) { Write-Host "ERREUR : $message" -ForegroundColor Red; exit 1 }
function Step($message) { Write-Host "== $message" -ForegroundColor Cyan }

if ($Version -notmatch '^\d+\.\d+\.\d+$') { Fail "la version doit avoir la forme 1.2.3" }

# Notes de version : le texte de la section « ## x.y.z » du journal des modifications.
$changelog = [IO.File]::ReadAllText("$root\CHANGELOG.md", $utf8)
$section = [regex]::Match($changelog, "(?ms)^## " + [regex]::Escape($Version) + "[^\r\n]*\r?\n(.*?)(?=^## |\z)")
if (-not $section.Success) { Fail "CHANGELOG.md n'a pas de section « ## $Version »" }
$notes = ($section.Groups[1].Value.Trim() -split "\r?\n" | Where-Object { $_.Trim() -ne "" } | ForEach-Object { $_.Trim() -replace '^[-*] ', '' }) -join "`n"
if ($notes -eq "") { Fail "la section « ## $Version » de CHANGELOG.md est vide" }

if (-not $NoGit) {
    # Seul le journal peut être en attente : tout le reste doit déjà être commité.
    $pending = @(git status --porcelain | Where-Object { $_ -notmatch 'CHANGELOG\.md$' })
    if ($pending.Count -gt 0) { Fail "des modifications ne sont pas commitées :`n$($pending -join "`n")" }
    if (git tag --list "v$Version") { Fail "l'étiquette v$Version existe déjà" }
}

Step "Version $Version"
$cargo = [IO.File]::ReadAllText("$root\Cargo.toml", $utf8)
$updated = [regex]::Replace($cargo, '(?ms)(\[workspace\.package\]\s*\r?\nversion = ")[^"]+(")', "`${1}$Version`${2}")
if ($updated -eq $cargo -and $cargo -notmatch "version = `"$Version`"") { Fail "version introuvable dans Cargo.toml" }
[IO.File]::WriteAllText("$root\Cargo.toml", $updated, $utf8)

Step "Tests"
cargo test --workspace --quiet
if ($LASTEXITCODE -ne 0) { Fail "les tests ne passent pas" }

Step "Programme optimisé"
cargo build --release -p pacewall
if ($LASTEXITCODE -ne 0) { Fail "la compilation a échoué" }

Step "Fichier d'installation"
New-Item -ItemType Directory -Force "$root\dist" | Out-Null
$name = "PaceWall-Setup-$Version.exe"
Copy-Item "$root\target\release\pacewall.exe" "$root\dist\$name" -Force
$built = (Get-Item "$root\dist\$name").VersionInfo.ProductVersion
if ($built -and -not $built.StartsWith($Version)) { Fail "le programme compilé porte la version $built au lieu de $Version" }
$hash = (Get-FileHash "$root\dist\$name" -Algorithm SHA256).Hash.ToLower()
$latest = [ordered]@{ version = $Version; file = $name; sha256 = $hash; notes = $notes } | ConvertTo-Json
[IO.File]::WriteAllText("$root\dist\latest.json", $latest, $utf8)
$size = [math]::Round((Get-Item "$root\dist\$name").Length / 1MB, 1)

if (-not $NoGit) {
    Step "Commit et étiquette"
    git add Cargo.toml Cargo.lock CHANGELOG.md
    git commit -q -m "Version $Version"
    git tag "v$Version"
}

Write-Host ""
Write-Host "Version $Version prête dans dist\ :" -ForegroundColor Green
Write-Host "  $name  ($size Mo)"
Write-Host "  latest.json  (sha256 $hash)"
Write-Host ""
Write-Host "Pour la publier :"
Write-Host "  1. git push ; git push --tags"
Write-Host "  2. Sur GitHub : Releases > Draft a new release, étiquette v$Version,"
Write-Host "     joindre les DEUX fichiers de dist\ (l'installeur et latest.json), puis publier."
Write-Host "  Les PaceWall déjà installés proposeront la mise à jour dans les 12 heures (ou tout de suite via"
Write-Host "  Réglages > Chercher maintenant)."
