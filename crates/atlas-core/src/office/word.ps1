$ErrorActionPreference = 'Stop'
$application = $null
$document = $null
$previousSecurity = $null
$hadWord = @(Get-Process WINWORD -ErrorAction SilentlyContinue).Count -gt 0
try {
    # Defense in depth if the source dehydrates between the Rust guard and Open.
    $attributes = [uint32][System.IO.File]::GetAttributes($env:SLATE_WORD_SOURCE)
    if (($attributes -band 0x00441000) -ne 0) { throw 'The source is cloud-only; make it available locally first.' }
    $application = New-Object -ComObject Word.Application
    $previousSecurity = $application.AutomationSecurity
    $application.AutomationSecurity = 3 # msoAutomationSecurityForceDisable
    $application.Visible = $false
    $application.DisplayAlerts = 0 # wdAlertsNone
    $confirmConversions = $false
    $readOnly = $true
    $addToRecent = $false
    $document = $application.Documents.Open($env:SLATE_WORD_SOURCE, $confirmConversions, $readOnly, $addToRecent)
    # wdFormatPDF: only the derived destination is written.
    $document.SaveAs2($env:SLATE_WORD_PDF, 17)
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
} finally {
    if ($null -ne $document) {
        try { $document.Close(0) } catch {}
        [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($document)
    }
    if ($null -ne $application) {
        try {
            if ($null -ne $previousSecurity) { $application.AutomationSecurity = $previousSecurity }
            if (-not $hadWord -and $application.Documents.Count -eq 0) { $application.Quit() }
        } catch {}
        [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($application)
    }
}
