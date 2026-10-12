# Fabrique et publie une version de PaceWall : numéro de version, tests, programme optimisé, fichier d'installation,
# empreinte, fichier « latest.json » lu par la recherche de mises à jour, commit, étiquette git, envoi sur GitHub et
# création de la version (« release ») avec ses deux fichiers.
#
#   powershell -ExecutionPolicy Bypass -File tools\release.ps1 -Version 0.9.1
#
# Avant de lancer : ajouter dans CHANGELOG.md une section « ## 0.9.1 » qui décrit la version.
# Résultat dans dist\ : PaceWall-Setup-0.9.1.exe et latest.json, joints à la version sur GitHub.
#
# -NoGit     : essai local, sans commit, étiquette ni publication.
# -NoPublish : commit et étiquette, mais rien n'est envoyé sur GitHub.
#
# La publication utilise l'identifiant GitHub que git a déjà sur cet ordinateur (celui des « git push ») : rien à
# installer ni à configurer. Le jeton n'est ni affiché ni enregistré.
param(
    [Parameter(Mandatory = $true)][string]$Version,
    [switch]$NoGit,
    [switch]$NoPublish
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$utf8 = New-Object System.Text.UTF8Encoding($false)

function Fail($message) { Write-Host "ERREUR : $message" -ForegroundColor Red; exit 1 }
function Step($message) { Write-Host "== $message" -ForegroundColor Cyan }

if ($Version -notmatch '^\d+\.\d+\.\d+$') { Fail "la version doit avoir la forme 1.2.3" }
$publish = -not $NoGit -and -not $NoPublish

# Notes de version : le texte de la section « ## x.y.z » du journal des modifications.
$changelog = [IO.File]::ReadAllText("$root\CHANGELOG.md", $utf8)
$section = [regex]::Match($changelog, "(?ms)^## " + [regex]::Escape($Version) + "[^\r\n]*\r?\n(.*?)(?=^## |\z)")
if (-not $section.Success) { Fail "CHANGELOG.md n'a pas de section « ## $Version »" }
$body = $section.Groups[1].Value.Trim() -replace "\r\n", "`n"
$notes = ($body -split "`n" | Where-Object { $_.Trim() -ne "" } | ForEach-Object { $_.Trim() -replace '^[-*] ', '' }) -join "`n"
if ($notes -eq "") { Fail "la section « ## $Version » de CHANGELOG.md est vide" }

if (-not $NoGit) {
    # Seul le journal peut être en attente : tout le reste doit déjà être commité.
    $pending = @(git status --porcelain | Where-Object { $_ -notmatch 'CHANGELOG\.md$' })
    if ($pending.Count -gt 0) { Fail "des modifications ne sont pas commitées :`n$($pending -join "`n")" }
    if (git tag --list "v$Version") { Fail "l'étiquette v$Version existe déjà" }
}

# Publication : on s'assure d'avoir de quoi publier avant de passer quatre minutes à compiler.
$token = ""
$repo = ""
if ($publish) {
    $repo = [regex]::Match([IO.File]::ReadAllText("$root\crates\pacewall\src\update.rs", $utf8), 'pub const REPO: &str = "([^"]+)"').Groups[1].Value
    if ($repo -eq "") { Fail "dépôt GitHub introuvable dans crates\pacewall\src\update.rs (constante REPO)" }
    # git lit sa question sur l'entrée standard, avec des fins de ligne simples : on passe par un fichier.
    $ask = [IO.Path]::GetTempFileName()
    [IO.File]::WriteAllText($ask, "protocol=https`nhost=github.com`n`n")
    $answer = cmd /c "git credential fill < `"$ask`""
    Remove-Item $ask -Force
    $token = ($answer | Where-Object { $_ -like "password=*" } | Select-Object -First 1) -replace '^password=', ''
    if (-not $token) { Fail "git n'a pas d'identifiant GitHub sur cet ordinateur : fais un « git push » une fois, ou relance avec -NoPublish" }
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
    # Rien à commiter si le numéro et le journal étaient déjà enregistrés.
    git diff --cached --quiet
    if ($LASTEXITCODE -ne 0) { git commit -q -m "Version $Version" }
    git tag "v$Version"
}

$page = ""
if ($publish) {
    Step "Envoi sur GitHub"
    git push --quiet
    if ($LASTEXITCODE -ne 0) { Fail "envoi du code refusé (git push)" }
    git push --quiet origin "v$Version"
    if ($LASTEXITCODE -ne 0) { Fail "envoi de l'étiquette refusé" }

    Step "Version sur GitHub"
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    $headers = @{ Authorization = "Bearer $token"; Accept = "application/vnd.github+json"; "User-Agent" = "PaceWall-release" }
    $json = [ordered]@{ tag_name = "v$Version"; name = "PaceWall $Version"; body = $body; draft = $false; prerelease = $false } | ConvertTo-Json
    try {
        $release = Invoke-RestMethod -Method Post -Uri "https://api.github.com/repos/$repo/releases" -Headers $headers -ContentType "application/json; charset=utf-8" -Body $utf8.GetBytes($json)
        $upload = $release.upload_url -replace '\{.*$', ''
        # L'installeur d'abord : tant que latest.json n'est pas en ligne, les PaceWall installés ne voient pas la version.
        Invoke-RestMethod -Method Post -Uri "${upload}?name=$name" -Headers $headers -ContentType "application/octet-stream" -InFile "$root\dist\$name" | Out-Null
        Invoke-RestMethod -Method Post -Uri "${upload}?name=latest.json" -Headers $headers -ContentType "application/json" -InFile "$root\dist\latest.json" | Out-Null
    }
    catch { Fail "publication refusée par GitHub : $($_.Exception.Message)`nLes fichiers sont dans dist\ : la version peut être créée à la main sur https://github.com/$repo/releases/new?tag=v$Version" }
    $page = $release.html_url

    # Contrôle depuis l'extérieur, à l'adresse que lisent les PaceWall installés.
    try {
        $seen = Invoke-RestMethod -Uri "https://github.com/$repo/releases/latest/download/latest.json?t=$([DateTimeOffset]::UtcNow.ToUnixTimeSeconds())"
        if ($seen.version -ne $Version -or $seen.sha256 -ne $hash) { Write-Host "ATTENTION : l'adresse des mises à jour annonce encore la version $($seen.version)." -ForegroundColor Yellow }
    }
    catch { Write-Host "ATTENTION : l'adresse des mises à jour ne répond pas encore ($($_.Exception.Message))." -ForegroundColor Yellow }
}
$token = ""

Write-Host ""
if ($publish) {
    Write-Host "Version $Version publiée : $page" -ForegroundColor Green
    Write-Host "  $name  ($size Mo)"
    Write-Host "  latest.json  (sha256 $hash)"
    Write-Host ""
    Write-Host "Les PaceWall déjà installés proposeront la mise à jour dans les 12 heures (ou tout de suite via"
    Write-Host "Réglages > Chercher maintenant)."
}
else {
    Write-Host "Version $Version prête dans dist\ (non publiée) :" -ForegroundColor Green
    Write-Host "  $name  ($size Mo)"
    Write-Host "  latest.json  (sha256 $hash)"
}
