# IGDB-like JavaScript Fixture

这是 QuickJS JavaScript Extension 和 Scraper Registry 的离线 fixture。它模拟一个
IGDB-like provider，但不会访问 Twitch、IGDB、图片 CDN 或任何真实 credential store。

发布边界由 [`cartfile.toml`](cartfile.toml) 控制，只包含 `continue.toml`、README 和已经
构建好的 [`dist/extension.js`](dist/extension.js)。`cartfile.dev.toml` 只供
Local Development 使用，不会进入 Cart；它的 build command 只是把可读的 source bundle
复制到 `dist/`，Runtime 不编译 TypeScript，也不执行 npm resolution。

`continue.toml` 声明 `continue-js-1`、`host_api = "0.1"`、两个 scraper commands、
`net.request`/`net.download`、KV、secret、log，以及 `extension-cache` mount。所有文件访问
都通过 SDK 的 `FileRef`，Extension 不获得真实路径或 Node filesystem API。

fixture 调用链是：

```text
secret.get
  → net.request (example.test OAuth-like token endpoint)
  → kv.set
  → net.request (example.test search/detail endpoint)
  → net.download (extension-cache FileRef)
```

测试通过 Node loader 把唯一 external import `@continue/sdk` 绑定到 workspace SDK，并注入
一个内存 Host bridge。测试不会启动 HTTP server、发起网络请求或读取真实 secret。

本目录可独立执行：

```bash
npm test
npm run build
```

## 安全设置示例

`igdb-like-client-secret` 使用 secret 类型，沿用离线 OAuth fixture 的 `sdk.secret.get` 调用。
`fixture-password` 使用 password 类型，用于验证宿主密码输入、隔离存储与清除；不连接真实账号。
两种类型都只向宿主界面提供配置状态，不回显明文，也不保存到 JSONC 或 KV。
