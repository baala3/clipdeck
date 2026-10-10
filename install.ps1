# Installs the latest Clipdeck release on Windows, for the current user:
#
#   irm https://raw.githubusercontent.com/baala3/clipdeck/main/install.ps1 | iex
#
# Clipdeck is unsigned (docs/adr/0002-ship-unsigned.md). A browser download
# carries the Mark of the Web, which is what makes SmartScreen warn about it; a
# PowerShell download doesn't, so the installer runs without the warning.
# Smart App Control still blocks unsigned apps either way.

& {
    $ErrorActionPreference = 'Stop'
    # The progress bar makes Windows PowerShell 5.1 downloads many times slower.
    $ProgressPreference = 'SilentlyContinue'

    # The installer's file name carries the version, so ask the updater
    # manifest where the latest one is.
    $manifest = 'https://github.com/baala3/clipdeck/releases/latest/download/latest.json'
    $url = (Invoke-RestMethod $manifest).platforms.'windows-x86_64'.url

    $installer = Join-Path ([IO.Path]::GetTempPath()) 'Clipdeck-setup.exe'
    try {
        Write-Host 'Downloading Clipdeck...'
        Invoke-WebRequest $url -OutFile $installer -UseBasicParsing

        # /P shows progress without asking questions, /R starts Clipdeck afterwards.
        $exit = (Start-Process $installer -ArgumentList '/P', '/R' -Wait -PassThru).ExitCode
        if ($exit -ne 0) {
            throw "The Clipdeck installer exited with code $exit."
        }
        Write-Host 'Clipdeck is installed. Look for it in the notification area.'
    }
    finally {
        Remove-Item $installer -ErrorAction SilentlyContinue
    }
}
