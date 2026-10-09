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
    # Report only processes this script started, so a timeout ends ours and never the user's.
    $before = @(Get-Process POWERPNT -ErrorAction SilentlyContinue | ForEach-Object Id)
    $application = New-Object -ComObject PowerPoint.Application
    # COM launches carry -Embedding; a window the user opens meanwhile does not.
    $launched = @(Get-CimInstance Win32_Process -Filter "Name='POWERPNT.EXE'" -ErrorAction SilentlyContinue |
        Where-Object { $before -notcontains $_.ProcessId -and $_.CommandLine -match '-Embedding' } | ForEach-Object ProcessId)
    foreach ($id in $launched) {
        [Console]::Out.WriteLine("SLATE_OFFICE_STARTED=$id")
    }
    [Console]::Out.Flush()
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
