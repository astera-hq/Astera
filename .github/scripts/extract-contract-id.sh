#!/usr/bin/env bash
set -euo pipefail

grep -E '^C[A-Z0-9]{55}$' | tail -1
