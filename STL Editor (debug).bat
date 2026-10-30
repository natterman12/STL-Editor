@echo off
rem Starts STL Editor with a console window so errors are visible.
cd /d "%~dp0"
python main.py %*
pause
