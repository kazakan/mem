# Fills the winget manifest templates for one GitHub release and validates them (RELEASING.md, "winget").
# usage: pwsh packaging/winget/prepare.ps1 -Version 0.0.1
# Writes <Out>/manifests/k/kazakan/mem/<Version>/ (the folder layout of microsoft/winget-pkgs) with the release's
# installer URLs, SHA-256 hashes (from the release's SHA256SUMS) and date, then runs `winget validate` on it.
param(
    [Parameter(Mandatory = $true)][string]$Version,
    [string]$Out = (Join-Path $PSScriptRoot '..\..\target\winget'),
    [string]$ReleaseDate = (Get-Date -Format 'yyyy-MM-dd'),
    # a local SHA256SUMS instead of the release's copy (for testing)
    [string]$SumsFile
)
$ErrorActionPreference = 'Stop'
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "version must look like 0.0.1, got '$Version'" }
$tag = "v$Version"
$base = "https://github.com/kazakan/mem/releases/download/$tag"

if (-not $SumsFile) {
    $SumsFile = Join-Path ([IO.Path]::GetTempPath()) "mem-$tag-SHA256SUMS"
    Invoke-WebRequest "$base/SHA256SUMS" -OutFile $SumsFile -UseBasicParsing
}
# sha256sum lines: "<hash>  <file>" (a '*' before the name marks binary mode)
$hash = @{}
foreach ($line in Get-Content $SumsFile) {
    if ($line -match '^([0-9a-fA-F]{64})\s+\*?(\S+)$') { $hash[$Matches[2]] = $Matches[1].ToUpper() }
}

$dir = Join-Path $Out "manifests\k\kazakan\mem\$Version"
New-Item -ItemType Directory -Force $dir | Out-Null
$utf8 = New-Object System.Text.UTF8Encoding($false)
foreach ($t in Get-ChildItem (Join-Path $PSScriptRoot 'kazakan.mem*.yaml')) {
    $file = $null
    $lines = foreach ($l in [IO.File]::ReadAllLines($t.FullName)) {
        if ($l -match '^PackageVersion:') { $l = "PackageVersion: $Version" }
        elseif ($l -match '^ReleaseDate:') { $l = "ReleaseDate: $ReleaseDate" }
        elseif ($l -match '^(\s*InstallerUrl:\s*).*/(mem-[0-9.]+-(.+))$') {
            $file = "mem-$Version-$($Matches[3])"
            $l = "$($Matches[1])$base/$file"
        }
        elseif ($l -match '^(\s*InstallerSha256:\s*)') {
            if (-not $hash.ContainsKey($file)) { throw "no SHA-256 for $file in $SumsFile" }
            $l = "$($Matches[1])$($hash[$file])"
        }
        elseif ($l -match '^ReleaseNotesUrl:') { $l = "ReleaseNotesUrl: https://github.com/kazakan/mem/releases/tag/$tag" }
        $l
    }
    [IO.File]::WriteAllText((Join-Path $dir $t.Name), (($lines -join "`n") + "`n"), $utf8)
}

Write-Host "manifests written to $dir"
winget validate --manifest $dir
if ($LASTEXITCODE -ne 0) { Write-Host "winget validate reported errors or warnings (exit $LASTEXITCODE)"; exit 1 }
