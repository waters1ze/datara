# Verify Datara Quickstart and Tutorial Steps
$ErrorActionPreference = "Continue"
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Datara Quickstart & Tutorial Verification" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan

$steps = @(
    "step01_hello",
    "step02_types_and_vars",
    "step03_control_flow",
    "step04_functions",
    "step05_records",
    "step06_modules",
    "step07_outcomes",
    "step08_capabilities",
    "step09_interop",
    "step10_production_cli"
)

$passed = 0
$total = $steps.Count

foreach ($step in $steps) {
    $stepTarget = "examples/tutorial/$step"
    Write-Host -NoNewline "[RUN] $step ... "
    
    $out = & "$env:USERPROFILE\.cargo\bin\cargo.exe" run --quiet --bin forgen -- run $stepTarget 2>&1
    if ($LASTEXITCODE -eq 0) {
        Write-Host "PASS" -ForegroundColor Green
        $passed++
    } else {
        Write-Host "FAIL (Exit code: $LASTEXITCODE)" -ForegroundColor Red
        Write-Host $out -ForegroundColor Yellow
    }
}

Write-Host "----------------------------------------"
if ($passed -eq $total) {
    Write-Host "Tutorial Verification: $passed / $total steps passed." -ForegroundColor Green
    exit 0
} else {
    Write-Host "Tutorial Verification: $passed / $total steps passed." -ForegroundColor Red
    exit 1
}
