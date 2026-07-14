# 跨机器端到端测试方案

> 通过 Tailscale 虚拟局域网，在两台计算机间完成完整的做种（seed）/ 下载（download）流程测试。Tailscale 解决 AP 隔离问题，提供稳定可达的虚拟 IP。

---

## 1. 测试概览

### 1.1 角色与节点

| 角色 | 设备 | OS | Tailscale IP | DHT P2P 端口 | gRPC 端口 | Peer 端口 |
|------|------|----|-------------|-------------|----------|----------|
| 机器 A（Seeder） | Mac | macOS（darwin） | `100.73.150.14` | 20000 | 50051 | 6881 |
| 机器 B（Leecher） | WSL | Linux（WSL2） | `100.90.54.104` | 20000 | 50052 | 6882 |

> **注意**：以上 IP 为示例值。实际测试时请替换为你的 Tailscale 分配的 IP（通过 `tailscale ip -4` 查看）。

### 1.2 测试拓扑

```
机器 A（Seeder, 100.73.150.14）         机器 B（Leecher, 100.90.54.104）
┌─────────────────────────────┐          ┌─────────────────────────────┐
│                             │          │                             │
│  DHT Sidecar (Go)          │          │  DHT Sidecar (Go)          │
│    --addr 100.73.150.14    │          │    --addr 100.90.54.104    │
│    --grpc-port 50051       │  DHT     │    --join 100.73.150.14    │
│    (创建 Kademlia 网络)      │◄───────►│    --grpc-port 50052       │
│                             │  K-Bucket│    (加入 A 的网络)          │
│  Backend (Rust)            │          │                             │
│    seed test.torrent       │  Peer    │  Backend (Rust)            │
│    --data ./data           │◄───────►│    download test.torrent    │
│    --port 6881             │  Wire    │    --output ./downloads     │
│    (全量 Bitfield, 等请求)   │  Protocol│    --port 6882             │
│                             │          │    (逐 Piece 请求并校验)     │
└─────────────────────────────┘          └─────────────────────────────┘
```

**数据流向**：
1. 机器 B 启动 download，通过 DHT（gRPC）调用 `GetPeers`，获得机器 A 的 Peer 地址 `100.73.150.14:6881`
2. 机器 B 通过 Peer Wire Protocol 连接机器 A `6881` 端口
3. 双方交换 Handshake（info_hash + PeerId）+ Bitfield
4. 机器 A 发送 Unchoke 后，机器 B 逐 Piece 发送 Request
5. 机器 A 从磁盘（PieceStore）读取数据，响应 Piece 消息
6. 机器 B 收到所有 Piece，SHA1 校验后写入磁盘

### 1.3 依赖软件

| 软件 | 最低版本 | 用途 | 安装命令 |
|------|---------|------|---------|
| Go | 1.18+ | 编译 DHT Sidecar | `brew install go`（Mac）/ `sudo apt install golang`（Linux） |
| Rust（stable） | 1.70+ | 编译 Rust Backend | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Tailscale | 任意 | 虚拟局域网 | `https://tailscale.com/download` |
| grpcurl | 任意 | gRPC 调试 | `go install github.com/fullstorydev/grpcurl/cmd/grpcurl@latest` |
| mktorrent | 任意 | 生成 .torrent | `brew install mktorrent`（Mac）/ `sudo apt install mktorrent`（Linux） |
| netcat（nc） | 任意 | TCP 连通性测试 | 系统自带 |

### 1.4 关于 IP 检测的重要说明

当前 Rust Backend 的 `detect_local_ip()` 通过 UDP connect 到 `8.8.8.8` 检测本地 IP。在 Tailscale 环境下，此方法可能返回物理网卡 IP 而非 Tailscale IP。

**CLI 必须显式使用 `--bind` 参数指定 Tailscale IP，不可依赖自动检测。**

```bash
# 正确做法 — 显式指定 Tailscale IP
./backend seed test.torrent    --data ./data    --bind 100.73.150.14 --port 6881
./backend download test.torrent --output ./out  --bind 100.90.54.104 --port 6882

# 错误做法 — 依赖自动检测（可能检测到物理网卡 IP，导致对端不可达）
./backend seed test.torrent    --data ./data --port 6881
```

DHT Sidecar 同理，`--addr` 必须使用 Tailscale IP。

---

## 2. 前置准备

### 2.1 Tailscale 安装与配置（两台机器都执行）

**macOS**：
```bash
brew install tailscale
sudo tailscale up
```

**Linux / WSL2**：
```bash
curl -fsSL https://tailscale.com/install.sh | sh
sudo tailscale up
```

**WSL2 特别注意事项**：WSL2 的网络栈与 Windows 主机共享。如果 Tailscale 在 WSL2 中无法正常工作，尝试以下步骤：

```bash
# 1. 确认 Tailscale 服务状态
sudo tailscale status

# 2. 如果 WSL2 中 tailscaled 无法启动，需要在 Windows 宿主机安装 Tailscale
#    WSL2 通过共享网络栈自动继承 Windows 的 Tailscale 虚拟网卡

# 3. 查看 Tailscale 分配的 IP
tailscale ip -4
# 应输出类似: 100.90.54.104
```

### 2.2 验证 Tailscale 互通

```bash
# 在机器 A（Mac）上 ping 机器 B（WSL）
ping 100.90.54.104
# 预期: 64 bytes from 100.90.54.104: icmp_seq=0 ttl=64 time=XX ms

# 在机器 B（WSL）上 ping 机器 A（Mac）
ping 100.73.150.14
# 预期: 64 bytes from 100.73.150.14: icmp_seq=0 ttl=64 time=XX ms

# 检查 P2P 连接类型（直连 vs DERP 中继）
tailscale ping --verbose 100.90.54.104
# 预期输出包含 "direct"（直连）或 "relay"（DERP 中继，延迟较高但功能正常）
```

### 2.3 防火墙配置

Tailscale WireGuard 隧道内的通信不经物理防火墙，但需要确认**本机**没有拦截 DHT P2P 端口和 Peer 端口。

```bash
# macOS — 关闭应用防火墙（或手动放行所需端口）
sudo /usr/libexec/ApplicationFirewall/socketfilterfw --setglobalstate off

# Linux / WSL2 — ufw 放行
sudo ufw allow 20000/tcp || true
sudo ufw allow 50051/tcp || true
sudo ufw allow 50052/tcp || true
sudo ufw allow 6881/tcp || true
sudo ufw allow 6882/tcp || true
```

---

## 3. 编译

### 3.1 自动编译（推荐）— 使用 setup.sh

**在机器 A（Mac）上**：
```bash
cd /path/to/BitTorrent
cd scripts/cross-machine
bash setup.sh --data-size-mb 10
```

**在机器 B（WSL）上**：
```bash
cd /path/to/BitTorrent
cd scripts/cross-machine
bash setup.sh --skip-build  # 如果二进制已拷贝或单独编译
# 或
bash setup.sh  # 完整编译 + 生成测试数据

# 从机器 A 拷贝 .torrent 和 SHA1 文件
scp user@100.73.150.14:~/bittorrent-test/test.torrent ~/bittorrent-test/
scp user@100.73.150.14:~/bittorrent-test/test_data.bin.sha1 ~/bittorrent-test/
```

### 3.2 手动编译

**机器 A（Mac）**：
```bash
PROJECT_ROOT="/path/to/BitTorrent"
mkdir -p ~/bittorrent-test

# 编译 DHT Sidecar
cd "${PROJECT_ROOT}/dht"
go build -o ~/bittorrent-test/dht-sidecar .

# 编译 Rust Backend
cd "${PROJECT_ROOT}/backend"
cargo build --release
cp target/release/backend ~/bittorrent-test/

# 验证
ls -lh ~/bittorrent-test/dht-sidecar ~/bittorrent-test/backend
```

**机器 B（WSL）**：同上步骤。

---

## 4. 创建测试 .torrent 文件

### 4.1 自动生成 — 由 setup.sh 完成

`setup.sh` 已自动完成以下操作。手动步骤如下供参考。

### 4.2 手动生成步骤

在 **机器 A（Mac）** 上：

```bash
cd ~/bittorrent-test

# 1. 生成 10 MiB 随机测试文件
dd if=/dev/urandom of=data/test_data.bin bs=1024 count=10240 2>/dev/null

# 2. 记录 SHA1
shasum data/test_data.bin | awk '{print $1}' > test_data.bin.sha1
cat test_data.bin.sha1
# 预期: 40 位十六进制字符串, 例如: d1c2e3...f0 (每次生成不同)

# 3. 生成 .torrent 文件
mktorrent \
  -a "http://dummy.tracker/announce" \
  -l 14 \
  -o test.torrent \
  data/test_data.bin

# 4. 验证 .torrent 文件
ls -lh test.torrent
# 预期: 约 1-2 KB 的 .torrent 文件
```

**mktorrent 参数说明**：
- `-a`：Tracker URL（可以被留空的占位值，本项目通过 DHT 发现 Peer）
- `-l 14`：piece_length = 2^14 = 16 KiB
- `-o`：输出 .torrent 文件名
- 最后参数：要共享的文件路径

### 4.3 分发 .torrent 到机器 B

```bash
# 在机器 A（Mac）上执行
scp ~/bittorrent-test/test.torrent ~/bittorrent-test/test_data.bin.sha1 \
    user@100.90.54.104:~/bittorrent-test/

# 注意：只传 .torrent 和 SHA1 文件，不传 test_data.bin 数据文件
```

---

## 5. 执行测试

### 5.1 一键自动测试（推荐）

**步骤 5.1.1 — 机器 A（Seeder）**：
```bash
cd /path/to/BitTorrent/scripts/cross-machine
bash machine_a_seed.sh 100.73.150.14
```

**步骤 5.1.2 — 机器 B（Leecher）**：
```bash
cd /path/to/BitTorrent/scripts/cross-machine
bash machine_b_leech.sh 100.73.150.14
```

**步骤 5.1.3 — 校验**：
```bash
cd /path/to/BitTorrent/scripts/cross-machine
bash verify.sh
```

### 5.2 手动分步执行（用于调试）

以下为手动分步执行的所有命令，便于理解流程或排查问题。

#### 5.2.1 启动机器 A 的 DHT Sidecar

```bash
cd ~/bittorrent-test

# 前台运行（第一个终端窗口）
./dht-sidecar \
  --addr 100.73.150.14 \
  --port 20000 \
  --grpc-port 50051
```

**预期输出**：
```
[100.73.150.14:20000] Successfully joined the Kademlia network
gRPC server listening on :50051
```

> 如果 gRPC 端口被占用，修改 `--grpc-port` 值，并同步更新后续命令。

#### 5.2.2 启动机器 A 的 Seeder

```bash
cd ~/bittorrent-test

# 新开第二个终端窗口
./backend seed test.torrent \
  --data ./data \
  --bind 100.73.150.14 \
  --port 6881 \
  --dht http://127.0.0.1:50051
```

**预期输出**：
```
Torrent: test_data.bin
Size: 10485760 bytes (10.00 MiB)
Pieces: 640
InfoHash: xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
Data directory: ./data
seeding 640 pieces, waiting for peers...
seed event loop started
```

#### 5.2.3 启动机器 B 的 DHT Sidecar

```bash
cd ~/bittorrent-test

# 前台运行（第一个终端窗口）
./dht-sidecar \
  --addr 100.90.54.104 \
  --port 20000 \
  --join 100.73.150.14:20000 \
  --grpc-port 50052
```

**预期输出**：
```
[100.90.54.104:20000] Successfully joined the Kademlia network
gRPC server listening on :50052
```

#### 5.2.4 启动机器 B 的 Download

```bash
cd ~/bittorrent-test

# 新开第二个终端窗口
./backend download test.torrent \
  --bind 100.90.54.104 \
  --port 6882 \
  --dht http://127.0.0.1:50052 \
  --output ./downloads \
  --max-peers 50
```

**预期输出**：
```
Torrent: test_data.bin
Size: 10485760 bytes (10.00 MiB)
Pieces: 640
InfoHash: xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
Output directory: ./downloads
[xxxxxx] downloading...
... (Peer Wire Protocol 交互日志)
Download complete!
Files saved to: ./downloads
```

#### 5.2.5 校验下载结果

```bash
# 在机器 B 上
cd ~/bittorrent-test

# 对比 SHA1
echo "原始 SHA1: $(cat test_data.bin.sha1)"
shasum downloads/test_data.bin

# 应打印相同的 40 位十六进制值
```

---

## 6. 配套 Shell 脚本说明

所有脚本位于 `scripts/cross-machine/` 目录下。

| 脚本 | 作用 | 需要运行的机器 |
|------|------|-------------|
| `setup.sh` | 编译 DHT Sidecar + Backend，生成测试数据，创建 .torrent | 两台都运行 |
| `machine_a_seed.sh` | 后台启动 DHT Sidecar + Seeder | 机器 A |
| `machine_b_leech.sh` | 后台启动 DHT Sidecar + 前台 Download | 机器 B |
| `verify.sh` | SHA1 对比原始文件与下载文件 | 机器 B |
| `cleanup.sh` | 停止所有进程，清理临时日志 | 任意机器 |

### 6.1 setup.sh

```bash
bash setup.sh [--skip-build] [--data-size-mb N]
```

| 参数 | 说明 | 默认值 |
|------|------|--------|
| `--skip-build` | 跳过编译（二进制已就绪时使用） | false |
| `--data-size-mb N` | 测试文件大小（MiB） | 10 |

**执行产物**（在 `~/bittorrent-test/` 下）：
```
~/bittorrent-test/
├── dht-sidecar          # Go 编译的 DHT Sidecar 二进制
├── backend              # Rust 编译的 Backend 二进制
├── test.torrent         # .torrent 文件
├── test_data.bin.sha1   # 原始文件的 SHA1
└── data/
    └── test_data.bin    # 随机测试数据（仅机器 A 需要）

~/bittorrent-test/downloads/   # Leecher 下载输出目录（Download 时自动创建）
```

### 6.2 machine_a_seed.sh

```bash
bash machine_a_seed.sh [TAILSCALE_IP]
```

该脚本执行以下操作：
1. 检查所有必要文件是否存在
2. 清理残留的 dht-sidecar / backend 进程
3. 后台启动 DHT Sidecar（`--addr <TAILSCALE_IP> --grpc-port 50051`）
4. 轮询等待 DHT Sidecar 的 gRPC 服务就绪（最多 15 秒）
5. 后台启动 Seeder（`seed test.torrent --data ./data --bind <TAILSCALE_IP> --port 6881`）

日志文件：
- `/tmp/dht-seed.log` — DHT Sidecar 输出
- `/tmp/seed.log` — Seeder 输出

### 6.3 machine_b_leech.sh

```bash
bash machine_b_leech.sh <机器A的Tailscale_IP>
```

该脚本执行以下操作：
1. 检查所有必要文件是否存在（.torrent 需从机器 A scp 过来）
2. **网络预检**：ping 机器 A 的 Tailscale IP
3. 清理残留进程
4. 后台启动 DHT Sidecar（`--addr <本机IP> --join <A_IP>:20000 --grpc-port 50052`）
5. 轮询等待 DHT Sidecar gRPC 就绪（最多 20 秒）
6. **DHT 连通性验证**：执行一次 gRPC `Put` 操作确认 DHT 网络正常
7. **前台启动 Download**（`download test.torrent --output ./downloads`），日志实时 tee 到 `/tmp/download.log`
8. Download 完成后打印退出码和输出文件列表

> **注意**：Download 在前台运行，可以实时看到进度日志。Download 完成后进程退出，DHT Sidecar 仍在后台。

### 6.4 verify.sh

```bash
bash verify.sh [--download-dir <path>]
```

校验项目：
1. 下载目录中是否存在文件
2. 下载文件 SHA1 是否与 `test_data.bin.sha1` 中的值一致
3. 文件大小是否与原始文件一致
4. .torrent 文件基本格式检查

### 6.5 cleanup.sh

```bash
bash cleanup.sh [--clean-all]
```

| 参数 | 说明 |
|------|------|
| 无参数 | 停止进程 + 清理 `/tmp/dht-*.log` 等临时日志，保留 `~/bittorrent-test/` |
| `--clean-all` | 额外删除 `~/bittorrent-test/` 目录 |

---

## 7. 预期输出详解

### 7.1 DHT Sidecar 启动（机器 A）

```
[100.73.150.14:20000] Successfully joined the Kademlia network
gRPC server listening on :50051
```

如果看到 `Successfully joined`，表示 DHT 网络已创建。

### 7.2 DHT Sidecar 启动（机器 B）

```
[100.90.54.104:20000] Successfully joined the Kademlia network
gRPC server listening on :50052
```

机器 B 的日志应包含 `joined`，表示已成功通过 `--join` 加入机器 A 的网络。

### 7.3 Seeder 启动（机器 A）

```
Torrent: test_data.bin
Size: 10485760 bytes (10.00 MiB)
Pieces: 640
InfoHash: <40 位 hex>
Data directory: ./data
seeding 640 pieces, waiting for peers...
seed event loop started
```

关键信息：
- `Pieces: 640` — 对应 10 MiB / 16 KiB（piece_length = 2^14 = 16384 bytes）≈ 640 块
- `seeding 640 pieces` — 表示所有 640 个 Piece 已通过 SHA1 校验，Bitfield 全满
- `waiting for peers...` — Seeder 已就绪，等待机器 B 连接

### 7.4 Download 过程（机器 B）

**阶段 1 — 连接 & 握手**：
```
Torrent: test_data.bin
Size: 10485760 bytes (10.00 MiB)
Pieces: 640
InfoHash: <40 位 hex>
Output directory: ./downloads
[<short_hash>] downloading...
```

日志中可能出现的信息（取决于 tracing level）：
- `HandshakeComplete` — 与 Seeder 握手完成
- `ReceivedMessage: Bitfield` — 收到 Seeder 的全满 Bitfield
- `ReceivedMessage: Unchoke` — Seeder 解除了对我们的 Choke
- `Request { index: 0, begin: 0, length: 16384 }` — 向 Seeder 请求第一个 Block
- `ReceivedMessage: Piece { index: 0, begin: 0 }` — 收到 Seeder 响应的 Piece 数据
- `have` — 每完成一个 Piece 后广播 Have 消息

**阶段 2 — 完成**：
```
download complete: 640/640 pieces
Download complete!
Files saved to: ./downloads
```

### 7.5 SHA1 校验（机器 B）

```bash
$ cat test_data.bin.sha1
d1c2e3f4...（40 位十六进制）

$ shasum downloads/test_data.bin
d1c2e3f4...（40 位十六进制）downloads/test_data.bin
```

两者应完全一致。若不一致，表明数据在传输过程中损坏。

### 7.6 完整 verify.sh 输出示例

```
==============================================
  跨机器传输结果校验
==============================================

原始 SHA1 记录: /Users/user/bittorrent-test/test_data.bin.sha1
原始文件:       /Users/user/bittorrent-test/data/test_data.bin
下载目录:       /Users/user/bittorrent-test/downloads

下载目录中的文件:
  -> /Users/user/bittorrent-test/downloads/test_data.bin

预期的 SHA1: d1c2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9d0

--- 校验: /Users/user/bittorrent-test/downloads/test_data.bin ---
  实际 SHA1: d1c2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9d0
  结果: PASS

  原始文件大小: 10485760 bytes
  下载文件大小: 10485760 bytes
  大小对比:     PASS

--- .torrent 元数据校验 ---
  .torrent 文件: /Users/user/bittorrent-test/test.torrent
  .torrent 格式: 合法 bencode 文件

==============================================
  校验结果: ALL PASS
  跨机器文件传输完整性验证通过!
==============================================
```

---

## 8. 中间状态检查点

### 8.1 检查点 1：DHT Sidecar 进程存活

```bash
# 查看 DHT Sidecar 进程
pgrep -f dht-sidecar
# 预期: 返回 PID（如 12345）

# 查看日志
tail -f /tmp/dht-seed.log    # 机器 A
tail -f /tmp/dht-leech.log   # 机器 B
```

### 8.2 检查点 2：gRPC 服务可达

```bash
# 在各自机器上
grpcurl -plaintext 127.0.0.1:50051 list  # 机器 A
# 预期: dht.DhtService  grpc.reflection.v1alpha.ServerReflection

grpcurl -plaintext 127.0.0.1:50052 list  # 机器 B
# 预期: dht.DhtService  grpc.reflection.v1alpha.ServerReflection
```

### 8.3 检查点 3：DHT 跨机器连通（gRPC Put/Get）

```bash
# 机器 B 写入
grpcurl -plaintext -d '{"key":"ping","value":"pong"}' \
  127.0.0.1:50052 dht.DhtService/Put
# 预期: {"success": true}

# 机器 A 读取（需要等待 DHT 传播，约 2-5 秒）
sleep 3
grpcurl -plaintext -d '{"key":"ping"}' \
  127.0.0.1:50051 dht.DhtService/Get
# 预期: {"found": true, "value": "pong"}
```

### 8.4 检查点 4：Peer TCP 连通

```bash
# 机器 A：用 nc 监听 Peer 端口
nc -l 6881

# 机器 B：测试连接
nc -zv 100.73.150.14 6881
# 预期: Connection to 100.73.150.14 6881 port [tcp/*] succeeded!
```

### 8.5 检查点 5：DHT Peer 宣告与发现

Seeker/Leecher 在启动时会自动调用 `AnnouncePeer`，无需手动操作。如需独立验证：

```bash
# 在机器 B 查看 DHT 中注册的 Peers（info_hash 使用 base64 编码的 20 字节）
# 获取 info_hash 的方法：
# python3 -c "import hashlib,base64; h=hashlib.sha1(open('~/bittorrent-test/data/test_data.bin','rb').read()).hexdigest(); print(h)"

grpcurl -plaintext -d "{\"info_hash\":\"<base64_编码的_info_hash>\"}" \
  127.0.0.1:50052 dht.DhtService/GetPeers
# 预期: {"peers": [{"ip": "100.73.150.14", "port": 6881}], "found": true}
```

---

## 9. 故障排查

### 9.1 常见问题表

| 现象 | 可能原因 | 排查命令 |
|------|---------|---------|
| Tailscale IP ping 不通 | ① 对端 tailscale 未登录 ② 防火墙拦截 ③ WSL 网络模式问题 | `tailscale status` 确认两端 online；`tailscale ping --verbose <对端IP>` |
| DHT join 超时（30 retries） | ① `--addr` 不是 Tailscale IP ② 机器 A 的 DHT Sidecar 未先启动 ③ DHT 端口 20000 被防火墙拦截 | `nc -zv <对端IP> 20000` |
| gRPC 连接超时 | ① gRPC 端口被占用 ② 防火墙拦截 ③ WSL 中 localhost 指向 Windows 宿主机 | `ss -tlnp \| grep 5005`（Linux）/ `lsof -i :50051`（macOS） |
| `get_peers` 返回空 | ① DHT 尚未传播 Peers 信息 ② info_hash 编码错误 ③ Seeder 未正常 announce | 等待 10 秒后重试；确认 Seeder 日志中有 announce 成功日志 |
| Peer TCP 连接超时 | ① `--bind` 未使用 Tailscale IP ② Peer 端口被防火墙拦截 | `nc -zv <对端IP> 6881` |
| Download 立即完成但文件大小为 0 | Seeder 未运行或 DHT 未发现 Peer；检查 Seeder 进程和 DHT 日志 |
| SHA1 校验失败 | 数据在传输中损坏（极低概率） | 重新执行 Download；检查 Seeder 端原始文件是否完整 |
| WSL2 Tailscale 无法启动 | WSL2 内核缺少 `/dev/net/tun` | 在 Windows 宿主机安装 Tailscale，WSL2 自动继承网络栈 |

### 9.2 诊断命令速查表

```bash
# ── Tailscale ──
tailscale status                          # 查看所有节点在线状态
tailscale ip -4                           # 查看本机 Tailscale IP
tailscale ping --verbose <对端IP>          # 查看 P2P 连接类型（direct / relay）

# ── 进程检查 ──
pgrep -af dht-sidecar                     # 列出所有 dht-sidecar 进程
pgrep -af backend                         # 列出所有 backend 进程
ps aux | grep -E 'dht-sidecar|backend'    # 详细进程信息

# ── 端口检查 ──
ss -tlnp | grep -E '20000|5005[12]|688[12]'    # Linux 端口监听状态
lsof -i :50051                                   # macOS 端口占用

# ── TCP 连通性 ──
nc -zv 100.73.150.14 20000               # 测试 DHT P2P 端口
nc -zv 100.73.150.14 50051               # 测试 gRPC 端口（机器 A）
nc -zv 100.90.54.104 50052               # 测试 gRPC 端口（机器 B）
nc -zv 100.73.150.14 6881                # 测试 Peer 端口（机器 A）

# ── gRPC 连通性 ──
grpcurl -plaintext 127.0.0.1:50051 list  # 列出机器 A 的 gRPC 服务
grpcurl -plaintext 127.0.0.1:50052 list  # 列出机器 B 的 gRPC 服务

# ── 日志查看 ──
tail -f /tmp/dht-seed.log                # 机器 A DHT Sidecar 日志
tail -f /tmp/dht-leech.log               # 机器 B DHT Sidecar 日志
tail -f /tmp/seed.log                     # 机器 A Seeder 日志
tail -f /tmp/download.log                 # 机器 B Download 日志（未 tee 时无此文件）
tail -f dht-test.log                      # DHT 详细调试日志（项目根目录）

# ── 进程清理 ──
pkill -f dht-sidecar                      # 停止所有 DHT Sidecar
pkill -f "./backend"                      # 停止所有 Backend
# 或使用 cleanup.sh
bash scripts/cross-machine/cleanup.sh
```

### 9.3 Tailscale DERP 中继说明

当 Tailscale 无法建立直接的 P2P 隧道时（如双方都在严格 NAT 后面），会通过 DERP（Designated Encrypted Relay for Packets）中继转发流量。这会增加延迟（通常 100-300ms），但功能上透明无差别。

```bash
# 检查当前连接方式
tailscale ping --verbose 100.90.54.104
# "direct" = 直连 P2P（低延迟）
# "relay" = DERP 中继（高延迟但可用）
```

---

## 10. 完整测试检查清单

测试前确认：
- [ ] 两台机器 Tailscale 状态均为 `active`（`tailscale status` 确认）
- [ ] Tailscale IP 双向 ping 通
- [ ] Go 和 Rust 工具链已安装
- [ ] `grpcurl` 已安装并可执行
- [ ] `mktorrent` 已安装
- [ ] `nc`（netcat）可用
- [ ] `setup.sh` 在两台机器上执行成功
- [ ] `.torrent` 和 `.sha1` 已分发到机器 B

测试中确认：
- [ ] 机器 A：DHT Sidecar 启动，日志显示 `Successfully joined the Kademlia network`
- [ ] 机器 A：Seeder 启动，日志显示 `seeding <N> pieces, waiting for peers...`
- [ ] 机器 B：DHT Sidecar 启动，日志显示 `Successfully joined the Kademlia network`
- [ ] 机器 B：gRPC 连通性验证通过（Put/Get 跨机器）
- [ ] 机器 B：Download 启动，日志显示正常的 Handshake / Bitfield / Piece 交互
- [ ] 机器 B：Download 最终输出 `Download complete!`
- [ ] 机器 B：`verify.sh` 输出 `ALL PASS`

测试后确认：
- [ ] 下载文件 SHA1 与原始文件 SHA1 完全一致
- [ ] 下载文件大小与原始文件大小一致
- [ ] 无异常日志（无 `SHA1 mismatch`、`peer disconnected`、`DHT refresh failed` 等错误）

---

## 11. 附录：Tailscale IP 速查

以下为实际使用的节点 IP 记录（运行当天填写）：

| 角色 | 设备 | Tailscale IP |
|------|------|-------------|
| 机器 A（Seeder） | Mac | `___` |
| 机器 B（Leecher） | WSL | `___` |

获取方式：
```bash
tailscale ip -4
```
