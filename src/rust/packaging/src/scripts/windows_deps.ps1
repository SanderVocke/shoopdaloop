$executable = "$args[0]"
$handled = @()
if (-not (Get-Command -Name Dependencies.exe -ErrorAction SilentlyContinue)) {
    echo "Error: Dependencies.exe not found in PATH."; exit 1
}
$output = & Dependencies.exe -chain -depth 4 $executable.Trim();
$output | Where-Object { -not ($_ -match "NotFound") } |
    Get-Unique |
    ForEach-Object { " $_" -replace "[|├]", " " -replace "([^ ]+).* : ", "$1" } |
    ForEach-Object {
        if ($handled -contains $_.Trim()) {
            return
        }
        $handled += $_.Trim()
        Write-Output $_
    }
$output | Where-Object { ($_ -match "NotFound") } |
    Get-Unique |
    ForEach-Object { Write-Error $_ }