#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Nikolay Govorov <me@govorov.online>
# SPDX-License-Identifier: AGPL-3.0-or-later

# Builds zorian, starts it in a temporary directory, runs smoke tests, cleans up.
#
# Usage:
#   ./tests/smoke/run-local.sh          # default port 2025
#   ZORIAN_PORT=9999 ./tests/smoke/run-local.sh

set -euo pipefail

PORT="${ZORIAN_PORT:-2025}"
BASE_URL="http://127.0.0.1:${PORT}"

TMPDIR="$(mktemp -d)"
LOGFILE="$TMPDIR/zorian.log"
trap 'kill "$PID" 2>/dev/null; wait "$PID" 2>/dev/null; rm -rf "$TMPDIR"' EXIT

# Build
echo "Building zorian..."
cargo build --release -p zorian

# Write minimal config
cat > "$TMPDIR/zorian.toml" <<EOF
appname = "smoke"
dirname = "$TMPDIR/state"

[[listen]]
addr = "127.0.0.1:${PORT}"
hostnames = []

[server]
shutdown_timeout = 5
request_timeout = 600
max_body_size = "64 MB"
max_concurrent_requests = 64
rate_limit_period = 1
rate_limit_burst_size = 200

[telemetry.stdout]
enabled = true
log_level = "info"
log_format = "pretty"
EOF

mkdir -p "$TMPDIR/state"

# Start zorian
echo "Starting zorian on ${BASE_URL}..."
echo "Server log: ${LOGFILE}"
cargo run --release -p zorian -- --config="$TMPDIR/zorian.toml" >"$LOGFILE" 2>&1 &
PID=$!

# Wait for index to load (log shows "index refreshed" for each backend)
echo "Waiting for index to load..."
for i in $(seq 1 120); do
    if grep -q "index refreshed" "$LOGFILE" 2>/dev/null; then
        echo "Index loaded after ${i}s"
        break
    fi
    if ! kill -0 "$PID" 2>/dev/null; then
        echo "zorian exited unexpectedly. Server log:"
        cat "$LOGFILE"
        exit 1
    fi
    sleep 1
done

if ! grep -q "index refreshed" "$LOGFILE" 2>/dev/null; then
    echo "Timed out waiting for index. Server log:"
    cat "$LOGFILE"
    exit 1
fi

# Run smoke tests
ZORIAN_URL="${BASE_URL}" cargo run -p smoke
