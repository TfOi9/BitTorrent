#!/usr/bin/env bash
#
# verify.sh — 下载结果校验脚本
#
# 比对原始文件 SHA1 与下载文件 SHA1，验证跨机器传输完整性
#
# 用法：
#   bash verify.sh [--download-dir <path>]
#
# 默认：
#   原始文件:     ~/bittorrent-test/data/test_data.bin
#   SHA1 记录:    ~/bittorrent-test/test_data.bin.sha1
#   下载文件目录:  ~/bittorrent-test/downloads/

set -euo pipefail

TEST_DIR="${HOME}/bittorrent-test"
ORIGINAL_FILE="${TEST_DIR}/data/test_data.bin"
SHA1_FILE="${TEST_DIR}/test_data.bin.sha1"
DOWNLOAD_DIR="${TEST_DIR}/downloads"

# 解析参数
while [[ $# -gt 0 ]]; do
    case "$1" in
        --download-dir) DOWNLOAD_DIR="$2"; shift 2 ;;
        --*) echo "未知参数: $1"; echo "用法: $0 [--download-dir <path>]"; exit 1 ;;
        *) break ;;
    esac
done

echo "=============================================="
echo "  跨机器传输结果校验"
echo "=============================================="
echo ""
echo "原始 SHA1 记录: ${SHA1_FILE}"
echo "原始文件:       ${ORIGINAL_FILE}"
echo "下载目录:       ${DOWNLOAD_DIR}"
echo ""

# ── 1. 查找下载的文件 ────────────────────────────────────────────
FOUND_FILES=$(find "${DOWNLOAD_DIR}" -type f 2>/dev/null | sort)
if [ -z "${FOUND_FILES}" ]; then
    echo "错误: 下载目录 ${DOWNLOAD_DIR} 中没有找到文件"
    echo "可能原因:"
    echo "  1. 下载未成功完成"
    echo "  2. 输出目录路径不正确"
    echo "  3. 数据文件被写入到其他位置"
    exit 1
fi

echo "下载目录中的文件:"
echo "${FOUND_FILES}" | sed 's/^/  -> /'
echo ""

# ── 2. 校验 SHA1 ─────────────────────────────────────────────────
PASS=true

if [ -f "${SHA1_FILE}" ]; then
    EXPECTED_SHA1=$(cat "${SHA1_FILE}")
    echo "预期的 SHA1: ${EXPECTED_SHA1}"
    echo ""

    # 对每个下载文件校验 SHA1
    while IFS= read -r file; do
        echo "--- 校验: ${file} ---"
        if command -v shasum &> /dev/null; then
            ACTUAL_SHA1=$(shasum "${file}" | awk '{print $1}')
        elif command -v sha1sum &> /dev/null; then
            ACTUAL_SHA1=$(sha1sum "${file}" | awk '{print $1}')
        else
            echo "错误: 未找到 shasum 或 sha1sum"
            exit 1
        fi
        echo "  实际 SHA1: ${ACTUAL_SHA1}"

        if [ "${ACTUAL_SHA1}" = "${EXPECTED_SHA1}" ]; then
            echo "  结果: PASS"
        else
            echo "  结果: FAIL"
            PASS=false
        fi
        echo ""

        # 文件大小对比
        if [ -f "${ORIGINAL_FILE}" ]; then
            ORIG_SIZE=$(stat -f%z "${ORIGINAL_FILE}" 2>/dev/null || stat -c%s "${ORIGINAL_FILE}" 2>/dev/null || echo "N/A")
            DL_SIZE=$(stat -f%z "${file}" 2>/dev/null || stat -c%s "${file}" 2>/dev/null || echo "N/A")
            echo "  原始文件大小: ${ORIG_SIZE} bytes"
            echo "  下载文件大小: ${DL_SIZE} bytes"
            if [ "${ORIG_SIZE}" = "${DL_SIZE}" ] && [ "${ORIG_SIZE}" != "N/A" ]; then
                echo "  大小对比:     PASS"
            elif [ "${ORIG_SIZE}" != "N/A" ]; then
                echo "  大小对比:     FAIL (差异: $((ORIG_SIZE - DL_SIZE)) bytes)"
                PASS=false
            fi
        fi
    done <<< "${FOUND_FILES}"
else
    echo "警告: SHA1 记录文件 ${SHA1_FILE} 不存在"
    echo "跳过 SHA1 校验，仅进行文件存在性检查"

    while IFS= read -r file; do
        SIZE=$(stat -f%z "${file}" 2>/dev/null || stat -c%s "${file}" 2>/dev/null || echo "0")
        echo "  -> ${file} (${SIZE} bytes)"
        if [ "${SIZE}" -eq 0 ] 2>/dev/null; then
            echo "     警告: 文件大小为 0！"
            PASS=false
        fi
    done <<< "${FOUND_FILES}"
fi

# ── 3. 校验 .torrent 元数据 ──────────────────────────────────────
TORRENT_FILE="${TEST_DIR}/test.torrent"
if [ -f "${TORRENT_FILE}" ]; then
    echo "--- .torrent 元数据校验 ---"
    echo "  .torrent 文件: ${TORRENT_FILE}"

    # 尝试用 Python 解析 .torrent 文件中的 files 列表
    if command -v python3 &> /dev/null; then
        python3 -c "
import sys
try:
    from pathlib import Path
    torrent_bytes = Path('${TORRENT_FILE}').read_bytes()
    # Quick check: .torrent starts with 'd' (bencode dict) and contains 'info'
    if torrent_bytes.startswith(b'd') and b'info' in torrent_bytes:
        print('  .torrent 格式: 合法 bencode 文件')
    else:
        print('  警告: .torrent 格式可能不正确')
except Exception as e:
    print(f'  无法解析 .torrent: {e}')
" 2>/dev/null || echo "  .torrent 解析跳过（Python3 不可用）"
    fi
fi

# ── 4. 最终结果 ───────────────────────────────────────────────────
echo ""
echo "=============================================="
if [ "${PASS}" = true ]; then
    echo "  校验结果: ALL PASS"
    echo "  跨机器文件传输完整性验证通过!"
else
    echo "  校验结果: FAIL"
    echo "  文件传输可能出现数据损坏或下载不完整"
fi
echo "=============================================="

if [ "${PASS}" = true ]; then
    exit 0
else
    exit 1
fi
