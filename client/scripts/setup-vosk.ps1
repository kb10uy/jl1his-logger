#!/usr/bin/env pwsh
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

Set-Location (Join-Path $PSScriptRoot '..')
$work = Join-Path ([System.IO.Path]::GetTempPath()) ([System.IO.Path]::GetRandomFileName())
New-Item -ItemType Directory -Force $work, 'static/models', '.vosk' | Out-Null

function Test-NonEmpty([string]$Path) {
    (Test-Path -PathType Leaf $Path) -and (Get-Item $Path).Length -gt 0
}

try {
    foreach ($model in 'vosk-model-small-en-us-0.15', 'vosk-model-small-ja-0.22') {
        $archive = "static/models/$model.tar.gz"
        $words = ".vosk/$model.words"
        if ((Test-NonEmpty $archive) -and (Test-NonEmpty $words)) {
            Write-Output "$model is ready"
            continue
        }
        $zip = Join-Path $work "$model.zip"
        $extracted = Join-Path $work $model
        Invoke-WebRequest -MaximumRetryCount 3 -OutFile $zip "https://alphacephei.com/vosk/models/$model.zip"
        Expand-Archive -Path $zip -DestinationPath $extracted
        if (-not (Test-NonEmpty $archive)) {
            tar -C $extracted -czf $archive $model
            if ($LASTEXITCODE -ne 0) { throw "tar failed for $model" }
            Write-Output $archive
        }
        $graph = Join-Path $extracted "$model/graph"
        $source = Join-Path $graph 'words.txt'
        if (-not (Test-NonEmpty $source)) { $source = Join-Path $graph 'Gr.fst' }
        Copy-Item $source $words
        Write-Output $words
    }
} finally {
    Remove-Item -Recurse -Force $work
}
