$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true') {
    throw 'Run this installation check only on an isolated GitHub-hosted Windows runner.'
}
$installRoot = [IO.Path]::GetFullPath((Join-Path $env:RUNNER_TEMP 'tg-grab-install-check'))
$tempRoot = [IO.Path]::GetFullPath($env:RUNNER_TEMP).TrimEnd('\') + '\'
if (-not $installRoot.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Installation check must stay inside the runner temporary directory.'
}
$installer = @(Get-ChildItem src-tauri/target/release/bundle/nsis/*-setup.exe)
if ($installer.Count -ne 1) { throw 'Expected exactly one installer.' }
function Install-Preview {
    $result = Start-Process -FilePath $installer[0].FullName -ArgumentList @('/S', "/D=$installRoot") -WindowStyle Hidden -PassThru -Wait
    if ($result.ExitCode -ne 0) { throw "Installer failed with exit code $($result.ExitCode)" }
    if (-not (Test-Path -LiteralPath (Join-Path $installRoot 'tg-grab.exe'))) {
        throw 'Installed application is missing.'
    }
}
Install-Preview
$dataRoot = Join-Path $env:APPDATA 'com.bryan.tg-grab'
New-Item -ItemType Directory -Path $dataRoot -Force | Out-Null
$marker = Join-Path $dataRoot 'release-reinstall-check.txt'
'preserve-local-data' | Set-Content -LiteralPath $marker -Encoding utf8NoBOM
Install-Preview
if ((Get-Content -LiteralPath $marker -Raw).Trim() -ne 'preserve-local-data') {
    throw 'Reinstall did not preserve local application data.'
}
$app = Start-Process -FilePath (Join-Path $installRoot 'tg-grab.exe') -WindowStyle Hidden -PassThru
try {
    Start-Sleep -Seconds 5
    $app.Refresh()
    if ($app.HasExited) { throw "Installed app exited early: $($app.ExitCode)" }
} finally {
    if (-not $app.HasExited) { Stop-Process -Id $app.Id }
}
Write-Output 'Fresh install, reinstall data preservation, and application startup passed.'
