# Continue Catalog

由 Yinks 管理后台发起的 Continue Arcade 系统字典发布仓库。

现行预发布：[v0.2.0-preview.3](https://github.com/continue-arcade/catalog/releases/tag/v0.2.0-preview.3)。此前试发布已撤回为草稿，当前数据直接采用 Yinks 身份。

## 发布流程

1. Yinks 从 PostgreSQL 的同一份一致性快照选择类型与平台，记录输入摘要和版本。
2. 服务端调用 `continue-cart` 打包并校验，将候选原件保存到本地不可覆盖的制品目录。
3. 操作员在「数据发布」中提交候选。后端通过已配置的 SSH 身份，原子推送固定输入和 `v<version>` tag。
4. GitHub Actions 使用本仓库固定的 Rust 工具链重新构建，并与本地制品 SHA-256、大小逐一比较。
5. 先创建 Draft Release，收齐 `.cart`、校验和及发布输入说明，校验已上传 bytes 后公开。失败可以重跑，不覆盖已存在的不同内容。

`snapshots/<version>/source/` 是公开的固定输入；Git 不保存 `.cart`、数据库备份、来源原文、账号、凭据或构建缓存。

## 当前交付范围

- Cart：`org.continue.system.catalog`。
- Resource：`org.continue.resource.system.catalog`。
- Schema：`continue.system.catalog.v1`。
- 类型取分类入口与状态为 `direct` 的类型词条；候选和待审词条不发布为正式分类。
- 全部已登记平台默认有离线字典资格，具体发布可以显式选择平台 key；网站的 `live / coming_soon / hidden` 不决定离线资格。
- 保留多语言名称、正式分类引用以及平台结构化字段；不包含游戏、ROM、指纹、图片、来源原文或审核记录。
- 厂商和系列本次为空。此前开发包可以废弃，新发布以 Yinks 当前定义建立唯一现行基线，不保留旧 ID 映射、别名或迁移层。
- 类型为 `genre.<key>`，玩法与模式为 `tag.<key>`，其他正式词条为 `term.<key>`，导航为 `navigation.<key>`；MMORPG 类型与 MMO 模式分别使用 `genre.mmorpg` 和 `tag.mmo`。
- 当前仅允许预发布版本，不自动更新应用内置包。

当前包未签名。GitHub Release、SHA-256 和展示用 publisher 不授予官方信任，Continue Arcade 的系统 Catalog 仍需专门可信安装流程。不能把本预发布当作现有完整内置字典的自动替换。

## 本地复现

```sh
cd toolchain
mise exec -- cargo build --locked -p continue-cart --bin continue-cart
cd ..
python3 scripts/release.py build 0.2.0-preview.3 --out dist/0.2.0-preview.3
```

设置 `CONTINUE_CART_BIN` 可以使用已构建的同一份工具链。固定版本的输入、制品和标签不可覆盖；数据或打包逻辑变化使用新版本。

发布步骤失败后，可在 Actions 手动运行 `Catalog release`，选择 `main` 并填写原版本。修复后的脚本会逐文件核对原 tag 的输入，不能通过重试改写快照，也不需要移动 tag。草稿按 Release ID 查询和校验，避免 GitHub 的 tag 查询遗漏尚未公开的草稿。

## 工具链来源与许可

`toolchain/` 是 Continue 主仓库中 `continue-cart` 与纯契约依赖 `continue-host-api` 的精确源码快照。`toolchain/source.json` 记录来源提交和逐文件 SHA-256，Cargo.lock 固定依赖。只复制构建所需的工具链，CI 不依赖私有业务仓库访问权，也不需要长期 GitHub Token。

工具链源码保留原有 `UNLICENSED` 声明。`LicenseRef-Continue-Catalog` 仅标识本产品的数据资源，不额外授予第三方数据源的许可；发布数据遵循其实际来源授权边界。
