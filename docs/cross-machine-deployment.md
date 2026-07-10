# DHT 跨机器部署与测试指南

> 如何在不同计算机之间组建 Kademlia DHT 网络并通过 gRPC 测试。

---

## 目录

1. [前置条件](#1-前置条件)
2. [架构原理](#2-架构原理)
3. [代码修改说明](#3-代码修改说明)
4. [部署步骤](#4-部署步骤)
5. [功能验证](#5-功能验证)
6. [常见问题排查](#6-常见问题排查)
7. [NAT / 容器 / 云环境注意事项](#7-nat--容器--云环境注意事项)

---

## 1. 前置条件

| 条件 | 说明 |
|---|---|
| **两台或多台计算机** | 需在同一局域网（或通过 VPN / 端口映射可达） |
| **Go 1.18+** | 编译 DHT Sidecar |
| **grpcurl** | gRPC 命令行测试工具 (`go install github.com/fullstorydev/grpcurl/cmd/grpcurl@latest`) |
| **开放端口** | DHT P2P 端口（默认 20000/tcp）和 gRPC 端口（如 50051/tcp）不能被防火墙拦截 |

---

## 2. 架构原理

```
机器 A                           机器 B
192.168.1.10                     192.168.1.20
┌──────────────────┐             ┌──────────────────┐
│  DHT Sidecar     │             │  DHT Sidecar     │
│                  │   P2P TCP   │                  │
│  监听 :20000 ◄───┼─────────────┼─── 监听 :20000   │
│  gRPC :50051     │             │  gRPC :50052     │
│                  │             │                  │
│  宣告地址:       │             │  宣告地址:       │
│  192.168.1.10    │             │  192.168.1.20    │
│  :20000          │             │  :20000          │
└──────────────────┘             └──────────────────┘
```

**关键设计**：
- **宣告地址**（`--addr`）用于 NodeID 生成和路由表分享 — 必须是**对端可达的 IP**
- **监听地址**固定为 `0.0.0.0:<port>`（即 `:<port>`）— 接受来自所有网络接口的连接
- **NodeID** = `SHA1("192.168.1.10:20000")` — 每台机器有唯一身份

---

## 3. 代码修改说明

`kademlia/kademlia.go` 中 `Run` 方法的监听逻辑已修改：

```go
// 修改前（只能本机访问）
node.listener, err = net.Listen("tcp", node.Addr)

// 修改后（接受所有网络接口的连接）
_, port, _ := net.SplitHostPort(node.Addr)
node.listener, err = net.Listen("tcp", ":"+port)
```

`node.Addr`（宣告地址）仍用于：
- SHA1 哈希生成 NodeID
- 路由表中的 `Contact.Addr` — 其他节点用此地址连接

gRPC server（`main.go` 中 `startGRPCServer`）已使用 `:port` 格式，无需修改。

---

## 4. 部署步骤

### 4.1 编译

在每台机器上执行：

```bash
cd /path/to/BitTorrent
go build -o dht-sidecar .
```

### 4.2 启动节点

**机器 A**（IP: 192.168.1.10）— 创建网络：

```bash
./dht-sidecar --addr 192.168.1.10 --port 20000 --grpc-port 50051
```

> `--addr` **必须是机器 A 的实际局域网 IP，不要用 127.0.0.1**。

**机器 B**（IP: 192.168.1.20）— 加入网络：

```bash
./dht-sidecar --addr 192.168.1.20 --port 20000 --join 192.168.1.10:20000 --grpc-port 50052
```

> `--addr` 是机器 B 自己的 IP；`--join` 是机器 A 的地址。

**机器 C、D、...** — 加入网络：

```bash
./dht-sidecar --addr <本机IP> --port 20000 --join <任意已在线节点的IP>:20000 --grpc-port 50053
```

### 4.3 后台运行（headless 模式）

```bash
# 使用 nohup 或 screen/tmux
nohup ./dht-sidecar --addr 192.168.1.10 --port 20000 --grpc-port 50051 \
  > /tmp/dht.log 2>&1 &

# 查看日志
tail -f /tmp/dht.log
# 查看 DHT 详细日志
tail -f dht-test.log
```

stdin 关闭时（后台运行必然发生），gRPC 服务继续运行，等待 `Ctrl-C` 信号优雅退出。

### 4.4 防火墙配置

```bash
# DHT 节点间 P2P 通信端口
sudo ufw allow 20000/tcp

# gRPC 端口（仅需要从其他机器调用 gRPC 时）
sudo ufw allow 50051/tcp
sudo ufw allow 50052/tcp
```

---

## 5. 功能验证

### 5.1 确认节点互联

查看机器 B 的日志 `dht-test.log`，应有：

```
[192.168.1.20:20000] Successfully joined the Kademlia network
```

### 5.2 K/V 跨节点读写

在机器 B 上写入：

```bash
grpcurl -plaintext -d '{"key":"greeting","value":"hello from B"}' \
  localhost:50052 dht.DhtService/Put
# → {"success": true}
```

在机器 A 上读取（数据通过 DHT 迭代查找从 B 获取）：

```bash
grpcurl -plaintext -d '{"key":"greeting"}' \
  localhost:50051 dht.DhtService/Get
# → {"found": true, "value": "hello from B"}
```

### 5.3 跨机器 Peer 发现

```bash
# 定义 info_hash（40 hex chars = 20 bytes，base64 编码）
# 原始 hex: deadbeef12345678deadbeef12345678deadbeef
INFO_HASH_B64="3q2+7xI0Vnjerb7vEjRWeN6tvu8="
```

在机器 B 上宣告自己为 Peer：

```bash
grpcurl -plaintext -d "{\"info_hash\":\"${INFO_HASH_B64}\",\"peer\":{\"ip\":\"192.168.1.20\",\"port\":6881}}" \
  localhost:50052 dht.DhtService/AnnouncePeer
# → {"success": true}
```

在机器 A 上查找 Peer：

```bash
grpcurl -plaintext -d "{\"info_hash\":\"${INFO_HASH_B64}\"}" \
  localhost:50051 dht.DhtService/GetPeers
# → {"peers": [{"ip": "192.168.1.20", "port": 6881}], "found": true}
```

### 5.4 验证路由表收敛

启动 3+ 个节点后，每个节点的 k-bucket 会逐渐填充：

```bash
# 使用 grpcurl + list services 检查 gRPC 是否存活
for ip in 192.168.1.10 192.168.1.20 192.168.1.30; do
  echo "=== $ip ==="
  grpcurl -plaintext $ip:50051 list 2>&1 | head -1
done
```

---

## 6. 常见问题排查

| 现象 | 可能原因 | 排查命令 |
|---|---|---|
| Join 失败（重试 30 次后超时） | ① `--addr` 用了 `127.0.0.1` ② 防火墙拦截 ③ 目标机器未启动 | `nc -zv <目标IP> 20000` |
| Put 返回 `{}`（`success: false`） | 网络中无其他节点（单节点 Put 需要 K 个最近节点） | 至少 2 个节点组成网络 |
| Get 返回 `{}`（`found: false`） | 数据未写入、或 key 错误 | 确认 Put 返回 `{"success":true}` |
| gRPC 连接超时 | ① 防火墙拦截 gRPC 端口 ② `--grpc-port 0` | `ss -tlnp \| grep <grpc-port>` |
| NodeID 相同 | 两台机器用了相同 `--addr` | 确保每台机器 `--addr` 不同（不同 IP 或不同端口） |
| `address already in use` | 之前的进程未退出 | `pkill dht-sidecar` |

### 6.1 诊断命令速查

```bash
# 检查端口是否在监听
ss -tlnp | grep -E '20000|50051|50052'

# 测试 TCP 连通性
nc -zv 192.168.1.10 20000

# 测试 gRPC 连通性  
grpcurl -plaintext 192.168.1.10:50051 list

# 查看 DHT 详细日志
tail -100 dht-test.log

# 查看 sidecar 输出日志
tail -100 /tmp/dht.log
```

---

## 7. NAT / 容器 / 云环境注意事项

### 7.1 NAT 后面

如果机器在 NAT 后面（如家用路由器），`--addr` 应设置为：

- **局域网内互访**：用局域网 IP（如 `192.168.1.x`），局域网内机器可互通
- **跨 NAT / 公网访问**：需要端口映射 + `--addr` 设为公网 IP，或使用 STUN/UPnP

### 7.2 Docker 容器

```bash
# 容器需映射端口并使用宿主机 IP 作为 --addr
docker run -p 20000:20000 -p 50051:50051 dht-sidecar \
  --addr <宿主机IP> --port 20000 --grpc-port 50051
```

容器内 `--addr` 不能用 `127.0.0.1` 或容器内 IP，必须用**宿主机的外部可达 IP**。

### 7.3 云服务器

云服务器通常有**公网 IP** 和**内网 IP**：
- 同一 VPC 内互访：用内网 IP 作为 `--addr`（延迟低、免流量费）
- 跨 VPC / 公网访问：用公网 IP，在安全组中放行端口

### 7.4 多网卡机器

如果机器有多个 IP（如 `192.168.1.10` 和 `10.0.0.10`）：

```bash
# 选择对端可达的 IP
./dht-sidecar --addr 192.168.1.10 --port 20000 --grpc-port 50051
```

监听 bind 到 `0.0.0.0`，所以两个网卡的 IP 都能接收连接。但宣告地址 `192.168.1.10` 会被分享给其他节点 — 如果对端在 `10.0.0.0/24` 网段，它们无法通过 `192.168.1.10` 连接，需要改用 `10.0.0.10`。

---

## 附录：完整参数参考

```
./dht-sidecar --help

  --addr       string   宣告给其他节点的 IP 地址（默认 127.0.0.1）
  --port       int      DHT P2P 监听端口（默认 20000）
  --join       string   引导节点地址 ip:port，空表示创建新网络
  --grpc-port  int      gRPC API 端口，0 表示禁用（默认 0）
  --cmd-port   int      CLI 文本命令端口，0 表示禁用（默认 0）
```

---

## 保障回环测试不受影响

修改后 `127.0.0.1` 场景仍然正常工作：

```bash
# 单机多节点测试 — 启动 3 个节点
./dht-sidecar --addr 127.0.0.1 --port 20000 --grpc-port 50051 &
./dht-sidecar --addr 127.0.0.1 --port 20001 --join 127.0.0.1:20000 --grpc-port 50052 &
./dht-sidecar --addr 127.0.0.1 --port 20002 --join 127.0.0.1:20000 --grpc-port 50053 &

# 通过不同 gRPC 端口测试
grpcurl -plaintext -d '{"key":"k","value":"v"}' localhost:50051 dht.DhtService/Put
grpcurl -plaintext -d '{"key":"k"}' localhost:50052 dht.DhtService/Get
```

因为所有节点监听 `:port`（即 `0.0.0.0:port`），`127.0.0.1` 连接仍然有效。
