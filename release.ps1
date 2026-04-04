param(
    [Parameter(Position = 0)]
    [string]$Version,

    [Parameter(Position = 1)]
    [string]$Targets,

    [Alias("h")]
    [switch]$Help
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Show-Help {
    Write-Host "Usage: .\release.ps1 [VERSION] [TARGETS]"
    Write-Host "  VERSION: The new version string (e.g., 1.2.0)"
    Write-Host "  TARGETS: Optional comma-separated list of targets: js, py, rs. If omitted, releases all."
    Write-Host ""
    Write-Host "Example All: .\release.ps1 1.2.0"
    Write-Host "Example Selective: .\release.ps1 1.2.1 js,py"
}

function Invoke-External {
    param(
        [Parameter(Mandatory = $true)]
        [string]$FilePath,

        [Parameter(ValueFromRemainingArguments = $true)]
        [string[]]$ArgumentList
    )

    & $FilePath @ArgumentList
    if ($LASTEXITCODE -ne 0) {
        throw "Command failed: $FilePath $($ArgumentList -join ' ')"
    }
}

function Replace-FirstMatch {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,

        [Parameter(Mandatory = $true)]
        [string]$Pattern,

        [Parameter(Mandatory = $true)]
        [string]$Replacement
    )

    $content = Get-Content -Raw -Path $Path
    $regex = [System.Text.RegularExpressions.Regex]::new(
        $Pattern,
        [System.Text.RegularExpressions.RegexOptions]::Multiline
    )
    $updated = $regex.Replace($content, $Replacement, 1)

    if ($content -eq $updated) {
        throw "No match found in $Path for pattern: $Pattern"
    }

    [System.IO.File]::WriteAllText((Resolve-Path $Path), $updated, [System.Text.UTF8Encoding]::new($false))
}

function Get-SelectedTargets {
    param([string]$RawTargets)

    if ([string]::IsNullOrWhiteSpace($RawTargets)) {
        return @()
    }

    $selected = $RawTargets.Split(",") | ForEach-Object { $_.Trim() } | Where-Object { $_ }
    $valid = @("js", "py", "rs")
    $invalid = $selected | Where-Object { $_ -notin $valid }

    if ($invalid.Count -gt 0) {
        throw "Invalid target(s): $($invalid -join ', '). Valid targets are: js, py, rs."
    }

    return $selected
}

function Should-Update {
    param(
        [string[]]$SelectedTargets,
        [string]$Target
    )

    return $SelectedTargets.Count -eq 0 -or $Target -in $SelectedTargets
}

if ($Help -or $Version -eq "--help") {
    Show-Help
    exit 0
}

if ([string]::IsNullOrWhiteSpace($Version)) {
    Show-Help
    exit 1
}

$selectedTargets = Get-SelectedTargets -RawTargets $Targets
$dirty = & git status --porcelain
if ($LASTEXITCODE -ne 0) {
    throw "Failed to inspect git status."
}

if (-not [string]::IsNullOrWhiteSpace(($dirty | Out-String))) {
    Write-Error "Git directory is dirty. Please commit or stash changes first."
    exit 1
}

$updatedFiles = [System.Collections.Generic.List[string]]::new()
Write-Host "Preparing release for version $Version..."

if (Should-Update -SelectedTargets $selectedTargets -Target "rs") {
    Replace-FirstMatch -Path "Cargo.toml" -Pattern '(?ms)(^\[workspace\.package\]\r?\n(?:.*\r?\n)*?^version = ").*?(")' -Replacement "`${1}$Version`${2}"
    Invoke-External cargo generate-lockfile
    $updatedFiles.Add("Cargo.toml")
    $updatedFiles.Add("Cargo.lock")
    Write-Host "Updated Cargo.toml"
}

if (Should-Update -SelectedTargets $selectedTargets -Target "js") {
    Push-Location "crates/bindings/enki-js"
    try {
        Invoke-External npm install --no-save
        Invoke-External npm version $Version --no-git-tag-version
    }
    finally {
        Pop-Location
    }

    $updatedFiles.Add("crates/bindings/enki-js/package.json")
    $updatedFiles.Add("crates/bindings/enki-js/package-lock.json")
    Write-Host "Updated crates/bindings/enki-js/package.json"
}

if (Should-Update -SelectedTargets $selectedTargets -Target "py") {
    Replace-FirstMatch -Path "crates/bindings/enki-py/Cargo.toml" -Pattern '(?ms)(^\[package\]\r?\n(?:.*\r?\n)*?^version = ").*?(")' -Replacement "`${1}$Version`${2}"
    Invoke-External cargo generate-lockfile
    $updatedFiles.Add("crates/bindings/enki-py/Cargo.toml")
    $updatedFiles.Add("Cargo.lock")
    Write-Host "Updated crates/bindings/enki-py/Cargo.toml for Python"
}

if ($updatedFiles.Count -eq 0) {
    throw "No files selected for update."
}

Invoke-External git add @updatedFiles

if ([string]::IsNullOrWhiteSpace($Targets)) {
    Invoke-External git commit -m "chore: release $Version"
}
else {
    Invoke-External git commit -m "chore: release $Version ($Targets)"
}

if ($selectedTargets.Count -eq 0) {
    Invoke-External git tag "v$Version"
    Write-Host "Created global tag: v$Version"
}
else {
    foreach ($target in $selectedTargets) {
        Invoke-External git tag "$target-v$Version"
        Write-Host "Created selective tag: $target-v$Version"
    }
}

$currentBranch = (& git branch --show-current).Trim()
if ($LASTEXITCODE -ne 0) {
    throw "Failed to determine current branch."
}

Invoke-External git push origin $currentBranch --tags
Write-Host "Release $Version dispatched to GitHub from branch $currentBranch"
