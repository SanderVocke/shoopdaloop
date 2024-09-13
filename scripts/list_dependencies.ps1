$output = & Dependencies.exe -modules $Args[0] | Sort-Object | Get-Unique
$filteredLines = $output | Select-String "[ApplicationDirectory]|MSVCP|MSVCR|VCRUNTIME"
$dllNames = $output | Where-Object { -not ($_ -match "NotFound") } | Where-Object { -not ($_ -match ".exe") } |  ForEach-Object {
    # Split the line by whitespace and get the last word
    $_.Trim() -split '\s+' | Select-Object -Last 1
} | Sort-Object | Get-Unique

# Output the results
Write-Error "Raw results:"
Write-Error "$output"
Write-Error "Dependencies:"
$dllNames