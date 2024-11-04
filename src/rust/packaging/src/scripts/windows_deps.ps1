$executable = "$args[0]"
$output = & Dependencies.exe -chain -depth 4 $executable.Trim();
$output | Where-Object { -not ($_ -match "NotFound") } |
    Get-Unique |
    ForEach-Object { " $_" -replace "[|├]", " " -replace "([^ ]+).* : ", "$1" } |
    ForEach-Object { Write-Output $_ }
$output | Where-Object { ($_ -match "NotFound") } |
    Get-Unique |
    ForEach-Object { " $_" -replace "[|├]", " " -replace "([^ ]+).* : ", "$1" } |
    ForEach-Object { Write-Error $_ }