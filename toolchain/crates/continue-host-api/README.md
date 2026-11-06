# continue-host-api

`continue-host-api` 是 Continue Host API 的纯 contract crate。它定义跨 transport、跨平台的
请求、响应、调用上下文、权限和公开错误；它不执行 I/O，也不依赖 `PathBuf`、
`std::io::Error`、SQLite、HTTP client 或桌面平台库。

## 责任边界

```text
Extension / Agent
      │ depends on
      ▼
continue-host-api      FileRef, ApiContext, Capability, ApiError, P0 DTOs
      ▲
      │ implemented by
continue-host          registry, authorization, services, platform backends
```

Contract 不等于 transport。Host 方法可以经由未来 JSON-RPC、WASM bridge、QuickJS、
in-process Rust call 或 Agent Tool 调用；每个 transport 都必须保留相同的语义、权限和
错误 shape。

## 核心类型

### FileRef 与路径安全

所有文件操作使用：

```rust
use continue_host_api::FileRef;

let cover = FileRef::new(
    "extension-cache",
    "igdb/covers/co1234.jpg",
)?;
```

`mount` 是 Host 授权边界，`path` 是 Unix 风格 `/` 分隔的相对路径。`FileRef::new`、
`FileRef::validate` 和 serde 反序列化都会拒绝：

- `..`、`.` 和异常空路径段；
- 绝对路径；
- Windows drive prefix；
- `\`、UNC 路径、NUL 与控制字符。

`NormalizedRelativePath` 把已验证路径保存为段数组，供 backend 按段导航；它在 JSON
中仍序列化为普通 `/` 分隔字符串。空路径表示 mount 根目录。

### ApiContext 与授权

每次调用持有 owned `ApiContext`：

```rust
pub struct ApiContext {
    pub actor: ActorIdentity,
    pub session_id: Option<SessionId>,
    pub request_id: Option<RequestId>,
    pub authorization: Arc<AuthorizationSnapshot>,
}
```

`AuthorizationSnapshot` 分别表达 method capability、mount 读写权限、精确网络域名和
secret 名称；它不表示平台是否支持某项功能。Host 必须在每次 dispatch 使用该快照，不能
依赖全局或可变权限。

### ApiError

跨边界只返回安全的：

```json
{
  "kind": "quota_exceeded",
  "message": "Response size exceeds the configured limit",
  "details": {
    "limit": 8388608,
    "suggestion": "use net.download for large responses"
  }
}
```

`kind` 是稳定、snake_case 的程序化分支字段。`details` 可扩展，但不能包含绝对路径、
token、secret、Authorization header 或内部 source chain。

## P0 方法

| Namespace | 方法 |
| --- | --- |
| Core | `core.hello`、`core.capabilities` |
| System | `system.info` |
| Network | `net.request`、`net.download` |
| Filesystem | `fs.readText`、`fs.writeText`、`fs.stat`、`fs.list`、`fs.mounts` |
| KV | `kv.get`、`kv.set`、`kv.delete` |
| Secret | `secret.get` |
| Crypto | `crypto.hashFile` |
| Log | `log.write` |

`Capability` 以方法字符串序列化，例如 `"fs.readText"`。未知值保留为
`Capability::Custom(String)`，为 future Domain API 提供扩展空间。

## 网络语义

`net.request` 只支持显式 `ResponseMode::Text`、`Json` 或 `Discard`。它不传输内联
二进制；图片、大文件和其他二进制必须通过 `net.download` 流式写入 `FileRef`。请求
header 与 query 参数使用有序 `Vec`，因此允许重复项并保留顺序。

## 验证

```bash
cargo test -p continue-host-api
cargo check -p continue-host-api --all-targets --all-features
cargo clippy -p continue-host-api --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc -p continue-host-api --all-features --no-deps
```
