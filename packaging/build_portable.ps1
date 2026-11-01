# Builds the portable (no-install) STL Editor and zips it for sharing.
#   powershell -ExecutionPolicy Bypass -File packaging\build_portable.ps1 [-Repo owner/name] [-OutDir folder]
# GitHub Actions runs this on every v* tag (.github/workflows/release.yml).
# Output: <Documents>\STL Editor - Share\STL Editor Portable <version>.zip
# Release asset name on GitHub: STL-Editor-Windows-portable.zip (stable, so the website's download link always works)
param([string]$Repo = '', [string]$OutDir = '')
$ErrorActionPreference = 'Stop'
$src = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$version = (Select-String -Path "$src\stleditor\__init__.py" -Pattern '__version__ = "(.+)"').Matches[0].Groups[1].Value
$work = Join-Path $env:TEMP 'stle-build'
$out = if ($OutDir) { $OutDir } else { Join-Path ([Environment]::GetFolderPath('MyDocuments')) 'STL Editor - Share' }
Write-Host "Building STL Editor $version"

# clean copy of the source (no caches, no user files)
if (Test-Path $work) { Remove-Item $work -Recurse -Force }
New-Item -ItemType Directory -Force "$work\src" | Out-Null
foreach ($item in 'main.py', 'stleditor', 'web', 'stl_editor.ico', 'USER_GUIDE.md', 'LICENSE', 'packaging') {
  Copy-Item (Join-Path $src $item) "$work\src\" -Recurse -Force
}
Get-ChildItem "$work\src" -Recurse -Directory -Filter __pycache__ | Remove-Item -Recurse -Force
if ($Repo) {
  # bake the GitHub repository for update checks into this build
  $init = "$work\src\stleditor\__init__.py"
  (Get-Content $init -Raw) -replace 'UPDATE_REPO = ".*"', "UPDATE_REPO = `"$Repo`"" | Set-Content $init -Encoding utf8
  Write-Host "Update checks: github.com/$Repo"
}

Push-Location "$work\src"
$piArgs = @(
  '-m', 'PyInstaller', '--noconfirm', '--clean', '--windowed',
  '--name', 'STL Editor', '--icon', "$work\src\stl_editor.ico",
  '--add-data', "$work\src\web;web",
  '--collect-data', 'trimesh',
  '--collect-all', 'manifold3d',
  '--collect-all', 'fast_simplification',
  '--collect-submodules', 'skimage.measure',
  '--collect-all', 'webview',
  '--hidden-import', 'clr',
  '--exclude-module', 'matplotlib', '--exclude-module', 'IPython', '--exclude-module', 'pandas',
  '--exclude-module', 'PyQt5', '--exclude-module', 'PyQt6', '--exclude-module', 'PySide2', '--exclude-module', 'PySide6',
  '--exclude-module', 'torch', '--exclude-module', 'cv2', '--exclude-module', 'pytest', '--exclude-module', 'notebook',
  # never used at run time (checked by packaging/smoke_test.py after every build); saves ~45 MB
  '--exclude-module', 'tkinter', '--exclude-module', '_tkinter', '--exclude-module', 'PIL.ImageTk', '--exclude-module', 'PIL._imagingtk',
  '--exclude-module', 'PIL._avif', '--exclude-module', 'PIL.AvifImagePlugin',
  '--exclude-module', 'cryptography', '--exclude-module', 'bcrypt', '--exclude-module', 'zstandard',
  '--exclude-module', 'scipy.stats', '--exclude-module', 'scipy.signal', '--exclude-module', 'scipy.interpolate',
  '--exclude-module', 'scipy.integrate', '--exclude-module', 'scipy.io', '--exclude-module', 'scipy.odr',
  '--exclude-module', 'scipy.cluster', '--exclude-module', 'scipy.datasets', '--exclude-module', 'scipy.differentiate',
  '--exclude-module', 'scipy.fftpack', '--exclude-module', 'scipy.misc',
  '--exclude-module', 'skimage.data', '--exclude-module', 'skimage.feature', '--exclude-module', 'skimage.filters',
  '--exclude-module', 'skimage.segmentation', '--exclude-module', 'skimage.restoration', '--exclude-module', 'skimage.registration',
  '--exclude-module', 'skimage.io', '--exclude-module', 'skimage.color', '--exclude-module', 'skimage.draw',
  '--exclude-module', 'skimage.exposure', '--exclude-module', 'skimage.future', '--exclude-module', 'skimage.graph',
  '--exclude-module', 'skimage.metrics', '--exclude-module', 'skimage.transform', '--exclude-module', 'skimage.morphology',
  '--exclude-module', 'skimage.util',
  '--distpath', "$work\dist", '--workpath', "$work\build", '--specpath', "$work",
  'main.py'
)
& python @piArgs
if ($LASTEXITCODE -ne 0) { throw "PyInstaller failed ($LASTEXITCODE)" }
Pop-Location

$app = "$work\dist\STL Editor"
# licences and the friends' readme
& python "$work\src\packaging\collect_licenses.py" "$app\LICENSES"
Copy-Item "$work\src\packaging\READ ME FIRST.txt" "$app\" -Force
Copy-Item "$work\src\USER_GUIDE.md" "$app\User guide.md" -Force
Copy-Item "$work\src\LICENSE" "$app\LICENSE.txt" -Force

# run every engine tool once in the built app; a missing library fails the build here
& python "$work\src\packaging\smoke_test.py" "$app\STL Editor.exe"
if ($LASTEXITCODE -ne 0) { throw "Smoke test of the built app failed" }

New-Item -ItemType Directory -Force $out | Out-Null
$zip = Join-Path $out "STL Editor Portable $version.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path $app -DestinationPath $zip -CompressionLevel Optimal
$mb = [math]::Round((Get-Item $zip).Length / 1MB, 1)
$folderMb = [math]::Round(((Get-ChildItem $app -Recurse -File | Measure-Object Length -Sum).Sum) / 1MB, 1)
Write-Host "BUILD OK: $zip ($mb MB zipped, $folderMb MB unzipped)"
