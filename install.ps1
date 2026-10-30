# Installs the `kelve` command, the newest release, for Windows:
#
#   irm https://raw.githubusercontent.com/kelve-ai/kelve/main/install.ps1 | iex
#
# It downloads the build from GitHub Releases, checks it against the
# release's checksums, puts kelve.exe in %LOCALAPPDATA%\kelve\bin, or in
# KELVE_INSTALL_DIR, and adds that folder to your PATH. Then `kelve open .`
# opens kelve in your browser. Downloaded this way, rather than in a
# browser, kelve.exe isn't marked as from the internet, so SmartScreen
# doesn't stop it.
#
# It's run with `iex`, in the person's own window: it never calls `exit`,
# which would close it, and says what went wrong instead.

& {
    $ErrorActionPreference = 'Stop'
    # Showing progress makes downloads many times slower in Windows PowerShell.
    $ProgressPreference = 'SilentlyContinue'
    # Windows PowerShell 5.1 may not offer TLS 1.2 unless asked; GitHub needs it.
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

    $repo = 'kelve-ai/kelve'
    $name = 'kelve-windows-x86_64.zip'
    # Windows on Arm runs the x64 build.
    if (-not [Environment]::Is64BitOperatingSystem) {
        Write-Host 'kelve has no build for 32-bit Windows.' -ForegroundColor Red
        return
    }
    $dir = if ($env:KELVE_INSTALL_DIR) { $env:KELVE_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'kelve\bin' }
    $base = "https://github.com/$repo/releases/latest/download"
    $tmp = Join-Path ([IO.Path]::GetTempPath()) ("kelve-" + [Guid]::NewGuid())
    New-Item -ItemType Directory -Path $tmp | Out-Null
    try {
        $zip = Join-Path $tmp $name
        $sums = Join-Path $tmp 'SHA256SUMS'
        Invoke-WebRequest -UseBasicParsing -Uri "$base/$name" -OutFile $zip
        Invoke-WebRequest -UseBasicParsing -Uri "$base/SHA256SUMS" -OutFile $sums
        $line = Get-Content $sums | Where-Object { $_ -match "\s\*?$([Regex]::Escape($name))$" } | Select-Object -First 1
        $want = if ($line) { ($line -split '\s+')[0].ToLowerInvariant() } else { '' }
        $got = (Get-FileHash -Algorithm SHA256 -Path $zip).Hash.ToLowerInvariant()
        if (-not $want -or $want -ne $got) {
            Write-Host "The download doesn't match the release's checksum, so it wasn't installed. Try again." -ForegroundColor Red
            return
        }
        Expand-Archive -Path $zip -DestinationPath $tmp -Force
        New-Item -ItemType Directory -Path $dir -Force | Out-Null
        try {
            Copy-Item -Path (Join-Path $tmp 'kelve.exe') -Destination (Join-Path $dir 'kelve.exe') -Force
        } catch [System.IO.IOException] {
            # In use: a kelve.exe that's running can't be replaced.
            Write-Host 'kelve is running, so it couldn''t be replaced. Run `kelve stop`, then try again.' -ForegroundColor Red
            return
        }
    } finally {
        Remove-Item -Recurse -Force -Path $tmp -ErrorAction SilentlyContinue
    }

    Write-Host "kelve is installed in $dir."
    # On the PATH for new windows, and for this one.
    $path = [Environment]::GetEnvironmentVariable('Path', 'User')
    $parts = if ($path) { $path -split ';' } else { @() }
    if ($parts -notcontains $dir) {
        [Environment]::SetEnvironmentVariable('Path', (($parts + $dir) | Where-Object { $_ }) -join ';', 'User')
        $env:Path = "$env:Path;$dir"
        Write-Host "It's on your PATH now."
    }
    # Set it up: connect the agent, add its hook, and how much kelve marks.
    & (Join-Path $dir 'kelve.exe') setup
}
