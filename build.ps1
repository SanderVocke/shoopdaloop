if (-not (Get-Command -Name uv -ErrorAction SilentlyContinue)) {
    Write-Host "Error: 'uv' not found in PATH. Please install it first (see https://github.com/astral-sh/uv)"
    exit 1
}

powershell -Command uv run scripts/build.py @args