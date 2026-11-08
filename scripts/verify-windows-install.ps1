$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true') {
    throw 'Run this installation check only on an isolated GitHub-hosted Windows runner.'
}
$installRoot = [IO.Path]::GetFullPath((Join-Path $env:RUNNER_TEMP 'tg-grab-install-check'))
$tempRoot = [IO.Path]::GetFullPath($env:RUNNER_TEMP).TrimEnd('\') + '\'
if (-not $installRoot.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Installation check must stay inside the runner temporary directory.'
}
$version = (Get-Content package.json -Raw | ConvertFrom-Json).version
$installer = Get-Item "src-tauri/target/release/bundle/nsis/tg-grab_${version}_x64-setup.exe"
function Install-Preview([string]$Path = $installer.FullName, [string[]]$Options = @()) {
    $result = Start-Process -FilePath $Path -ArgumentList (@('/S') + $Options + @("/D=$installRoot")) -WindowStyle Hidden -PassThru -Wait
    if ($result.ExitCode -ne 0) { throw "Installer failed with exit code $($result.ExitCode)" }
    if (-not (Test-Path -LiteralPath (Join-Path $installRoot 'tg-grab.exe'))) {
        throw 'Installed application is missing.'
    }
}
$shortcutShell = New-Object -ComObject WScript.Shell
$desktopShortcut = Join-Path ([Environment]::GetFolderPath('Desktop')) 'tg-grab.lnk'
$startShortcut = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\tg-grab.lnk'
function Assert-BrandShortcuts {
    $icon = Join-Path $installRoot "branding/tg-grab-$version.ico"
    if ((Get-FileHash -LiteralPath $icon).Hash -ne (Get-FileHash src-tauri/icons/icon.ico).Hash) {
        throw 'Installed shortcut icon differs from the canonical brand asset.'
    }
    foreach ($path in @($desktopShortcut, $startShortcut)) {
        if (-not (Test-Path -LiteralPath $path)) { throw "Shortcut is missing: $path" }
        $shortcut = $shortcutShell.CreateShortcut($path)
        if ($shortcut.TargetPath -ne (Join-Path $installRoot 'tg-grab.exe')) { throw 'Shortcut target changed.' }
        if ($shortcut.IconLocation -ne "$icon,0") { throw "Shortcut icon was not updated: $path" }
    }
}
Install-Preview
Assert-BrandShortcuts
$dataRoot = Join-Path $env:APPDATA 'com.bryan.tg-grab'
New-Item -ItemType Directory -Path $dataRoot -Force | Out-Null
$marker = Join-Path $dataRoot 'release-reinstall-check.txt'
'preserve-local-data' | Set-Content -LiteralPath $marker -Encoding utf8NoBOM

# Test an actual upgrade from the published first preview, not only reinstall.
$uninstall = Start-Process -FilePath (Join-Path $installRoot 'uninstall.exe') -ArgumentList '/S' -WindowStyle Hidden -PassThru -Wait
if ($uninstall.ExitCode -ne 0) { throw 'Fresh-install cleanup failed.' }
$previousInstaller = Join-Path $env:RUNNER_TEMP 'tg-grab_0.1.0_x64-setup.exe'
Invoke-WebRequest 'https://github.com/R051/tg-grab/releases/download/v0.1.0/tg-grab_0.1.0_x64-setup.exe' -OutFile $previousInstaller
if ((Get-FileHash -LiteralPath $previousInstaller -Algorithm SHA256).Hash.ToLowerInvariant() -ne 'e2f40c91ef021ec73bca69703695647e2b9e6c03a9076f7786bcded0da77c564') {
    throw 'Previous preview checksum differs.'
}
Install-Preview $previousInstaller
Install-Preview
Assert-BrandShortcuts

# Updating existing pins must also leave unrelated shortcuts alone.
$pinnedRoot = Join-Path $env:APPDATA 'Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar'
New-Item -ItemType Directory -Path $pinnedRoot -Force | Out-Null
$testPin = Join-Path $pinnedRoot 'tg-grab-upgrade-check.lnk'
$pin = $shortcutShell.CreateShortcut($testPin)
$pin.TargetPath = Join-Path $installRoot 'tg-grab.exe'
$pin.IconLocation = "$env:SystemRoot\System32\shell32.dll,0"
$pin.Save()
$otherPin = Join-Path $pinnedRoot 'unrelated-upgrade-check.lnk'
$other = $shortcutShell.CreateShortcut($otherPin)
$other.TargetPath = "$env:SystemRoot\System32\notepad.exe"
$other.Save()
$otherHash = (Get-FileHash -LiteralPath $otherPin).Hash
Install-Preview
Assert-BrandShortcuts
if ($shortcutShell.CreateShortcut($testPin).IconLocation -ne "$(Join-Path $installRoot "branding/tg-grab-$version.ico"),0") {
    throw 'Existing pinned shortcut icon was not updated.'
}
if ((Get-FileHash -LiteralPath $otherPin).Hash -ne $otherHash) { throw 'Unrelated pinned shortcut changed.' }
# All paths below belong to this isolated runner's installer test.
Remove-Item -LiteralPath $desktopShortcut, $startShortcut
Install-Preview -Options @('/UPDATE', '/NS')
if ((Test-Path -LiteralPath $desktopShortcut) -or (Test-Path -LiteralPath $startShortcut)) {
    throw 'Update recreated shortcuts the user had removed.'
}
if ((Get-Content -LiteralPath $marker -Raw).Trim() -ne 'preserve-local-data') {
    throw 'Upgrade/reinstall did not preserve local application data.'
}
$app = Start-Process -FilePath (Join-Path $installRoot 'tg-grab.exe') -WindowStyle Hidden -PassThru
try {
    Start-Sleep -Seconds 5
    $app.Refresh()
    if ($app.HasExited) { throw "Installed app exited early: $($app.ExitCode)" }
} finally {
    if (-not $app.HasExited) { Stop-Process -Id $app.Id }
}
Write-Output 'Fresh install, 0.1.0 upgrade, shortcut icons, reinstall data preservation, and application startup passed.'
