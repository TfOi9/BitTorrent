# BitTorrent

## 项目概述

基于 Kademlia DHT 的 BitTorrent 实现，是课程设计项目，DHT 采用 Go 语言实现，后端主体采用 Rust 语言实现，二者通过 gRPC 实现通信，相对分离。

## 项目结构

```
.
├── AGENTS.md
├── backend                     # 后端部分
│   ├── Cargo.lock
│   ├── Cargo.toml
│   ├── build.rs
│   ├── dht-test.log
│   ├── proto                   # Proto 文件，请勿修改
│   │   └── dht.proto
│   ├── src                     # 后端源代码
│   │   ├── core                # 类型定义与解析器
│   │   ├── dht                 # 与 DHT 的对接部分
│   │   ├── generated           # 自动生成代码，请勿修改
│   │   ├── lib.rs
│   │   ├── main.rs
│   │   ├── peer                # Peer 部分
│   │   ├── session.rs
│   │   ├── storage             # 存储部分
│   │   └── tracker             # Tracker 部分
│   └── tests                   # 测试点
├── dht                         # DHT 实现，请勿修改
│   ├── bridge
│   ├── dht-sidecar
│   ├── go.mod
│   ├── go.sum
│   ├── kademlia                # Kademlia 协议
│   ├── main.go
│   ├── node                    # DHT 结点
│   ├── proto
│   ├── repro_test.go
│   └── testutil
└── impl_docs                   # 实现参考文档
```

## 规则
- 不允许修改项目结构或引入其他第三方库（如确实需要，请提出请求）
- 不允许修改 DHT 实现（已经经过验证）
- 不允许修改 Proto 文件等自动生成的代码
- `git` 只能读，不能做任何 `commit` `checkout` `push` 等修改操作
- 已有部分的详细说明文档在 `impl_docs` 下
- 新增代码应该同样在 `impl_docs` 下写入说明文档
- 写完代码后需要保证编译通过
- 写完某模块后应当编写单元测试并运行，要求测试鲁棒且有一定规模
- 单元测试应该写成源代码，放在 `tests` 下，不要把测试内联在代码内
- 代码风格应遵循 `rustfmt`
- 注释应当非常简洁，仅能使用英文和 ASCII 字符，不写整行分隔线
