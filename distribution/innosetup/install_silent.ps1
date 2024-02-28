# For installing in a CI flow while reporting log output
# pass the installer and a log filename.

$pjob = Start-Job -ScriptBlock { & "$1 /SILENT /SUPPRESSMSGBOXES /LOG=$2" }
$ljob = Start-Job -ScriptBlock { Get-Content "$2" -Wait }

while ($pjob.State -eq 'Running' -and $ljob.HasMoreData) {
  Receive-Job $ljob
  Start-Sleep -Milliseconds 200
}
Receive-Job $ljob

Stop-Job $ljob

Remove-Job $ljob
Remove-Job $pjob
