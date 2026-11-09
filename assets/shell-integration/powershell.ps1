# Installed in this process only, after PowerShell loads the user's profiles.
if ($env:EDOLTERM_RESTORE_CWD) {
    Set-Location -LiteralPath $env:EDOLTERM_RESTORE_CWD -ErrorAction SilentlyContinue
    Remove-Item Env:EDOLTERM_RESTORE_CWD
}
$global:__EdoltermOriginalPrompt = $function:prompt
function global:prompt {
    $edoltermLastCommandSucceeded = $?
    $edoltermLastExitCode = $global:LASTEXITCODE
    $edoltermPrompt = if ($edoltermLastCommandSucceeded) {
        & $global:__EdoltermOriginalPrompt
    } else {
        # Preserve $? for prompts which display the preceding command status.
        # Ignore prevents a synthetic entry from being added to $Error.
        Write-Error 'Previous command failed' -ErrorAction Ignore
        & $global:__EdoltermOriginalPrompt
    }
    $edoltermLocation = $executionContext.SessionState.Path.CurrentLocation
    if ($edoltermLocation.Provider.Name -eq 'FileSystem') {
        [Console]::Write("$([char]27)]9;9;`"$($edoltermLocation.ProviderPath)`"$([char]27)\")
    }
    $global:LASTEXITCODE = $edoltermLastExitCode
    $edoltermPrompt
}
