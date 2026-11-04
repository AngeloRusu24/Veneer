Set-StrictMode -Version Latest
<#
.SYNOPSIS
  Samples CPU and memory of a running Veneer process while idle.
.EXAMPLE
  .\scripts\measure.ps1 -Seconds 60
#>
param(
    [string]$ProcessName = "veneer-desktop",
    [int]$Seconds = 60,
    [int]$IntervalSeconds = 2
)

$proc = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $proc) { throw "Process '$ProcessName' is not running." }

$cores = [Environment]::ProcessorCount
$samples = @()
$prev = $proc.TotalProcessorTime
for ($t = 0; $t -lt $Seconds; $t += $IntervalSeconds) {
    Start-Sleep -Seconds $IntervalSeconds
    $proc.Refresh()
    $now = $proc.TotalProcessorTime
    $cpu = ($now - $prev).TotalSeconds / $IntervalSeconds / $cores * 100
    $prev = $now
    $samples += [pscustomobject]@{ CpuPercent = $cpu; WorkingSetMB = $proc.WorkingSet64 / 1MB; PrivateMB = $proc.PrivateMemorySize64 / 1MB }
}

[pscustomobject]@{
    Samples        = $samples.Count
    CpuAvgPercent  = [math]::Round(($samples | Measure-Object CpuPercent -Average).Average, 3)
    CpuMaxPercent  = [math]::Round(($samples | Measure-Object CpuPercent -Maximum).Maximum, 3)
    WorkingSetMB   = [math]::Round(($samples | Measure-Object WorkingSetMB -Average).Average, 1)
    PrivateMB      = [math]::Round(($samples | Measure-Object PrivateMB -Average).Average, 1)
} | Format-List
