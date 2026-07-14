#!/usr/bin/env bash
#
# machine_a_seed.sh — 机器 A（Seeder）启动脚本
#
# 启动顺序：
#   1. 启动 DHT Sidecar（前台阻塞，创建 Kademlia 网络）
#   2. 等待 DHT Sidecar 就绪后，在新终端启动 Seed
#
# 用法：
#   bash machine_a_seed.sh [TAILSCALE_IP]
#
# 默认 Tailscale IP: 100.73.150.14
# 如需指定，通过环境变量 TAILSCALE_IP 或第一个参数传入

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TEST_DIR="${HOME}/bittorrent-test"

TAILSCALE_IP="${1:-${TAILSCALE_IP:-}}"
if [ -z "${TAILSCALE_IP}" ]; then
    # 尝试自动检测
    if command -v tailscale &> /dev/null; then
        TAILSCALE_IP=$(tailscale ip -4 2>/dev/null || echo "")
    fi
    if [ -z "${TAILSCALE_IP}" ]; then
        echo "错误: 无法检测 Tailscale IP，请手动指定"
        echo "用法: bash $0 <TAILSCALE_IP>"
        echo "或: export TAILSCALE_IP=100.73.150.14 && bash $0"
        exit 1
    fi
fi

DHT_PORT=20000
GRPC_PORT=50051
PEER_PORT=6881
TORRENT_FILE="${TEST_DIR}/test.torrent"
DATA_DIR="${TEST_DIR}/data"

echo "=============================================="
echo "  机器 A — Seeder 启动"
echo "=============================================="
echo ""
echo "Tailscale IP: ${TAILSCALE_IP}"
echo "DHT P2P 端口: ${DHT_PORT}"
echo "gRPC 端口:    ${GRPC_PORT}"
echo "Peer 端口:    ${PEER_PORT}"
echo ".torrent:     ${TORRENT_FILE}"
echo "数据目录:     ${DATA_DIR}"
echo ""

# 检查必要文件
if [ ! -f "${TEST_DIR}/dht-sidecar" ]; then
    echo "错误: ${TEST_DIR}/dht-sidecar 不存在，请先运行 setup.sh"
    exit 1
fi
if [ ! -f "${TEST_DIR}/backend" ]; then
    echo "错误: ${TEST_DIR}/backend 不存在，请先运行 setup.sh"
    exit 1
fi
if [ ! -f "${TORRENT_FILE}" ]; then
    echo "错误: ${TORRENT_FILE} 不存在，请先运行 setup.sh"
    exit 1
fi
if [ ! -f "${DATA_DIR}/test_data.bin" ]; then
    echo "错误: ${DATA_DIR}/test_data.bin 不存在，请先运行 setup.sh"
    exit 1
fi

# 清理残留进程
pkill -f "dht-sidecar" 2>/dev/null || true
pkill -f "./backend seed" 2>/dev/null || true
sleep 1

# ── 启动 DHT Sidecar ─────────────────────────────────────────────
echo ">>> 启动 DHT Sidecar（创建 Kademlia 网络）..."
cd "${TEST_DIR}"
nohup ./dht-sidecar \
    --addr "${TAILSCALE_IP}" \
    --port "${DHT_PORT}" \
    --grpc-port "${GRPC_PORT}" \
    > /tmp/dht-seed.log 2>&1 &

DHT_PID=$!
echo "  -> PID: ${DHT_PID}, 日志: /tmp/dht-seed.log"

# 等待 DHT Sidecar 就绪
echo "  -> 等待 DHT Sidecar 就绪..."
for i in $(seq 1 15); do
    if grpcurl -plaintext "127.0.0.1:${GRPC_PORT}" list 2>/dev/null | grep -q "DhtService"; then
        echo "  -> DHT Sidecar 就绪!"
        break
    fi
    if [ $i -eq 15 ]; then
        echo "  -> 超时! 请检查日志: tail -f /tmp/dht-seed.log"
        echo "  -> DHT Sidecar PID: ${DHT_PID}"
        exit 1
    fi
    sleep 1
done
echo ""

# ── 启动 Seed ────────────────────────────────────────────────────
echo ">>> 启动 Seeder ..."
echo ""
nohup ./backend seed "${TORRENT_FILE}" \
    --data "${DATA_DIR}" \
    --bind "${TAILSCALE_IP}" \
    --port "${PEER_PORT}" \
    --dht "http://127.0.0.1:${GRPC_PORT}" \
    --max-peers 50 \
    > /tmp/seed.log 2>&1 &

SEED_PID=$!
echo "  -> Seed PID: ${SEED_PID}, 日志: /tmp/seed.log"

# 等待 Seeder 就绪
sleep 3
if ps -p ${SEED_PID} > /dev/null 2>&1; then
    echo "  -> Seeder 运行中!"
    echo ""
    echo "=============================================="
    echo "  机器 A Seeder 已就绪，等待机器 B 连接..."
    echo "=============================================="
    echo ""
    echo "查看日志:"
    echo "  DHT Sidecar: tail -f /tmp/dht-seed.log"
    echo "  Seeder:      tail -f /tmp/seed.log"
    echo ""
    echo "预期日志中包含:"
    echo "  - gRPC server listening on :${GRPC_PORT}"
    echo "  - seeding <N> pieces, waiting for peers..."
    echo ""
    echo "清理进程:"
    echo "  kill ${DHT_PID} ${SEED_PID}"
    echo "  或运行: bash cleanup.sh"
else
    echo "错误: Seeder 启动失败!"
    echo "DHT Sidecar 日志:"
    tail -20 /tmp/dht-seed.log
    echo ""
    echo "Seed 日志:"
    tail -20 /tmp/seed.log
    exit 1
fi
