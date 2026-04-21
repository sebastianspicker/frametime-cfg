@echo off
:: CS2 Optimization Suite — GUI Launcher
:: Elevates to administrator and launches the WPF dashboard

net session >nul 2>&1
if %errorlevel% neq 0 (
    powershell -NoProfile -WindowStyle Hidden -Command ^
        "Start-Process powershell -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-WindowStyle','Hidden','-File','\"%~dp0CS2-Optimize-GUI.ps1\"') -Verb RunAs"
    exit /b
)

powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "%~dp0CS2-Optimize-GUI.ps1"
