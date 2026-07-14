#!/usr/bin/env bash
#
# cleanup.sh — 清理跨机器测试环境
#
# 功能：
#   1. 停止所有 dht-sidecar 进程
#   2. 停止所有 backend 进程
#   3. （可选）清理测试目录
#
# 用法：
#   bash cleanup.sh [--clean-all]
#     --clean-all  同时删除 ~/bittorrent-test/ 目录

set -euo pipefail

CLEAN_ALL=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --clean-all) CLEAN_ALL=true; shift ;;
        --*) echo "未知参数: $1"; echo "用法: $0 [--clean-all]"; exit 1 ;;
        *) break ;;
    esac
done

echo "=============================================="
echo "  清理跨机器测试环境"
echo "=============================================="
echo ""

# ── 1. 停止进程 ──────────────────────────────────────────────────
echo ">>> 停止 DHT Sidecar 进程 ..."
PIDS=$(pgrep -f "dht-sidecar" 2>/dev/null || true)
if [ -n "${PIDS}" ]; then
    echo "  发现 PID: ${PIDS}"
    pkill -f "dht-sidecar" 2>/dev/null || true
    echo "  -> 已发送终止信号"
else
    echo "  -> 未发现 dht-sidecar 进程"
fi

echo ""
echo ">>> 停止 Backend 进程 ..."
PIDS=$(pgrep -f "./backend" 2>/dev/null || true)
if [ -n "${PIDS}" ]; then
    echo "  发现 PID: ${PIDS}"
    pkill -f "./backend" 2>/dev/null || true
    echo "  -> 已发送终止信号"
else
    echo "  -> 未发现 backend 进程"
fi

sleep 1

# 强制清理残留
echo ""
echo ">>> 强制清理残留进程 ..."
PIDS=$(pgrep -f "dht-sidecar\|backend" 2>/dev/null || true)
if [ -n "${PIDS}" ]; then
    echo "  残留 PID: ${PIDS}"
    pkill -9 -f "dht-sidecar" 2>/dev/null || true
    pkill -9 -f "backend" 2>/dev/null || true
    echo "  -> 已强制终止"
else
    echo "  -> 无残留进程"
fi

# ── 2. 清理临时日志 ──────────────────────────────────────────────
echo ""
echo ">>> 清理临时日志 ..."
rm -f /tmp/dht-seed.log /tmp/dht-leech.log /tmp/seed.log /tmp/download.log
echo "  -> 已清理"

# ── 3. 清理测试目录（可选）────────────────────────────────────────
if [ "${CLEAN_ALL}" = true ]; then
    echo ""
    echo ">>> 清理测试目录 ~/bittorrent-test/ ..."
    if [ -d "${HOME}/bittorrent-test" ]; then
        rm -rf "${HOME}/bittorrent-test"
        echo "  -> 已删除"
    else
        echo "  -> 目录不存在，跳过"
    fi
else
    echo ""
    echo ">>> 保留测试目录 ${HOME}/bittorrent-test/"
    echo "    如需完全清理，请使用 --clean-all 参数"
fi

echo ""
echo "=============================================="
echo "  清理完成!"
echo "=============================================="
