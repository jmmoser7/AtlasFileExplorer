[CmdletBinding()]
param([Parameter(Mandatory)][string]$Directory,
    [Parameter(Mandatory)][ValidatePattern('^\d+\.\d+\.\d+(-preview\.\d+)?$')][string]$Version,
    [Parameter(Mandatory)][ValidateSet('stable', 'preview')][string]$Channel,
    [Parameter(Mandatory)][ValidatePattern('^[a-f0-9]{40}$')][string]$Commit)
$ErrorActionPreference = 'Stop'
$repository = 'jmmoser7/AtlasFileExplorer'
$tag = if ($Channel -eq 'stable') { "v$Version" } else { 'preview' }
$title = if ($Channel -eq 'stable') { "Slate & File Atlas $Version" } else { "Slate & File Atlas Preview ($Version)" }
$notes = Join-Path $env:RUNNER_TEMP 'atlas-release-notes.md'
@"
Windows x64 installer for Slate and File Atlas. Download the **Setup.exe** asset once; installed copies check for updates when opened.

Channel: **$Channel** · Version: **$Version** · Source: $Commit

The setup includes both apps, PDFium, and a private Node/npm runtime. It installs WebView2 and the Visual C++ runtime when missing. Optional Cursor/Office integrations still require their own account or installed application.

Workbooks and settings are preserved during updates. Save your work and close all suite windows when an update is ready. See [distribution documentation](https://github.com/$repository/blob/$Commit/docs/distribution.md).
"@ | Set-Content -LiteralPath $notes -Encoding utf8

# Stable releases are created as drafts and exposed only after every asset lands.
# The preview release is a rolling opt-in feed. Upload binaries before its feed.
$existing = gh release view $tag --repo $repository --json id 2>$null
$exists = $LASTEXITCODE -eq 0
if ($Channel -eq 'stable' -and $exists) { throw "Release $tag already exists; use a new version." }
if (-not $exists) {
    $create = @('release', 'create', $tag, '--repo', $repository, '--target', $Commit, '--title', $title, '--notes-file', $notes, '--draft')
    if ($Channel -eq 'preview') { $create += '--prerelease' } else { $create += '--verify-tag' }
    gh @create
    if ($LASTEXITCODE -ne 0) { throw 'Release creation failed.' }
}
$assets = @(Get-ChildItem -LiteralPath $Directory -File)
$feeds = @($assets | Where-Object { $_.Name -match '^releases\..*\.json$|^RELEASES' })
foreach ($asset in @($assets | Where-Object { $_ -notin $feeds }) + $feeds) {
    gh release upload $tag $asset.FullName --repo $repository --clobber
    if ($LASTEXITCODE -ne 0) { throw "Upload failed: $($asset.Name)" }
}
if ($Channel -eq 'preview') {
    # This reserved tag is deliberately movable; stable version tags never move.
    gh api --method PATCH "repos/$repository/git/refs/tags/preview" -f "sha=$Commit" -F force=true
    if ($LASTEXITCODE -ne 0) { throw 'Could not advance the preview tag.' }
    gh release edit $tag --repo $repository --draft=false --prerelease --latest=false --title $title --notes-file $notes
} else {
    gh release edit $tag --repo $repository --draft=false --latest --title $title --notes-file $notes
}
if ($LASTEXITCODE -ne 0) { throw 'Could not publish the release.' }
