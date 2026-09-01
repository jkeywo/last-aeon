param(
    [Parameter(Mandatory = $true)]
    [string] $ChromeDriver,
    [string] $ChromeBinary = "C:\Program Files\Google\Chrome\Application\chrome.exe"
)

$driver = (Resolve-Path -LiteralPath $ChromeDriver).Path
$browser = (Resolve-Path -LiteralPath $ChromeBinary).Path
$env:PATH = (Split-Path -Parent $driver) + [IO.Path]::PathSeparator + $env:PATH
$env:CHROME_BIN = $browser

& rtk cargo test -p aeon_client --bin last_aeons --target wasm32-unknown-unknown rendered_state -- --nocapture
exit $LASTEXITCODE
