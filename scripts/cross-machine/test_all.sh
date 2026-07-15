#!/usr/bin/env bash
#
# test_all.sh — Unified cross-machine BitTorrent end-to-end test
#
# Runs on Machine A, orchestrates Machine B via SSH.
# Machine A: macOS (local), Machine B: Linux ARM64 (parallels@100.66.36.52)
#
# Usage:
#   bash test_all.sh [--skip-build] [--data-size-mb N] [--download-timeout S]
#
# The test:
#   1. Builds binaries on both machines
#   2. Generates test data + .torrent on A, copies to B
#   3. Starts DHT + Seeder on A, DHT + Leecher on B
#   4. Downloads file from A to B via P2P
#   5. Verifies SHA1 integrity
#   6. Cleans up all processes and temp files automatically
#
# Requirements:
#   - Machine A: go, cargo, mktorrent (or transmission-create), grpcurl, tailscale
#   - Machine B: go, cargo, tailscale (grpcurl auto-installed if missing)

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info()  { echo -e "${BLUE}[INFO]${NC}  $(date '+%H:%M:%S') $*"; }
log_ok()    { echo -e "${GREEN}[OK]${NC}    $(date '+%H:%M:%S') $*"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC}  $(date '+%H:%M:%S') $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $(date '+%H:%M:%S') $*"; }
log_step()  { echo ""; echo -e "${GREEN}============================================================${NC}"; echo -e "${GREEN}  $*${NC}"; echo -e "${GREEN}============================================================${NC}"; echo ""; }

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT_A="$(cd "${SCRIPT_DIR}/../.." && pwd)"
PROJECT_ROOT_B="/home/parallels/BitTorrent"
TEST_DIR_A="${HOME}/bittorrent-test"
TEST_DIR_B="/home/parallels/bittorrent-test"
MACHINE_B="parallels@100.66.36.52"

DHT_PORT=20000
GRPC_PORT_A=50051
GRPC_PORT_B=50052
PEER_PORT_A=6881
PEER_PORT_B=6882
DATA_SIZE_MB=10
DOWNLOAD_TIMEOUT=120
SKIP_BUILD=false
CLEANUP_DONE=false

usage() {
    echo "Usage: $0 [--skip-build] [--data-size-mb N] [--download-timeout S]"
    echo "  --skip-build         Skip compilation, reuse existing binaries"
    echo "  --data-size-mb N     Test file size in MiB (default: 10)"
    echo "  --download-timeout S Download timeout in seconds (default: 120)"
    exit 1
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --skip-build)       SKIP_BUILD=true; shift ;;
        --data-size-mb)     DATA_SIZE_MB="$2"; shift 2 ;;
        --download-timeout) DOWNLOAD_TIMEOUT="$2"; shift 2 ;;
        --help|-h)          usage ;;
        *)                  echo "Unknown: $1"; usage ;;
    esac
done

cleanup() {
    if [ "${CLEANUP_DONE}" = true ]; then return; fi
    CLEANUP_DONE=true
    echo ""
    log_warn "Cleaning up processes and temp files..."

    log_info "Killing processes on Machine A..."
    pkill -f "dht-sidecar" 2>/dev/null || true
    pkill -f "target/release/backend\|bittorrent-test/backend" 2>/dev/null || true
    sleep 1
    pkill -9 -f "dht-sidecar" 2>/dev/null || true
    pkill -9 -f "backend" 2>/dev/null || true

    log_info "Killing processes on Machine B..."
    ssh -o ConnectTimeout=5 "${MACHINE_B}" \
        "pkill -f dht-sidecar 2>/dev/null || true; pkill -f 'bittorrent-test/backend\|/backend download\|/backend seed' 2>/dev/null || true; sleep 1; pkill -9 -f dht-sidecar 2>/dev/null || true; pkill -9 -f backend 2>/dev/null || true" 2>/dev/null || true

    log_info "Removing temp logs..."
    rm -f /tmp/dht-seed.log /tmp/seed.log /tmp/dht-leech.log /tmp/download.log /tmp/test_all_download.log

    ssh -o ConnectTimeout=5 "${MACHINE_B}" \
        "rm -f /tmp/dht-seed.log /tmp/seed.log /tmp/dht-leech.log /tmp/download.log /tmp/test_all_download.log" 2>/dev/null || true

    log_info "Cleanup complete."
}
trap cleanup EXIT

# ── Pre-flight checks ─────────────────────────────────────────────
log_step "PRE-FLIGHT CHECKS"

if [ ${DATA_SIZE_MB} -lt 1 ]; then
    log_error "data-size-mb must be >= 1"
    exit 1
fi

log_info "Checking SSH connectivity to Machine B..."
if ! ssh -o ConnectTimeout=10 -o BatchMode=yes "${MACHINE_B}" "echo ok" > /dev/null 2>&1; then
    log_error "Cannot connect to ${MACHINE_B}"
    exit 1
fi
log_ok "SSH to Machine B OK"

log_info "Detecting Machine A IP..."
IP_A=""
if command -v tailscale &>/dev/null; then
    IP_A=$(tailscale ip -4 2>/dev/null || echo "")
fi
if [ -z "${IP_A}" ]; then
    IP_A=$(ifconfig 2>/dev/null | grep 'inet ' | grep -v '127.0.0.1' | awk '{print $2}' | head -1)
fi
if [ -z "${IP_A}" ]; then
    log_error "Cannot detect Machine A IP. Please set IP_A environment variable."
    exit 1
fi
log_ok "Machine A IP: ${IP_A}"

log_info "Detecting Machine B IP..."
IP_B=$(ssh -o ConnectTimeout=5 "${MACHINE_B}" "tailscale ip -4 2>/dev/null || hostname -I 2>/dev/null | awk '{print \$1}'" 2>/dev/null || echo "")
if [ -z "${IP_B}" ]; then
    IP_B="100.66.36.52"
    log_warn "Cannot detect IP on B, using known IP: ${IP_B}"
else
    log_ok "Machine B IP: ${IP_B}"
fi

# ── Build ──────────────────────────────────────────────────────────
build_binaries() {
    local project_root="$1"
    local test_dir="$2"
    local machine_label="$3"
    local machine_ssh="$4"

    log_info "Compiling DHT Sidecar on ${machine_label}..."
    if [ -n "${machine_ssh}" ]; then
        ssh -o ConnectTimeout=10 "${machine_ssh}" \
            "cd ${project_root}/dht && go build -o ${test_dir}/dht-sidecar ." 2>&1 | tail -3
    else
        cd "${project_root}/dht"
        go build -o "${test_dir}/dht-sidecar" . 2>&1 | tail -3
        cd - > /dev/null
    fi
    log_ok "DHT Sidecar compiled on ${machine_label}"

    log_info "Compiling Rust Backend on ${machine_label}..."
    local build_output
    if [ -n "${machine_ssh}" ]; then
        build_output=$(ssh -o ConnectTimeout=10 "${machine_ssh}" \
            "cd ${project_root}/backend && cargo build --release 2>&1" 2>&1)
        ssh -o ConnectTimeout=10 "${machine_ssh}" \
            "cp ${project_root}/backend/target/release/backend ${test_dir}/backend"
    else
        build_output=$(cd "${project_root}/backend" && cargo build --release 2>&1)
        cp "${project_root}/backend/target/release/backend" "${test_dir}/backend"
    fi
    echo "${build_output}" | tail -5
    log_ok "Backend compiled on ${machine_label}"
}

ensure_grpcurl() {
    local machine_ssh="$1"
    local has_grpcurl
    has_grpcurl=$(ssh -o ConnectTimeout=5 "${machine_ssh}" \
        "command -v grpcurl 2>/dev/null || test -x \"\$HOME/go/bin/grpcurl\" 2>/dev/null && echo found || echo ''" 2>/dev/null)
    if [ -z "${has_grpcurl}" ]; then
        log_info "Installing grpcurl on Machine B..."
        ssh -o ConnectTimeout=10 "${machine_ssh}" \
            "command -v go &>/dev/null && go install github.com/fullstorydev/grpcurl/cmd/grpcurl@latest 2>&1" 2>&1 | tail -3
        log_ok "grpcurl installed on Machine B"
    else
        log_ok "grpcurl found on Machine B"
    fi
}

mkdir -p "${TEST_DIR_A}/data"
mkdir -p "${TEST_DIR_A}/downloads"

ssh -o ConnectTimeout=5 "${MACHINE_B}" "mkdir -p ${TEST_DIR_B}/data ${TEST_DIR_B}/downloads" 2>/dev/null || true

if [ "${SKIP_BUILD}" = false ]; then
    log_step "BUILD (skip with --skip-build)"
    build_binaries "${PROJECT_ROOT_A}" "${TEST_DIR_A}" "Machine A" ""
    build_binaries "${PROJECT_ROOT_B}" "${TEST_DIR_B}" "Machine B" "${MACHINE_B}"
    ensure_grpcurl "${MACHINE_B}"
else
    log_step "BUILD (skipped)"
    log_info "Checking existing binaries..."
    for f in "${TEST_DIR_A}/dht-sidecar" "${TEST_DIR_A}/backend"; do
        if [ ! -f "$f" ]; then
            log_error "Missing: $f (run without --skip-build to compile)"
            exit 1
        fi
    done
    b_missing=$(ssh -o ConnectTimeout=5 "${MACHINE_B}" \
        "ls ${TEST_DIR_B}/dht-sidecar ${TEST_DIR_B}/backend 2>/dev/null || echo MISSING" 2>/dev/null)
    if echo "${b_missing}" | grep -q "MISSING"; then
        log_error "Missing binaries on Machine B (run without --skip-build to compile)"
        exit 1
    fi
    log_ok "All binaries present"
    ensure_grpcurl "${MACHINE_B}"
fi

# ── Generate test data ─────────────────────────────────────────────
log_step "GENERATE TEST DATA"

TEST_FILE="${TEST_DIR_A}/data/test_data.bin"
TORRENT_FILE="${TEST_DIR_A}/test.torrent"
SHA1_FILE="${TEST_DIR_A}/test_data.bin.sha1"
COUNT=$((DATA_SIZE_MB * 1024))

log_info "Generating ${DATA_SIZE_MB} MiB random data..."
dd if=/dev/urandom of="${TEST_FILE}" bs=1024 count="${COUNT}" 2>/dev/null
sync
log_ok "Test file: ${TEST_FILE} ($(du -h "${TEST_FILE}" | cut -f1))"

if command -v shasum &>/dev/null; then
    shasum "${TEST_FILE}" | awk '{print $1}' > "${SHA1_FILE}"
elif command -v sha1sum &>/dev/null; then
    sha1sum "${TEST_FILE}" | awk '{print $1}' > "${SHA1_FILE}"
fi
EXPECTED_SHA1=$(cat "${SHA1_FILE}")
log_ok "SHA1: ${EXPECTED_SHA1}"

log_info "Creating .torrent file..."
rm -f "${TORRENT_FILE}"
if command -v mktorrent &>/dev/null; then
    mktorrent -a "http://dummy.tracker/announce" -l 15 -o "${TORRENT_FILE}" "${TEST_FILE}" 2>/dev/null
elif command -v transmission-create &>/dev/null; then
    transmission-create -o "${TORRENT_FILE}" -s 16 "${TEST_FILE}" 2>/dev/null
else
    log_error "Neither mktorrent nor transmission-create found"
    exit 1
fi
if [ ! -f "${TORRENT_FILE}" ]; then
    log_error "Failed to create .torrent file (${TORRENT_FILE})"
    exit 1
fi
log_ok ".torrent created: ${TORRENT_FILE}"

log_info "Copying .torrent and SHA1 to Machine B..."
scp -o ConnectTimeout=10 "${TORRENT_FILE}" "${MACHINE_B}:${TEST_DIR_B}/test.torrent" 2>/dev/null
scp -o ConnectTimeout=10 "${SHA1_FILE}" "${MACHINE_B}:${TEST_DIR_B}/test_data.bin.sha1" 2>/dev/null
log_ok "Files copied to Machine B"

# ── Kill stale processes ───────────────────────────────────────────
log_info "Killing stale processes on both machines..."
pkill -f "dht-sidecar" 2>/dev/null || true
pkill -f "backend" 2>/dev/null || true
ssh -o ConnectTimeout=5 "${MACHINE_B}" \
    "pkill -f dht-sidecar 2>/dev/null || true; pkill -f backend 2>/dev/null || true" 2>/dev/null || true
sleep 2
log_ok "Stale processes cleared"

# ── Start DHT Sidecar on A (bootstrap node) ────────────────────────
log_step "START DHT SIDECAR ON MACHINE A (bootstrap)"

cd "${TEST_DIR_A}"
nohup ./dht-sidecar \
    --addr "${IP_A}" \
    --port "${DHT_PORT}" \
    --grpc-port "${GRPC_PORT_A}" \
    > /tmp/dht-seed.log 2>&1 &
DHT_A_PID=$!
log_info "DHT Sidecar A PID: ${DHT_A_PID}"

log_info "Waiting for DHT Sidecar A to be ready..."
DHT_A_READY=false
for i in $(seq 1 20); do
    if grpcurl -plaintext "127.0.0.1:${GRPC_PORT_A}" list 2>/dev/null | grep -q "DhtService"; then
        DHT_A_READY=true
        break
    fi
    sleep 1
done
if [ "${DHT_A_READY}" = false ]; then
    log_error "DHT Sidecar A failed to start. Log:"
    tail -30 /tmp/dht-seed.log
    exit 1
fi
log_ok "DHT Sidecar A ready (bootstrap node)"

# ── Start DHT Sidecar on B (join A) ────────────────────────────────
log_step "START DHT SIDECAR ON MACHINE B (join A)"

GRPCURL_BIN_B=$(ssh -o ConnectTimeout=5 "${MACHINE_B}" "command -v grpcurl || echo '\$HOME/go/bin/grpcurl'" 2>/dev/null)

ssh -o ConnectTimeout=10 "${MACHINE_B}" \
    "cd ${TEST_DIR_B} && nohup ./dht-sidecar --addr ${IP_B} --port ${DHT_PORT} --join ${IP_A}:${DHT_PORT} --grpc-port ${GRPC_PORT_B} > /tmp/dht-leech.log 2>&1 & sleep 1" 2>/dev/null

log_info "Waiting for DHT Sidecar B gRPC server..."
DHT_B_READY=false
for i in $(seq 1 25); do
    if ssh -o ConnectTimeout=5 "${MACHINE_B}" \
        "${GRPCURL_BIN_B} -plaintext 127.0.0.1:${GRPC_PORT_B} list 2>/dev/null" 2>/dev/null | grep -q "DhtService"; then
        DHT_B_READY=true
        break
    fi
    sleep 1
done
if [ "${DHT_B_READY}" = false ]; then
    log_error "DHT Sidecar B failed to start. Log:"
    ssh -o ConnectTimeout=5 "${MACHINE_B}" "tail -30 /tmp/dht-leech.log" 2>/dev/null || true
    exit 1
fi
log_ok "DHT Sidecar B gRPC ready"

log_info "Waiting for DHT join to complete..."
sleep 5

log_info "Verifying DHT cross-node connectivity..."
PUT_OK=$(ssh -o ConnectTimeout=5 "${MACHINE_B}" \
    "${GRPCURL_BIN_B} -plaintext -d '{\"key\":\"cross-check\",\"value\":\"ok\"}' 127.0.0.1:${GRPC_PORT_B} dht.DhtService/Put 2>/dev/null" 2>/dev/null || echo "")
if echo "${PUT_OK}" | grep -q '"success".*true'; then
    log_ok "DHT cross-node connectivity verified"
else
    log_warn "DHT Put test did not return success — may still work, continuing..."
fi

# ── Start Seeder on A ──────────────────────────────────────────────
log_step "START SEEDER ON MACHINE A"

cd "${TEST_DIR_A}"
nohup ./backend seed "${TORRENT_FILE}" \
    --data "${TEST_DIR_A}/data" \
    --bind "${IP_A}" \
    --port "${PEER_PORT_A}" \
    --dht "http://127.0.0.1:${GRPC_PORT_A}" \
    --max-peers 50 \
    > /tmp/seed.log 2>&1 &
SEED_PID=$!
log_info "Seeder PID: ${SEED_PID}"

sleep 2
if ! ps -p ${SEED_PID} > /dev/null 2>&1; then
    log_error "Seeder failed to start. Log:"
    tail -30 /tmp/seed.log
    exit 1
fi
log_ok "Seeder running on ${IP_A}:${PEER_PORT_A}"

# ── Run Leecher on B (download) ────────────────────────────────────
log_step "START LEECHER ON MACHINE B (download)"

DOWNLOAD_LOG="/tmp/test_all_download.log"
DOWNLOAD_EXIT_CODE=0

echo ""
log_info "Downloading... (timeout: ${DOWNLOAD_TIMEOUT}s)"
echo ""

set +e
timeout ${DOWNLOAD_TIMEOUT} ssh -o ConnectTimeout=10 "${MACHINE_B}" \
    "cd ${TEST_DIR_B} && ./backend download ${TEST_DIR_B}/test.torrent \
        --bind ${IP_B} \
        --port ${PEER_PORT_B} \
        --dht http://127.0.0.1:${GRPC_PORT_B} \
        --output ${TEST_DIR_B}/downloads \
        --max-peers 50 \
        --pipeline 5" 2>&1 | tee "${DOWNLOAD_LOG}"
DOWNLOAD_EXIT_CODE=${PIPESTATUS[0]}
set -e

echo ""
if [ ${DOWNLOAD_EXIT_CODE} -eq 124 ]; then
    log_error "Download timed out after ${DOWNLOAD_TIMEOUT}s"
    log_info "Seeder log (last 20 lines):"
    tail -20 /tmp/seed.log
    log_info "DHT B log (last 20 lines):"
    ssh -o ConnectTimeout=5 "${MACHINE_B}" "tail -20 /tmp/dht-leech.log" 2>/dev/null || true
    exit 1
elif [ ${DOWNLOAD_EXIT_CODE} -ne 0 ]; then
    log_error "Download failed with exit code ${DOWNLOAD_EXIT_CODE}"
    log_info "Download log (last 30 lines):"
    tail -30 "${DOWNLOAD_LOG}"
    log_info "Seeder log (last 20 lines):"
    tail -20 /tmp/seed.log
    exit 1
fi
log_ok "Download completed successfully"

# ── Verify downloaded file ─────────────────────────────────────────
log_step "VERIFY DOWNLOAD"

log_info "Listing downloaded files on Machine B..."
DOWNLOADED_FILES=$(ssh -o ConnectTimeout=5 "${MACHINE_B}" \
    "find ${TEST_DIR_B}/downloads -type f 2>/dev/null | sort" 2>/dev/null)

if [ -z "${DOWNLOADED_FILES}" ]; then
    log_error "No files found in ${TEST_DIR_B}/downloads on Machine B"
    exit 1
fi
echo "${DOWNLOADED_FILES}" | while read f; do echo "  $f"; done

log_info "Computing SHA1 of downloaded file(s)..."
PASS=true
while IFS= read -r file; do
    ACTUAL_SHA1=$(ssh -o ConnectTimeout=5 "${MACHINE_B}" \
        "sha1sum '${file}' | awk '{print \$1}'" 2>/dev/null)
    DL_SIZE=$(ssh -o ConnectTimeout=5 "${MACHINE_B}" \
        "stat -c%s '${file}'" 2>/dev/null || echo "0")

    echo ""
    echo "  File:     ${file}"
    echo "  Expected: ${EXPECTED_SHA1}"
    echo "  Actual:   ${ACTUAL_SHA1}"
    echo "  Size:     ${DL_SIZE} bytes"

    if [ "${ACTUAL_SHA1}" = "${EXPECTED_SHA1}" ]; then
        log_ok "  SHA1: PASS"
    else
        log_error "  SHA1: FAIL"
        PASS=false
    fi
done <<< "${DOWNLOADED_FILES}"

# ── Final result ───────────────────────────────────────────────────
log_step "RESULT"

if [ "${PASS}" = true ]; then
    log_ok "============================================================"
    log_ok "  CROSS-MACHINE TEST PASSED"
    log_ok "  SHA1 verified: ${EXPECTED_SHA1}"
    log_ok "  File transferred via P2P from ${IP_A} to Machine B"
    log_ok "============================================================"
else
    log_error "============================================================"
    log_error "  CROSS-MACHINE TEST FAILED"
    log_error "  SHA1 mismatch — data corruption or incomplete transfer"
    log_error "============================================================"
    exit 1
fi

# Cleanup runs via trap EXIT
