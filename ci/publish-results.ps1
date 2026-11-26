# Publishes a folder of CI results (JSON, PNG, logs) to its own branch
# "ci-results/<run>-<attempt>-<job>" so they can be read with plain git.
# (Artifact blob storage is not reachable from the development sandbox.)
param(
    [Parameter(Mandatory)] [string] $Folder,
    [Parameter(Mandatory)] [string] $Job
)
$ErrorActionPreference = 'Stop'
$safeJob = ($Job -replace '[^A-Za-z0-9._-]', '-').ToLower()
$branch = "ci-results/$($env:GITHUB_RUN_ID)-$($env:GITHUB_RUN_ATTEMPT)-$safeJob"
$work = Join-Path $env:RUNNER_TEMP "results-$safeJob"
if (Test-Path $work) { Remove-Item $work -Recurse -Force }
New-Item -ItemType Directory $work | Out-Null
Copy-Item (Join-Path $Folder '*') $work -Recurse -Force
Push-Location $work
try {
    git init -q -b $branch
    git config user.name 'claude-talk-ci'
    git config user.email 'ci@users.noreply.github.com'
    git add -A
    git commit -q --allow-empty -m "CI results: $Job (run $($env:GITHUB_RUN_ID))"
    $url = "https://x-access-token:$($env:GITHUB_TOKEN)@github.com/$($env:GITHUB_REPOSITORY).git"
    git push -q --force $url "HEAD:refs/heads/$branch"
    Write-Host "Results published to branch $branch"
} finally { Pop-Location }
