<#
.SYNOPSIS
  NFR-03 gate: fail if one 128-sample block takes longer than the budget.
  Reads criterion's estimates for every `process_128/*` benchmark.
#>
[CmdletBinding()]
param(
    [double] $BudgetNs = 270000,
    [string] $CriterionDir = 'target/criterion/process_128'
)
$ErrorActionPreference = 'Stop'
$files = Get-ChildItem -Path $CriterionDir -Recurse -Filter estimates.json | Where-Object { $_.Directory.Name -eq 'new' }
if (-not $files) { throw "No criterion results under $CriterionDir" }
$failed = $false
foreach ($f in $files) {
    $bench = $f.Directory.Parent.Name
    $est = Get-Content $f.FullName -Raw | ConvertFrom-Json
    $mean = [double]$est.mean.point_estimate
    $upper = [double]$est.mean.confidence_interval.upper_bound
    $pct = [math]::Round(100 * $upper / $BudgetNs, 1)
    Write-Output ("{0,-12} mean {1,10:N0} ns  (upper CI {2:N0} ns = {3}% of {4:N0} ns budget)" -f $bench, $mean, $upper, $pct, $BudgetNs)
    if ($upper -ge $BudgetNs) {
        Write-Output "::error::NFR-03 violated by $bench"
        $failed = $true
    }
}
if ($failed) { exit 1 }
