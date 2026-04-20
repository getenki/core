param(
    [Parameter(Position = 0)]
    [string]$Version,

    [Alias("h")]
    [switch]$Help
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Show-Help {
    Write-Host "Usage: .\release.ps1 [VERSION]"
    Write-Host "  VERSION: The new version string (e.g., 0.5.76)"
}

function Invoke-External {
    param(
        [Parameter(Mandatory = $true)]
        [string]$FilePath,

        [Parameter(ValueFromRemainingArguments = $true)]
        [string[]]$ArgumentList
    )

    $renderedArgs = if ($ArgumentList.Count -gt 0) { $ArgumentList -join " " } else { "" }
    Write-Host ">> $FilePath $renderedArgs".TrimEnd()
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

function Replace-AllMatches {
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
    $updated = $regex.Replace($content, $Replacement)

    if ($content -eq $updated) {
        throw "No match found in $Path for pattern: $Pattern"
    }

    [System.IO.File]::WriteAllText((Resolve-Path $Path), $updated, [System.Text.UTF8Encoding]::new($false))
}

function Get-WorkspaceVersion {
    $content = Get-Content -Raw -Path "Cargo.toml"
    $regex = [System.Text.RegularExpressions.Regex]::new(
        '(?ms)^\[workspace\.package\].*?^version = "([^"]+)"',
        [System.Text.RegularExpressions.RegexOptions]::Multiline
    )
    $match = $regex.Match($content)

    if (-not $match.Success) {
        throw "Unable to determine current workspace version from Cargo.toml."
    }

    return $match.Groups[1].Value
}

if ($Help -or $Version -eq "--help") {
    Show-Help
    exit 0
}

if ([string]::IsNullOrWhiteSpace($Version)) {
    Show-Help
    exit 1
}

$currentVersion = Get-WorkspaceVersion
Write-Host "Updating version references from $currentVersion to $Version..."

Replace-FirstMatch -Path "Cargo.toml" -Pattern '(?ms)(^\[workspace\.package\]\r?\n(?:.*\r?\n)*?^version = ").*?(")' -Replacement "`${1}$Version`${2}"
Replace-FirstMatch -Path "crates/bindings/enki-py/Cargo.toml" -Pattern '(?ms)(^\[package\]\r?\n(?:.*\r?\n)*?^version = ").*?(")' -Replacement "`${1}$Version`${2}"

Replace-AllMatches -Path "README.md" -Pattern "(?m)(enki_next = \{ package = ""enki-next"", version = "")$([regex]::Escape($currentVersion))(""[^`r`n]*\})" -Replacement "`${1}$Version`${2}"
Replace-AllMatches -Path "README.md" -Pattern '(?m)(^- The current workspace version is `).*?(`\.)' -Replacement "`${1}$Version`${2}"

Replace-AllMatches -Path "crates/core/README.md" -Pattern "(?m)(enki_next = \{ package = ""enki-next"", version = "")$([regex]::Escape($currentVersion))(""[^`r`n]*\})" -Replacement "`${1}$Version`${2}"
Replace-AllMatches -Path "docs/enki-doc/docs/rust.md" -Pattern "(?m)(enki_next = \{ package = ""enki-next"", version = "")$([regex]::Escape($currentVersion))(""[^`r`n]*\})" -Replacement "`${1}$Version`${2}"

Replace-FirstMatch -Path "crates/bindings/enki-js/package.json" -Pattern '(?m)(^  "version": ").*?(",$)' -Replacement "`${1}$Version`${2}"
Replace-AllMatches -Path "crates/bindings/enki-js/package.json" -Pattern "((?m)^    ""@getenki/ai-[^""]+"": "")$([regex]::Escape($currentVersion))(""[,]?$)" -Replacement "`${1}$Version`${2}"

Invoke-External cargo generate-lockfile

Write-Host "Updated:"
Write-Host "  Cargo.toml"
Write-Host "  Cargo.lock"
Write-Host "  README.md"
Write-Host "  crates/core/README.md"
Write-Host "  docs/enki-doc/docs/rust.md"
Write-Host "  crates/bindings/enki-py/Cargo.toml"
Write-Host "  crates/bindings/enki-js/package.json"
