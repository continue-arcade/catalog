# continue-cart

`continue-cart` 是 Cart Toolchain MVP 的 library + CLI crate。

它导出两层能力：

- Library API：供 Forge、未来 Rails Depot、测试和其他工具复用。
- `continue-cart` CLI：作为 library 的薄壳，方便本地开发和 CI 调用。

当前 CLI：

```bash
cargo run -p continue-cart --bin continue-cart -- inspect <cart-dir-or-cart-file>
cargo run -p continue-cart --bin continue-cart -- pack <cart-dir> --out <file.cart>
cargo run -p continue-cart --bin continue-cart -- verify <file.cart>
cargo run -p continue-cart --bin continue-cart -- unpack <file.cart> --out <dir>
cargo run -p continue-cart --bin continue-cart -- index <cart-dir-or-cart-file>... --out <index.json>
cargo run -p continue-cart --bin continue-cart -- resolve <index.json>... --type core --id org.continue.resource.core.mgba --version '>=0.10.3'
```

v0.1 已覆盖基础 manifest 校验、include 边界、安全路径、ZIP `.cart` 打包/解包、整包 SHA-256、inspect、静态 Cart Market Index 生成和本地资源依赖解析提示。

真实签名算法、证书链、远端 resolver 和 Rails Depot 不在当前 crate 的首个切片内。
