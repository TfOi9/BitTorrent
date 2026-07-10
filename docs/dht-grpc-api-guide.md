# DHT gRPC API — 上层应用开发指南

> **适用语言**：Rust（推荐）、C++、或任何支持 gRPC 的语言  
> **协议**：gRPC + Protocol Buffers 3  
> **传输**：TCP / localhost（sidecar 模式）

---

## 目录

1. [架构概览](#1-架构概览)
2. [快速开始：启动 DHT Sidecar](#2-快速开始启动-dht-sidecar)
3. [Proto 服务参考](#3-proto-服务参考)
   - [3.1 Put](#31-put)
   - [3.2 Get](#32-get)
   - [3.3 Delete](#33-delete)
   - [3.4 GetPeers](#34-getpeers)
   - [3.5 AnnouncePeer](#35-announcepeer)
4. [Rust 客户端集成](#4-rust-客户端集成)
5. [BitTorrent 集成模式](#5-bittorrent-集成模式)
6. [数据约定](#6-数据约定)
7. [错误处理](#7-错误处理)
8. [调试与故障排除](#8-调试与故障排除)

---

## 1. 架构概览

```
┌──────────────────────────────────────────────────┐
│  你的 BitTorrent 客户端 (Rust / C++ / ...)       │
│                                                   │
│  ┌─────────────────────────────────────────────┐ │
│  │        gRPC Client Stub                     │ │
│  │   (从 dht.proto 自动生成)                    │ │
│  └──────────────────┬──────────────────────────┘ │
└────────────────────┼─────────────────────────────┘
                     │ gRPC over TCP (localhost)
┌────────────────────┼─────────────────────────────┐
│   Go DHT Sidecar 进程                            │
│                    │                              │
│  ┌─────────────────┴──────────────────────────┐  │
│  │  gRPC Server  (bridge/server.go)            │  │
│  │  翻译 protobuf ↔ DhtNode 接口               │  │
│  └─────────────────┬──────────────────────────┘  │
│                    │                              │
│  ┌─────────────────┴──────────────────────────┐  │
│  │  KademliaNode  (kademlia/)                  │  │
│  │  • 160 k-buckets 路由表                     │  │
│  │  • XOR 距离迭代查找                         │  │
│  │  • K/V 存储 + Peer 存储                     │  │
│  └─────────────────┬──────────────────────────┘  │
│                    │ Go net/rpc (P2P 网络)        │
└────────────────────┼─────────────────────────────┘
                     │
          ┌──────────┼──────────┐
     ┌────┴────┐ ┌───┴───┐ ┌───┴────┐
     │ DHT     │ │ DHT   │ │ DHT    │   ...
     │ Node    │ │ Node  │ │ Node   │
     └─────────┘ └───────┘ └────────┘
```

**核心原则**：

- Go DHT Sidecar 是一个**独立进程**，你的客户端通过 localhost gRPC 调用它
- **Token 防 Sybil 攻击机制完全在 DHT 内部处理** — 你不需要管理 token
- 每个 BitTorrent 客户端实例启动一个对应的 DHT Sidecar
- Sidecar 负责：路由维护、K/V 复制、Peer 发现、节点加入/退出

---

## 2. 快速开始：启动 DHT Sidecar

### 编译

```bash
cd /path/to/BitTorrent
go build -o dht-sidecar .
```

### 启动

```bash
# 创建新网络（第一个节点）
./dht-sidecar --port 20000 --grpc-port 50051

# 加入已有网络
./dht-sidecar --port 20001 --join 127.0.0.1:20000 --grpc-port 50052
```

### 参数说明

| 参数 | 默认值 | 说明 |
|---|---|---|
| `--port` | `20000` | DHT 节点 P2P 端口（节点间通信用） |
| `--addr` | `127.0.0.1` | 宣告给其他节点的 IP 地址 |
| `--join` | `""` | 引导节点地址（`ip:port`），空表示创建新网络 |
| `--grpc-port` | `0` | gRPC 服务端口，`0` 表示不启动 gRPC |
| `--cmd-port` | `0` | CLI 文本命令端口（调试用），`0` 表示不启动 |

### 验证服务可用

```bash
# 使用 grpcurl 测试（需要安装 grpcurl）
grpcurl -plaintext localhost:50051 list
# 输出: dht.DhtService

grpcurl -plaintext localhost:50051 dht.DhtService/Put \
  -d '{"key":"test","value":"hello"}'
# 输出: { "success": true }

grpcurl -plaintext localhost:50051 dht.DhtService/Get \
  -d '{"key":"test"}'
# 输出: { "found": true, "value": "hello" }
```

---

## 3. Proto 服务参考

完整的 proto 文件位于 `proto/dht.proto`，供你拷贝到上层项目中用于代码生成。

### 3.1 Put

存储一个键值对到 DHT。数据会被复制到 K（=20）个最接近 key 的节点上。

```
rpc Put(PutRequest) returns (PutResponse);
```

**请求**：

```protobuf
message PutRequest {
  string key   = 1;  // 任意字符串 key
  string value = 2;  // 任意字符串 value
}
```

**响应**：

```protobuf
message PutResponse {
  bool success = 1;  // true = 至少有一个节点确认存储
}
```

**语义**：
- `success = true`：数据已复制到至少 1 个节点（通常 K 个）
- `success = false`：网络异常，无一节点可达
- Key 通过 SHA1 哈希映射到 160 位 DHT 地址空间
- Value 是 opaque string，DHT 不解析其内容

**典型用法**：存储 torrent 元数据（bencoded info dict）、DHT 爬虫数据等。

---

### 3.2 Get

从 DHT 检索键值对。

```
rpc Get(GetRequest) returns (GetResponse);
```

**请求**：

```protobuf
message GetRequest {
  string key = 1;
}
```

**响应**：

```protobuf
message GetResponse {
  bool   found = 1;   // 是否找到
  string value = 2;   // 值（found=true 时有效）
}
```

**语义**：
- 先查本地存储，命中则立即返回
- 未命中则执行 Kademlia **迭代查找**（最多 32 轮，每轮并行查询 α=3 个节点）
- 找到后自动缓存到 K 个最近节点

**性能**：通常 O(log N) 轮，在网络稳定的局域网中约 100–500ms。

---

### 3.3 Delete

从 DHT 删除键值对。

```
rpc Delete(DeleteRequest) returns (DeleteResponse);
```

**请求**：

```protobuf
message DeleteRequest {
  string key = 1;
}
```

**响应**：

```protobuf
message DeleteResponse {
  bool success = 1;  // true = 至少有一个副本删除成功
}
```

**语义**：
- 删除本地副本 + 向 K 个最近节点发送 `DeleteData` RPC
- 由于 DHT 的最终一致性，已缓存的副本不会主动删除（会随时间过期）

---

### 3.4 GetPeers

**这是 BitTorrent Peer 发现的核心 RPC。** 查找拥有指定 `info_hash` 的 Peer 列表。

```
rpc GetPeers(GetPeersRequest) returns (GetPeersResponse);
```

**请求**：

```protobuf
message GetPeersRequest {
  bytes info_hash = 1;  // 必须是 20 字节的 SHA1 info_hash (raw binary)
}
```

**响应**：

```protobuf
message GetPeersResponse {
  repeated Peer peers = 1;  // Peer 列表
  bool          found = 2;  // 是否找到 Peer
}

message Peer {
  string ip   = 1;  // "x.x.x.x" (IPv4) 或 "x:x:...:x" (IPv6)
  uint32 port = 2;  // 1–65535
}
```

**语义**：
- 先查本地 `peerStore`，命中则返回已知 Peer
- 未命中则执行 Kademlia 迭代查找，聚合多个来源的 Peer（去重）
- `found = false` 表示 DHT 网络中尚无任何节点宣告过此 `info_hash`

**警告**：
- `info_hash` **必须正好是 20 字节** raw binary，不要传 hex 字符串
- 返回的 Peer 列表**不保证全部在线** — 你需要自行尝试 TCP 连接筛选

---

### 3.5 AnnouncePeer

宣告你的客户端正在下载/做种某个 torrent。

```
rpc AnnouncePeer(AnnouncePeerRequest) returns (AnnouncePeerResponse);
```

**请求**：

```protobuf
message AnnouncePeerRequest {
  bytes info_hash = 1;  // 20 字节 SHA1 info_hash
  Peer  peer      = 2;  // 你的 Peer 信息（IP + 监听端口）
}
```

**响应**：

```protobuf
message AnnouncePeerResponse {
  bool success = 1;  // true = 至少一个节点接受了宣告
}
```

**语义**：
- DHT 内部自动完成 **两阶段 token 交换**（你无需管理 token）：
  1. 向 K 个最近节点发起 `GetPeers` 获取各节点的 HMAC token
  2. 用各自的 token 向对应节点发起 `AnnouncePeer`
- 先本地存储，再远程宣告
- 重复宣告同一 Peer 是**幂等**的（去重）

**重要**：你应该在以下时机调用：
- 下载开始后（宣告为 leecher，帮助他人找到你）
- 下载完成后（宣告为 seeder）
- 定期重新宣告（建议每 30 分钟），因为 token 有时效性（10 分钟窗口）

---

## 4. Rust 客户端集成

### 4.1 项目设置

**目录结构**：

```
bittorrent-rs/
├── Cargo.toml
├── build.rs                  # tonic-build 编译 proto
├── proto/
│   └── dht.proto             # 从 Go 项目复制或 symlink
└── src/
    ├── main.rs
    └── dht/
        ├── mod.rs
        └── client.rs         # DHT gRPC 客户端封装
```

### 4.2 Cargo.toml

```toml
[package]
name = "bittorrent-rs"
version = "0.1.0"
edition = "2021"

[dependencies]
tonic = "0.12"
prost = "0.13"
tokio = { version = "1", features = ["full"] }
sha1 = "0.10"
hex = "0.4"

[build-dependencies]
tonic-build = "0.12"
```

### 4.3 build.rs — 编译时生成 gRPC 代码

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::compile_protos("proto/dht.proto")?;
    Ok(())
}
```

生成后的 Rust 模块路径为 `dht::DhtServiceClient`，消息类型为 `dht::PutRequest` 等。

### 4.4 DHT 客户端封装 (`src/dht/client.rs`)

```rust
use tonic::transport::Channel;
use dht::dht_service_client::DhtServiceClient;
use dht::{PutRequest, GetRequest, DeleteRequest,
          GetPeersRequest, AnnouncePeerRequest, Peer};

pub struct DhtClient {
    inner: DhtServiceClient<Channel>,
}

impl DhtClient {
    /// 连接到本地 DHT Sidecar 的 gRPC 端口。
    /// `grpc_addr` 格式: "http://127.0.0.1:50051"
    pub async fn connect(grpc_addr: &str) -> Result<Self, tonic::transport::Error> {
        let inner = DhtServiceClient::connect(grpc_addr.to_string()).await?;
        Ok(Self { inner })
    }

    /// 存储键值对。
    pub async fn put(&mut self, key: &str, value: &str) -> Result<bool, tonic::Status> {
        let req = PutRequest {
            key: key.to_string(),
            value: value.to_string(),
        };
        let resp = self.inner.put(req).await?;
        Ok(resp.into_inner().success)
    }

    /// 检索键值对。
    pub async fn get(&mut self, key: &str) -> Result<(bool, String), tonic::Status> {
        let req = GetRequest { key: key.to_string() };
        let resp = self.inner.get(req).await?;
        let r = resp.into_inner();
        Ok((r.found, r.value))
    }

    /// 删除键值对。
    pub async fn delete(&mut self, key: &str) -> Result<bool, tonic::Status> {
        let req = DeleteRequest { key: key.to_string() };
        let resp = self.inner.delete(req).await?;
        Ok(resp.into_inner().success)
    }

    /// 查找 Peer 列表。
    /// `info_hash`: 20 字节 SHA1 raw bytes（不是 hex 字符串！）
    pub async fn get_peers(
        &mut self,
        info_hash: &[u8; 20],
    ) -> Result<(bool, Vec<Peer>), tonic::Status> {
        let req = GetPeersRequest {
            info_hash: info_hash.to_vec(),
        };
        let resp = self.inner.get_peers(req).await?;
        let r = resp.into_inner();
        Ok((r.found, r.peers))
    }

    /// 宣告自己为 Peer。
    pub async fn announce_peer(
        &mut self,
        info_hash: &[u8; 20],
        ip: &str,
        port: u16,
    ) -> Result<bool, tonic::Status> {
        let req = AnnouncePeerRequest {
            info_hash: info_hash.to_vec(),
            peer: Some(Peer {
                ip: ip.to_string(),
                port: port as u32,
            }),
        };
        let resp = self.inner.announce_peer(req).await?;
        Ok(resp.into_inner().success)
    }
}
```

### 4.5 使用示例

```rust
use sha1::{Sha1, Digest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut dht = DhtClient::connect("http://127.0.0.1:50051").await?;

    // ── K/V 操作 ──
    dht.put("config:max_upload_slots", "8").await?;
    let (found, value) = dht.get("config:max_upload_slots").await?;
    assert!(found);
    assert_eq!(value, "8");

    // ── Peer 发现 ──
    // 计算 info_hash (20 bytes SHA1)
    let info_hash: [u8; 20] = Sha1::digest(b"test-torrent-content").into();

    // 宣告自己
    dht.announce_peer(&info_hash, "203.0.113.45", 6881).await?;

    // 查找其他 Peer
    let (found, peers) = dht.get_peers(&info_hash).await?;
    if found {
        for peer in &peers {
            println!("Found peer: {}:{}", peer.ip, peer.port);
            // 用 BitTorrent wire protocol 连接这个 peer...
        }
    }

    Ok(())
}
```

---

## 5. BitTorrent 集成模式

### 5.1 完整的下载流程

```
┌─────────────────────────────────────────────────────────┐
│ 1. 解析 .torrent / Magnet Link → info_hash (20 bytes)  │
└──────────────────────┬──────────────────────────────────┘
                       ▼
┌─────────────────────────────────────────────────────────┐
│ 2. AnnouncePeer(info_hash, my_ip, my_port)              │
│    宣告自己为 leecher，加入 DHT swarm                    │
└──────────────────────┬──────────────────────────────────┘
                       ▼
┌─────────────────────────────────────────────────────────┐
│ 3. 循环: GetPeers(info_hash) → 获取 Peer 列表           │
│    • 过滤在线 Peer（TCP handshake 测试）                 │
│    • 维持 30–50 个活跃连接                              │
│    • 每 5 分钟重新 GetPeers 发现新 Peer                  │
└──────────────────────┬──────────────────────────────────┘
                       ▼
┌─────────────────────────────────────────────────────────┐
│ 4. BitTorrent Wire Protocol 下载                        │
│    • Handshake → Bitfield → Interested → Unchoke        │
│    • Request / Piece 分片传输                           │
│    • SHA1 校验每个 piece                                │
└──────────────────────┬──────────────────────────────────┘
                       ▼
┌─────────────────────────────────────────────────────────┐
│ 5. 下载完成 → AnnouncePeer(info_hash, my_ip, my_port)   │
│    宣告自己为 seeder                                     │
│    每 30 分钟重新宣告                                    │
└─────────────────────────────────────────────────────────┘
```

### 5.2 AnnouncePeer 的调用时机

| 事件 | 操作 |
|---|---|
| 开始下载 | `AnnouncePeer` — 帮助其他 leecher 找到你 |
| 下载完成 | `AnnouncePeer` — 转为 seeder |
| 每 30 分钟 | `AnnouncePeer` — 刷新 DHT 宣告（token 过期） |
| 优雅退出 | `Delete(info_hash)` — 可选，从 K/V 移除元数据 |

### 5.3 并发与重试

```rust
use tokio::time::{sleep, Duration};

/// 健壮的 GetPeers，带重试和退避。
async fn discover_peers_robust(
    client: &mut DhtClient,
    info_hash: &[u8; 20],
    max_retries: u32,
) -> Vec<Peer> {
    for attempt in 0..max_retries {
        match client.get_peers(info_hash).await {
            Ok((true, peers)) if !peers.is_empty() => return peers,
            Ok((false, _)) => {
                eprintln!("No peers found yet (attempt {})", attempt + 1);
            }
            Err(e) => {
                eprintln!("GetPeers error (attempt {}): {}", attempt + 1, e);
            }
        }
        sleep(Duration::from_secs(2u64.pow(attempt.min(5)))).await;
    }
    vec![]
}
```

---

## 6. 数据约定

### 6.1 info_hash

| 项目 | 规范 |
|---|---|
| **来源** | `.torrent` 文件中 `info` 字典的 SHA1 哈希；或 Magnet Link `urn:btih:` 后的 hex 字符串 |
| **长度** | **恰好 20 字节** |
| **gRPC 传输** | `bytes` 类型（raw binary），**不要**用 hex 字符串 |
| **生成方式 (Rust)** | `let hash: [u8; 20] = Sha1::digest(&bencoded_info).into();` |
| **从 Magnet Link 提取** | 解析 `magnet:?xt=urn:btih:<40-hex-chars>`，将 40 hex 字符解码为 20 bytes |

```rust
/// 从 hex 字符串解码 info_hash（用于解析 Magnet Link）
fn info_hash_from_hex(hex: &str) -> Result<[u8; 20], hex::FromHexError> {
    let bytes = hex::decode(hex)?;
    let mut hash = [0u8; 20];
    hash.copy_from_slice(&bytes);
    Ok(hash)
}

/// 将 info_hash 编码为 hex 字符串（用于日志/显示）
fn info_hash_to_hex(hash: &[u8; 20]) -> String {
    hex::encode(hash)
}
```

### 6.2 Peer 编码

gRPC 中使用结构化 `Peer` 消息（`ip` + `port`），无需 compact 编码。

如果你需要与 Mainline DHT 的 compact peer 格式互操作：

| 格式 | 编码 |
|---|---|
| **Compact IPv4** | 6 bytes = 4 bytes IP (big-endian) + 2 bytes port (big-endian) |
| **Compact IPv6** | 18 bytes = 16 bytes IP + 2 bytes port (big-endian) |

```rust
/// Compact IPv4 peer list → Vec<Peer>
fn decode_compact_peers(data: &[u8]) -> Vec<Peer> {
    data.chunks_exact(6)
        .map(|chunk| Peer {
            ip: format!("{}.{}.{}.{}", chunk[0], chunk[1], chunk[2], chunk[3]),
            port: u16::from_be_bytes([chunk[4], chunk[5]]) as u32,
        })
        .collect()
}
```

### 6.3 K/V Key 命名建议

DHT 的 `Put`/`Get` 使用 flat namespace。为避免冲突，建议用前缀：

| 前缀 | 用途 |
|---|---|
| `torrent:<hex-hash>:meta` | torrent 元数据（bencoded info dict） |
| `torrent:<hex-hash>:trackers` | tracker 列表 |
| `torrent:<hex-hash>:comments` | 用户评论 |
| `config:<key>` | 节点配置 |

---

## 7. 错误处理

### 7.1 gRPC 状态码

| 状态码 | 含义 | 处理建议 |
|---|---|---|
| `OK` | 成功 | — |
| `Unavailable` | DHT Sidecar 未运行或无法连接 | 重试连接，检查进程是否存活 |
| `DeadlineExceeded` | 操作超时 | 检查 DHT 网络规模、增加超时时间 |
| `InvalidArgument` | 参数错误（如 `info_hash` 长度 ≠ 20） | 修复调用代码 |
| `Internal` | DHT 内部错误 | 查看 Sidecar 日志 |

### 7.2 幂等性与重试

- **Put**：可安全重试（重复 Put 覆盖旧值）
- **Get**：可安全重试
- **Delete**：可安全重试（删除不存在的 key 返回 success=true）
- **AnnouncePeer**：可安全重试（去重机制）
- **GetPeers**：可安全重试

### 7.3 超时建议

```rust
use tokio::time::timeout;

let result = timeout(
    Duration::from_secs(30),  // 30 秒超时
    client.get_peers(&info_hash)
).await;
```

| 操作 | 推荐超时 |
|---|---|
| Put | 10s |
| Get | 30s（需迭代查找） |
| Delete | 10s |
| GetPeers | 30s（需迭代查找） |
| AnnouncePeer | 20s（两阶段） |

---

## 8. 调试与故障排除

### 8.1 验证 DHT Sidecar 状态

```bash
# 检查进程是否运行
ps aux | grep dht-sidecar

# 列出可用 gRPC 服务
grpcurl -plaintext localhost:50051 list

# 列出所有方法
grpcurl -plaintext localhost:50051 list dht.DhtService

# 查看方法签名
grpcurl -plaintext localhost:50051 describe dht.DhtService.Put
```

### 8.2 常见问题

| 问题 | 可能原因 | 解决方案 |
|---|---|---|
| `GetPeers` 始终返回 `found=false` | DHT 网络中尚无节点宣告过此 info_hash | 先调用 `AnnouncePeer`；确保有其他节点在线 |
| gRPC 连接被拒绝 | Sidecar 未启动或 `--grpc-port` 未设置 | 检查启动参数，确认 gRPC 端口 > 0 |
| `info_hash must be 20 bytes` 错误 | 传入了 hex 字符串（40 字节）而非 raw bytes | 用 `hex::decode()` 将 hex → 20 bytes |
| Peer 列表返回的 IP 无法连接 | Peer 已下线或 NAT 问题 | 这是正常的 — 应尝试 TCP 连接筛选在线 Peer |
| 端口冲突 `address already in use` | 之前进程未正常退出 | `pkill dht-sidecar` 后重试 |

### 8.3 日志

DHT Sidecar 使用 [logrus](https://github.com/sirupsen/logrus) 输出结构化日志到 `dht-test.log`。关键日志行格式：

```
[127.0.0.1:20000] Putting key: mykey, value: myvalue
[127.0.0.1:20000] GetPeers for info_hash 1234567890abcdef
[127.0.0.1:20000] AnnouncePeerRPC: registered peer 10.0.0.1:6881
```

---

## 附录 A：完整的 proto 文件

见项目根目录 `proto/dht.proto`。可直接复制到上层项目的 `proto/` 目录。

```protobuf
syntax = "proto3";
package dht;
option go_package = "dht/proto";

service DhtService {
  rpc Put(PutRequest) returns (PutResponse);
  rpc Get(GetRequest) returns (GetResponse);
  rpc Delete(DeleteRequest) returns (DeleteResponse);
  rpc GetPeers(GetPeersRequest) returns (GetPeersResponse);
  rpc AnnouncePeer(AnnouncePeerRequest) returns (AnnouncePeerResponse);
}

message PutRequest    { string key = 1; string value = 2; }
message PutResponse   { bool success = 1; }
message GetRequest    { string key = 1; }
message GetResponse   { bool found = 1; string value = 2; }
message DeleteRequest { string key = 1; }
message DeleteResponse { bool success = 1; }

message Peer {
  string ip   = 1;
  uint32 port = 2;
}

message GetPeersRequest  { bytes info_hash = 1; }
message GetPeersResponse {
  repeated Peer peers = 1;
  bool          found = 2;
}

message AnnouncePeerRequest {
  bytes info_hash = 1;
  Peer  peer      = 2;
}
message AnnouncePeerResponse { bool success = 1; }
```

## 附录 B：最小可工作的 Rust 示例

```rust
// examples/hello_dht.rs
use tonic::transport::Channel;
use sha1::{Sha1, Digest};

mod dht {
    tonic::include_proto!("dht");
}
use dht::dht_service_client::DhtServiceClient;
use dht::{PutRequest, GetRequest, GetPeersRequest, AnnouncePeerRequest, Peer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = DhtServiceClient::connect("http://127.0.0.1:50051").await?;

    // Put
    let resp = client.put(PutRequest {
        key: "hello".into(), value: "world".into(),
    }).await?;
    println!("Put success: {}", resp.into_inner().success);

    // Get
    let resp = client.get(GetRequest { key: "hello".into() }).await?;
    let r = resp.into_inner();
    println!("Get found={}, value={}", r.found, r.value);

    // Peer discovery
    let info_hash: [u8; 20] = Sha1::digest(b"example-torrent").into();
    client.announce_peer(AnnouncePeerRequest {
        info_hash: info_hash.to_vec(),
        peer: Some(Peer { ip: "10.0.0.1".into(), port: 6881 }),
    }).await?;

    let resp = client.get_peers(GetPeersRequest {
        info_hash: info_hash.to_vec(),
    }).await?;
    for peer in &resp.into_inner().peers {
        println!("Peer: {}:{}", peer.ip, peer.port);
    }

    Ok(())
}
```
