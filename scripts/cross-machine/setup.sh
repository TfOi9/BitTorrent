#!/usr/bin/env bash
#
# setup.sh — 跨机器测试前置准备脚本
#
# 功能：
#   1. 编译 DHT Sidecar (Go)
#   2. 编译 Rust Backend
#   3. 创建测试目录 ~/bittorrent-test/
#   4. 生成随机测试文件并创建 .torrent
#   5. 记录原始文件 SHA1 供后续校验
#
# 用法：
#   chmod +x setup.sh
#   bash setup.sh [--skip-build] [--data-size-mb N]
#
# 注意事项：
#   - 两台机器都需要运行此脚本（分别在各自的机器上）
#   - 机器 B 不需要生成 .torrent，可从机器 A scp 获取
#   - 使用 --skip-build 跳过编译（如果已经编译过）

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
TEST_DIR="${HOME}/bittorrent-test"
DATA_SIZE_MB=10
SKIP_BUILD=false

usage() {
    echo "用法: $0 [--skip-build] [--data-size-mb N]"
    echo "  --skip-build    跳过编译步骤，假设二进制已就绪"
    echo "  --data-size-mb   测试文件大小，单位 MiB（默认 10）"
    exit 1
}

# 解析参数
while [[ $# -gt 0 ]]; do
    case "$1" in
        --skip-build) SKIP_BUILD=true; shift ;;
        --data-size-mb) DATA_SIZE_MB="$2"; shift 2 ;;
        --*) echo "未知参数: $1"; usage ;;
        *) usage ;;
    esac
done

echo "=============================================="
echo "  跨机器 BitTorrent 测试 — 环境准备"
echo "=============================================="
echo ""
echo "项目根目录: ${PROJECT_ROOT}"
echo "测试目录:   ${TEST_DIR}"
echo "测试文件大小: ${DATA_SIZE_MB} MiB"
echo ""

# ── 1. 编译 ──────────────────────────────────────────────────────
if [ "${SKIP_BUILD}" = false ]; then
    echo ">>> [1/4] 编译 DHT Sidecar ..."
    if ! command -v go &> /dev/null; then
        echo "错误: 未找到 Go，请先安装 Go 1.18+"
        echo "  macOS:  brew install go"
        echo "  Linux:  sudo apt install golang"
        exit 1
    fi
    cd "${PROJECT_ROOT}/dht"
    go build -o "${TEST_DIR}/dht-sidecar" .
    echo "  -> 完成: ${TEST_DIR}/dht-sidecar"

    echo ""
    echo ">>> [2/4] 编译 Rust Backend ..."
    if ! command -v cargo &> /dev/null; then
        echo "错误: 未找到 cargo，请先安装 Rust"
        echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
        exit 1
    fi
    cd "${PROJECT_ROOT}/backend"
    cargo build --release 2>&1 | tail -5
    cp target/release/backend "${TEST_DIR}/backend"
    echo "  -> 完成: ${TEST_DIR}/backend"
else
    echo ">>> 跳过编译 (--skip-build)"
    if [ ! -f "${TEST_DIR}/dht-sidecar" ]; then
        echo "警告: ${TEST_DIR}/dht-sidecar 不存在"
    fi
    if [ ! -f "${TEST_DIR}/backend" ]; then
        echo "警告: ${TEST_DIR}/backend 不存在"
    fi
fi

# ── 2. 创建测试目录 ───────────────────────────────────────────────
echo ""
echo ">>> [3/4] 创建测试目录 ..."
mkdir -p "${TEST_DIR}"
mkdir -p "${TEST_DIR}/data"
mkdir -p "${TEST_DIR}/downloads"
echo "  -> ${TEST_DIR}/data       (Seeder 数据目录)"
echo "  -> ${TEST_DIR}/downloads  (Leecher 输出目录)"

# ── 3. 生成测试文件 ───────────────────────────────────────────────
# 只在机器 A（Seeder）上需要生成 .torrent
# 机器 B 通过 scp 获取 .torrent
echo ""
echo ">>> [4/4] 生成测试数据 ..."
TEST_FILE="${TEST_DIR}/data/test_data.bin"
COUNT=$((DATA_SIZE_MB * 1024))

dd if=/dev/urandom of="${TEST_FILE}" bs=1024 count="${COUNT}" 2>/dev/null
echo "  -> 测试文件: ${TEST_FILE} (${DATA_SIZE_MB} MiB)"

# 记录 SHA1
if command -v shasum &> /dev/null; then
    shasum "${TEST_FILE}" | awk '{print $1}' > "${TEST_DIR}/test_data.bin.sha1"
elif command -v sha1sum &> /dev/null; then
    sha1sum "${TEST_FILE}" | awk '{print $1}' > "${TEST_DIR}/test_data.bin.sha1"
else
    echo "警告: 未找到 shasum 或 sha1sum，跳过 SHA1 记录"
fi
echo "  -> SHA1: $(cat "${TEST_DIR}/test_data.bin.sha1" 2>/dev/null || echo 'N/A')"

# 生成 .torrent
if command -v mktorrent &> /dev/null; then
    mktorrent \
        -a "http://dummy.tracker/announce" \
        -l 14 \
        -o "${TEST_DIR}/test.torrent" \
        "${TEST_FILE}" 2>/dev/null
    echo "  -> .torrent: ${TEST_DIR}/test.torrent"
elif command -v transmission-create &> /dev/null; then
    transmission-create \
        -o "${TEST_DIR}/test.torrent" \
        -s 16 \
        "${TEST_FILE}" 2>/dev/null
    echo "  -> .torrent: ${TEST_DIR}/test.torrent"
else
    echo "警告: 未找到 mktorrent 或 transmission-create"
    echo "  请手动生成 .torrent 文件:"
    echo "  macOS:  brew install mktorrent"
    echo "  Linux:  sudo apt install mktorrent"
    exit 1
fi

echo ""
echo "=============================================="
echo "  环境准备完成!"
echo "=============================================="
echo ""
echo "测试目录结构:"
ls -lh "${TEST_DIR}/" | tail -n +2
ls -lh "${TEST_DIR}/data/"
echo ""
echo "下一步:"
echo "  1. 如果机器 B 是另一台电脑，将 .torrent 和 .sha1 拷贝到机器 B:"
echo "     scp ${TEST_DIR}/test.torrent ${TEST_DIR}/test_data.bin.sha1 user@<机器B_IP>:~/bittorrent-test/"
echo "  2. 在各自机器上运行 DHT Sidecar + Backend:"
echo "     机器 A: bash machine_a_seed.sh"
echo "     机器 B: bash machine_b_leech.sh"
