# Installs the built installer into a temporary folder (no admin), starts it, uninstalls it, and says what each step did. Windows only.
#
# It REFUSES TO RUN if Inbox Agent is already installed for this user or has a data folder (%LOCALAPPDATA%\app.inboxagent.desktop): the installer
# and the uninstaller use the same per-user entry and Start Menu shortcut as a real install, and the program it starts writes that data folder,
# which this check deletes at the end. Uninstall the real one (and move its data folder away) first.
$ErrorActionPreference = "Continue"
$root = Split-Path (Split-Path $PSScriptRoot)
$installer = Get-ChildItem (Join-Path $root "app\src-tauri\target\release\bundle\nsis") -Filter "*-setup.exe" | Sort-Object LastWriteTime | Select-Object -Last 1 -ExpandProperty FullName
if (-not $installer) { throw "no installer: build the app first (cd app; npm run tauri build)" }

$uninstallKeys = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall"
$entry = { Get-ChildItem $uninstallKeys -ErrorAction SilentlyContinue | Where-Object { (Get-ItemProperty $_.PSPath).DisplayName -like "*Inbox Agent*" } }
$data = Join-Path $env:LOCALAPPDATA "app.inboxagent.desktop"
if (& $entry) { throw "Inbox Agent is installed for this user: this check would install over it and then uninstall it. Uninstall it first." }
if (Test-Path $data) { throw "$data exists: this check would delete it (the program it starts writes there). Close the app and move that folder away first." }

$dir = Join-Path $env:TEMP "inbox-install-test"
if (Test-Path $dir) { Remove-Item $dir -Recurse -Force }
$sw = [Diagnostics.Stopwatch]::StartNew()
$p = Start-Process -FilePath $installer -ArgumentList "/S", "/D=$dir" -PassThru -Wait
"installer exit code: $($p.ExitCode) in $([math]::Round($sw.Elapsed.TotalSeconds,1))s"
$files = if (Test-Path $dir) { Get-ChildItem $dir -Recurse -File | Select -ExpandProperty Name } else { @() }
"installed files: " + ($files -join ", ")
# the licences travel with the program (MIT and Apache ask for that): the installer carries these three
$licences = @("LICENSE", "THIRD_PARTY_NOTICES.md", "THIRD_PARTY_LICENSES.txt") | Where-Object { -not (Get-ChildItem $dir -Recurse -File -Filter $_ -ErrorAction SilentlyContinue) }
"licence files installed: " + (-not $licences) + $(if ($licences) { " (missing: " + ($licences -join ", ") + ")" } else { "" })
$exe = Get-ChildItem $dir -Filter *.exe -ErrorAction SilentlyContinue | Where-Object { $_.Name -notmatch "uninstall" } | Select -First 1
"main exe: " + $exe.Name + "  " + [math]::Round($exe.Length/1MB,2) + " MB"
$reg = & $entry
"uninstall entry (per-user, no admin): " + [bool]$reg
if ($reg) { $v = Get-ItemProperty $reg.PSPath; "  name=$($v.DisplayName) version=$($v.DisplayVersion) publisher=$($v.Publisher)" }
$lnk = Get-ChildItem "$env:APPDATA\Microsoft\Windows\Start Menu\Programs" -Recurse -Filter "Inbox Agent*.lnk" -ErrorAction SilentlyContinue
"start menu shortcut: " + [bool]$lnk
# the installed program starts and stays up (its keychain is in memory for this run: it must not read or write the real one)
$env:INBOX_AGENT_KEYCHAIN = "memory"
$proc = Start-Process $exe.FullName -ArgumentList "--minimized" -PassThru
Start-Sleep 6
"installed app running after 6s: " + (-not $proc.HasExited)
Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
# only what runs from the temporary folder is stopped: not another program that happens to be called "app", and not a real Inbox Agent
Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.Path -and $_.Path.StartsWith($dir, [StringComparison]::OrdinalIgnoreCase) } | Stop-Process -Force -ErrorAction SilentlyContinue
Remove-Item Env:\INBOX_AGENT_KEYCHAIN -ErrorAction SilentlyContinue
Start-Sleep 2
# uninstall
$un = Get-ChildItem $dir -Filter "uninstall*.exe" | Select -First 1
if ($un) { $u = Start-Process $un.FullName -ArgumentList "/S" -PassThru -Wait; "uninstaller exit code: $($u.ExitCode)" }
Start-Sleep 4
"install dir removed: " + (-not (Test-Path ($dir + "\" + $exe.Name)))
"uninstall entry removed: " + (-not (& $entry))
$lnk2 = Get-ChildItem "$env:APPDATA\Microsoft\Windows\Start Menu\Programs" -Recurse -Filter "Inbox Agent*.lnk" -ErrorAction SilentlyContinue
"start menu shortcut removed: " + (-not $lnk2)
if (Test-Path $dir) { Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue }
# the program wrote a data folder when it started (the guard above made sure there was none before): the browser engine holds it for a few seconds
for ($i = 0; $i -lt 20 -and (Test-Path $data); $i++) { Remove-Item $data -Recurse -Force -ErrorAction SilentlyContinue; if (Test-Path $data) { Start-Sleep 1 } }
"data folder removed: " + (-not (Test-Path $data))
