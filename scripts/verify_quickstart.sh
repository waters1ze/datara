#!/usr/bin/env bash
set -e
echo "========================================"
echo "  Datara Quickstart & Tutorial Verification"
echo "========================================"

STEPS=(
    "step01_hello"
    "step02_types_and_vars"
    "step03_control_flow"
    "step04_functions"
    "step05_records"
    "step06_modules"
    "step07_outcomes"
    "step08_capabilities"
    "step09_interop"
    "step10_production_cli"
)

PASSED=0
TOTAL=${#STEPS[@]}

for step in "${STEPS[@]}"; do
    echo -n "[RUN] $step ... "
    if cargo run --quiet --bin forgen -- run "examples/tutorial/$step" > /dev/null 2>&1; then
        echo "PASS"
        ((PASSED++))
    else
        echo "FAIL"
    fi
done

echo "----------------------------------------"
echo "Tutorial Verification: $PASSED / $TOTAL steps passed."
if [ "$PASSED" -ne "$TOTAL" ]; then
    exit 1
fi
exit 0
