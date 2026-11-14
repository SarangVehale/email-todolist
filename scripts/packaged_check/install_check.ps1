# Installs the built installer into a temporary folder (no admin), starts it, uninstalls it, and says what each step did. Windows only.
$ErrorActionPreference = "Continue"
$root = Split-Path (Split-Path $PSScriptRoot)
$installer = Get-ChildItem (Join-Path $root "app\src-tauri\target\release\bundle\nsis") -Filter "*-setup.exe" | Sort-Object LastWriteTime | Select-Object -Last 1 -ExpandProperty FullName
if (-not $installer) { throw "no installer: build the app first (cd app; npm run tauri build)" }
$dir = Join-Path $env:TEMP "inbox-install-test"
if (Test-Path $dir) { Remove-Item $dir -Recurse -Force }
$sw = [Diagnostics.Stopwatch]::StartNew()
$p = Start-Process -FilePath $installer -ArgumentList "/S", "/D=$dir" -PassThru -Wait
"installer exit code: $($p.ExitCode) in $([math]::Round($sw.Elapsed.TotalSeconds,1))s"
$files = if (Test-Path $dir) { Get-ChildItem $dir -Recurse -File | Select -ExpandProperty Name } else { @() }
"installed files: " + ($files -join ", ")
$exe = Get-ChildItem $dir -Filter *.exe -ErrorAction SilentlyContinue | Where-Object { $_.Name -notmatch "uninstall" } | Select -First 1
"main exe: " + $exe.Name + "  " + [math]::Round($exe.Length/1MB,2) + " MB"
$reg = Get-ChildItem "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall" -ErrorAction SilentlyContinue | Where-Object { (Get-ItemProperty $_.PSPath).DisplayName -like "*Inbox Agent*" }
"uninstall entry (per-user, no admin): " + [bool]$reg
if ($reg) { $v = Get-ItemProperty $reg.PSPath; "  name=$($v.DisplayName) version=$($v.DisplayVersion) publisher=$($v.Publisher)" }
$lnk = Get-ChildItem "$env:APPDATA\Microsoft\Windows\Start Menu\Programs" -Recurse -Filter "Inbox Agent*.lnk" -ErrorAction SilentlyContinue
"start menu shortcut: " + [bool]$lnk
# the installed program starts and stays up
$proc = Start-Process $exe.FullName -ArgumentList "--minimized" -PassThru
Start-Sleep 6
"installed app running after 6s: " + (-not $proc.HasExited)
Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
Get-Process app, "Inbox Agent" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep 2
# uninstall
$un = Get-ChildItem $dir -Filter "uninstall*.exe" | Select -First 1
if ($un) { $u = Start-Process $un.FullName -ArgumentList "/S" -PassThru -Wait; "uninstaller exit code: $($u.ExitCode)" }
Start-Sleep 4
"install dir removed: " + (-not (Test-Path ($dir + "\" + $exe.Name)))
$reg2 = Get-ChildItem "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall" -ErrorAction SilentlyContinue | Where-Object { (Get-ItemProperty $_.PSPath).DisplayName -like "*Inbox Agent*" }
"uninstall entry removed: " + (-not $reg2)
$lnk2 = Get-ChildItem "$env:APPDATA\Microsoft\Windows\Start Menu\Programs" -Recurse -Filter "Inbox Agent*.lnk" -ErrorAction SilentlyContinue
"start menu shortcut removed: " + (-not $lnk2)
if (Test-Path $dir) { Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue }
