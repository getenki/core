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

if ($Help -or $Version -eq "--help") {
    Show-Help
    exit 0
}

if ([string]::IsNullOrWhiteSpace($Version)) {
    Show-Help
    exit 1
}

Write-Host "Updating version references to $Version..."

Write-Host "Updating Cargo.toml"
Replace-FirstMatch -Path "Cargo.toml" -Pattern '(?m)^(version = ")[^"]+(")\r?$' -Replacement "`${1}$Version`${2}"

Write-Host "Updating crates/bindings/enki-py/Cargo.toml"
Replace-FirstMatch -Path "crates/bindings/enki-py/Cargo.toml" -Pattern '(?m)^(version = ")[^"]+(")\r?$' -Replacement "`${1}$Version`${2}"

Write-Host "Updating README.md"
Replace-AllMatches -Path "README.md" -Pattern '(?m)(enki_next = \{ package = "enki-next", version = ")[^"]+("[^\r\n]*\}\r?$)' -Replacement "`${1}$Version`${2}"
Replace-AllMatches -Path "README.md" -Pattern '(?m)(^- The current workspace version is `).*?(`\.)' -Replacement "`${1}$Version`${2}"

Write-Host "Updating crates/core/README.md"
Replace-AllMatches -Path "crates/core/README.md" -Pattern '(?m)(enki_next = \{ package = "enki-next", version = ")[^"]+("[^\r\n]*\}\r?$)' -Replacement "`${1}$Version`${2}"

Write-Host "Updating docs/enki-doc/docs/rust.md"
Replace-AllMatches -Path "docs/enki-doc/docs/rust.md" -Pattern '(?m)(enki_next = \{ package = "enki-next", version = ")[^"]+("[^\r\n]*\}\r?$)' -Replacement "`${1}$Version`${2}"

Write-Host "Updating crates/bindings/enki-js/package.json"
Replace-FirstMatch -Path "crates/bindings/enki-js/package.json" -Pattern '(?m)(^  "version": ").*?(",$)' -Replacement "`${1}$Version`${2}"
Replace-AllMatches -Path "crates/bindings/enki-js/package.json" -Pattern '(?m)(^    "@getenki/ai-[^"]+": ")[^"]+("[,]?\r?$)' -Replacement "`${1}$Version`${2}"

Write-Host "Regenerating Cargo.lock"
Invoke-External cargo generate-lockfile

Write-Host "Updated:"
Write-Host "  Cargo.toml"
Write-Host "  Cargo.lock"
Write-Host "  README.md"
Write-Host "  crates/core/README.md"
Write-Host "  docs/enki-doc/docs/rust.md"
Write-Host "  crates/bindings/enki-py/Cargo.toml"
Write-Host "  crates/bindings/enki-js/package.json"
