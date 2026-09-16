$ErrorActionPreference = 'Stop'
$application = $null
$presentation = $null
$previousSecurity = $null
$previousAlerts = $null
$hadPowerPoint = @(Get-Process POWERPNT -ErrorAction SilentlyContinue).Count -gt 0
try {
    # Defense in depth if the source dehydrates between the Rust guard and Open.
    $attributes = [uint32][System.IO.File]::GetAttributes($env:SLATE_POWERPOINT_SOURCE)
    if (($attributes -band 0x00441000) -ne 0) { throw 'The source is cloud-only; make it available locally first.' }
    $application = New-Object -ComObject PowerPoint.Application
    $previousSecurity = $application.AutomationSecurity
    $previousAlerts = $application.DisplayAlerts
    $application.AutomationSecurity = 3 # msoAutomationSecurityForceDisable
    $application.DisplayAlerts = 1 # ppAlertsNone
    # ReadOnly=true, Untitled=true (independent copy), WithWindow=false.
    $presentation = $application.Presentations.Open($env:SLATE_POWERPOINT_SOURCE, -1, -1, 0)
    # ppSaveAsPDF: only the derived destination is written.
    $presentation.SaveAs($env:SLATE_POWERPOINT_PDF, 32)
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    exit 1
} finally {
    if ($null -ne $presentation) {
        try { $presentation.Close() } catch {}
        [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($presentation)
    }
    if ($null -ne $application) {
        try {
            if ($null -ne $previousSecurity) { $application.AutomationSecurity = $previousSecurity }
            if ($null -ne $previousAlerts) { $application.DisplayAlerts = $previousAlerts }
            if (-not $hadPowerPoint -and $application.Presentations.Count -eq 0) { $application.Quit() }
        } catch {}
        [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($application)
    }
}
