# STL Editor installer (per-user, no admin needed).
# Installs the Python packages, creates Desktop and Start Menu shortcuts, opens .stlx projects
# with STL Editor and adds "Edit in STL Editor" to the right-click menu of STL/3MF/OBJ files.
$ErrorActionPreference = 'Stop'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$py = (Get-Command python -ErrorAction SilentlyContinue).Source
if (-not $py) { throw 'Python 3 was not found. Install it from python.org first.' }
$pyw = Join-Path (Split-Path $py) 'pythonw.exe'
if (-not (Test-Path $pyw)) { $pyw = $py }
$main = Join-Path $here 'main.py'
$ico = Join-Path $here 'stl_editor.ico'

Write-Host 'Installing Python packages...'
& $py -m pip install --user --quiet --disable-pip-version-check -r (Join-Path $here 'requirements.txt')

Write-Host 'Creating shortcuts...'
$shell = New-Object -ComObject WScript.Shell
$targets = @(
  (Join-Path ([Environment]::GetFolderPath('Desktop')) 'STL Editor.lnk'),
  (Join-Path ([Environment]::GetFolderPath('Programs')) 'STL Editor.lnk')
)
foreach ($t in $targets) {
  $s = $shell.CreateShortcut($t)
  $s.TargetPath = $pyw
  $s.Arguments = "`"$main`""
  $s.WorkingDirectory = $here
  $s.IconLocation = "$ico,0"
  $s.Description = 'STL Editor - CAD tools for STL files'
  $s.Save()
}

Write-Host 'Registering file types...'
$cmd = "`"$pyw`" `"$main`" `"%1`""
$base = 'HKCU:\Software\Classes'
New-Item -Path "$base\.stlx" -Force | Out-Null
Set-ItemProperty -Path "$base\.stlx" -Name '(default)' -Value 'STLEditor.Project'
New-Item -Path "$base\STLEditor.Project\DefaultIcon" -Force | Out-Null
Set-ItemProperty -Path "$base\STLEditor.Project" -Name '(default)' -Value 'STL Editor Project'
Set-ItemProperty -Path "$base\STLEditor.Project\DefaultIcon" -Name '(default)' -Value "$ico,0"
New-Item -Path "$base\STLEditor.Project\shell\open\command" -Force | Out-Null
Set-ItemProperty -Path "$base\STLEditor.Project\shell\open\command" -Name '(default)' -Value $cmd
foreach ($ext in '.stl', '.3mf', '.obj') {
  $k = "$base\SystemFileAssociations\$ext\shell\STLEditor"
  New-Item -Path "$k\command" -Force | Out-Null
  Set-ItemProperty -Path $k -Name '(default)' -Value 'Edit in STL Editor'
  Set-ItemProperty -Path $k -Name 'Icon' -Value "$ico,0"
  Set-ItemProperty -Path "$k\command" -Name '(default)' -Value $cmd
}
Write-Host 'Done. Start STL Editor from the Desktop shortcut.'
