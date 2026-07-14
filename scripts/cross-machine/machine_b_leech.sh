#!/usr/bin/env bash
#
# machine_b_leech.sh — 机器 B（Leecher）启动脚本
#
# 启动顺序：
#   1. 启动 DHT Sidecar（加入机器 A 的 Kademlia 网络）并等待就绪
#   2. 启动 Download
#   3. DNS 解析验证（如果提供了机器 A 的 IP 或 Tailscale IP）
#
# 用法：
#   bash machine_b_leech.sh <机器A_TAILSCALE_IP>
#
# 示例：
#   bash machine_b_leech.sh 100.73.150.14

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TEST_DIR="${HOME}/bittorrent-test"

# ── 参数解析 ─────────────────────────────────────────────────────
SEEDER_TAILSCALE_IP="${1:-${SEEDER_IP:-}}"
if [ -z "${SEEDER_TAILSCALE_IP}" ]; then
    echo "错误: 请传入机器 A 的 Tailscale IP"
    echo "用法: bash $0 <机器A_Tailscale_IP>"
    echo "示例: bash $0 100.73.150.14"
    exit 1
fi

# 检测本机 Tailscale IP
MY_TAILSCALE_IP=""
if command -v tailscale &> /dev/null; then
    MY_TAILSCALE_IP=$(tailscale ip -4 2>/dev/null || echo "")
fi
if [ -z "${MY_TAILSCALE_IP}" ]; then
    echo "错误: 无法检测本机 Tailscale IP，请确认 tailscale 已安装并登录"
    echo "  tailscale ip -4"
    exit 1
fi

DHT_PORT=20000
GRPC_PORT=50052
PEER_PORT=6882
TORRENT_FILE="${TEST_DIR}/test.torrent"
OUTPUT_DIR="${TEST_DIR}/downloads"

echo "=============================================="
echo "  机器 B — Leecher 启动"
echo "=============================================="
echo ""
echo "本机 Tailscale IP:  ${MY_TAILSCALE_IP}"
echo "机器 A (Seeder) IP: ${SEEDER_TAILSCALE_IP}"
echo "DHT P2P 端口:       ${DHT_PORT}"
echo "gRPC 端口:          ${GRPC_PORT}"
echo "Peer 端口:          ${PEER_PORT}"
echo ".torrent:           ${TORRENT_FILE}"
echo "输出目录:           ${OUTPUT_DIR}"
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
    echo "错误: ${TORRENT_FILE} 不存在"
    echo "  请从机器 A 拷贝: scp user@${SEEDER_TAILSCALE_IP}:~/bittorrent-test/test.torrent ~/bittorrent-test/"
    echo "  以及 SHA1 文件:   scp user@${SEEDER_TAILSCALE_IP}:~/bittorrent-test/test_data.bin.sha1 ~/bittorrent-test/"
    exit 1
fi

# ── 网络连通性预检 ───────────────────────────────────────────────
echo ">>> 网络连通性预检 ..."
if ping -c 1 -W 2 "${SEEDER_TAILSCALE_IP}" > /dev/null 2>&1; then
    echo "  -> ping ${SEEDER_TAILSCALE_IP} 成功"
else
    echo "  -> ping ${SEEDER_TAILSCALE_IP} 失败！请检查 Tailscale 是否在线"
    echo "     tailscale status"
    echo "     tailscale ping --verbose ${SEEDER_TAILSCALE_IP}"
    exit 1
fi

# 清理残留进程
pkill -f "dht-sidecar" 2>/dev/null || true
pkill -f "./backend download" 2>/dev/null || true
sleep 1

# ── 启动 DHT Sidecar ─────────────────────────────────────────────
echo ""
echo ">>> 启动 DHT Sidecar（加入机器 A 的 Kademlia 网络, join=${SEEDER_TAILSCALE_IP}:${DHT_PORT}）..."
cd "${TEST_DIR}"
nohup ./dht-sidecar \
    --addr "${MY_TAILSCALE_IP}" \
    --port "${DHT_PORT}" \
    --join "${SEEDER_TAILSCALE_IP}:${DHT_PORT}" \
    --grpc-port "${GRPC_PORT}" \
    > /tmp/dht-leech.log 2>&1 &

DHT_PID=$!
echo "  -> PID: ${DHT_PID}, 日志: /tmp/dht-leech.log"

# 等待 DHT Sidecar 就绪
echo "  -> 等待 DHT Sidecar 就绪并加入网络..."
for i in $(seq 1 20); do
    # 检查 gRPC 是否已启动
    if grpcurl -plaintext "127.0.0.1:${GRPC_PORT}" list 2>/dev/null | grep -q "DhtService"; then
        echo "  -> gRPC 服务就绪"
        # 再等 3 秒让 DHT join 完成
        sleep 3
        break
    fi
    if [ $i -eq 20 ]; then
        echo "  -> 超时! 请检查日志: tail -f /tmp/dht-leech.log"
        exit 1
    fi
    sleep 1
done

# 验证 DHT 已加入网络（通过 Put/Get 测试）
echo "  -> 验证 DHT 网络连通性..."
if grpcurl -plaintext -d '{"key":"B-check","value":"alive"}' \
    "127.0.0.1:${GRPC_PORT}" dht.DhtService/Put 2>/dev/null | grep -q '"success": true'; then
    echo "  -> DHT 网络连通性验证通过!"
else
    echo "  -> 警告: DHT Put 测试未返回 success，但可能不影响后续流程"
fi
echo ""

# ── 启动 Download ────────────────────────────────────────────────
echo ">>> 启动 Download ..."
echo ""

./backend download "${TORRENT_FILE}" \
    --bind "${MY_TAILSCALE_IP}" \
    --port "${PEER_PORT}" \
    --dht "http://127.0.0.1:${GRPC_PORT}" \
    --output "${OUTPUT_DIR}" \
    --max-peers 50 \
    --pipeline 5 \
    2>&1 | tee /tmp/download.log

DOWNLOAD_EXIT_CODE=${PIPESTATUS[0]}

echo ""
echo "=============================================="
echo "  下载完成! (exit code: ${DOWNLOAD_EXIT_CODE})"
echo "=============================================="
echo ""

if [ ${DOWNLOAD_EXIT_CODE} -ne 0 ]; then
    echo "下载似乎失败了，请检查日志: /tmp/download.log"
    echo ""
    echo "DHT Sidecar 日志:"
    tail -30 /tmp/dht-leech.log
    echo ""
    echo "Download 日志:"
    tail -30 /tmp/download.log
    exit 1
fi

echo "输出文件:"
ls -lh "${OUTPUT_DIR}/" 2>/dev/null || echo "  (输出目录为空)"

echo ""
echo "DHT Sidecar 仍在后台运行（PID: ${DHT_PID}）"
echo "如需停止，运行: kill ${DHT_PID}"
echo "或运行: bash cleanup.sh"
