@echo off
REM Build and run The Last Aeons natively.
REM
REM   run.bat            debug build (fast to compile, slow to play)
REM   run.bat release    optimised build (slow to compile, smooth to play)
REM
REM The web build is separate:
REM   trunk serve --config crates/aeon_client/Trunk.toml
REM
REM Authored content is embedded at compile time by crates/aeon_client/build.rs,
REM so the game needs no working directory beyond the repository itself.

setlocal
cd /d "%~dp0"

set PROFILE=
if /i "%~1"=="release" set PROFILE=--release

echo Building and running the native client %PROFILE%
cargo run -p aeon_client %PROFILE%
if errorlevel 1 (
    echo.
    echo Build or run failed. See the errors above.
    endlocal
    exit /b 1
)

endlocal
