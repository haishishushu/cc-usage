# CC Usage

Windows 桌面 AI 用量监控工具。常驻灵动岛 + 完整主面板，监控 Claude / Codex 的
额度、Token 用量与请求日志。

- 设计文件：[`pencil-new.pen`](./prd/pencil-new.pen)（浅色 / 深色画布）
- 需求文档：[`Pencil-UI设计需求.md`](./prd/Pencil-UI设计需求.md)
- 实现清单：[`todo.md`](./todo.md)

## 技术栈

Tauri 2 + Rust + React 19 + TypeScript + Vite + Tailwind CSS 4 + shadcn/ui + SQLite

```
frontend/   React 前端（主面板、灵动岛、设置与浏览器设计预览）
backend/    Tauri + Rust（窗口、托盘、采集、SQLite 与只读查询）
```

`frontend/package.json` 和 `frontend/pnpm-lock.yaml` 管理 React 前端依赖；
根目录的 `package.json` 和 `pnpm-lock.yaml` 管理 Tauri CLI 及统一启动入口，Rust 依赖由 `backend/Cargo.toml` 和 `backend/Cargo.lock` 管理。

## 启动

### 只看界面（不需要 Rust）

```bash
pnpm --dir frontend install
pnpm --dir frontend dev      # http://localhost:5173
```

顶部可切换 主面板 / 灵动岛 / 画布对照，以及浅色 / 深色主题。

### 启动桌面端

桌面端需要 Rust 工具链。没装的话先按第 1 步装，已装可直接跳到第 2 步。

**第 1 步 · 安装 Rust**（只需一次，约 5 分钟）

1. 打开 <https://rustup.rs> 下载 `rustup-init.exe` 并运行
2. 一路回车选默认（`1) Proceed with standard installation`）
3. **装完关掉终端重新开一个**，让 PATH 生效
4. 验证：

```bash
rustc --version
cargo --version
```

> WebView2 运行时和 VS 2022 生成工具这台机器已经有了，不用再装。

**第 2 步 · 启动**

```bash
pnpm --dir frontend install  # 仓库根目录，安装前端依赖
pnpm install                 # 仓库根目录，安装 Tauri CLI
pnpm tauri dev
```

在仓库根目录或 `frontend/` 下使用 `pnpm tauri dev`；它会自动先起 Vite，
再编译 Rust 并拉起桌面窗口。**首次编译 Rust 依赖需要 3–10 分钟**，之后增量编译很快。

**打包安装程序**

```bash
pnpm tauri build             # 在仓库根目录运行，产物在 backend/target/release/bundle/nsis/
```

安装向导是中文界面，欢迎页与完成页的侧边大图、右上角标识由
[`scripts/installer-art/`](./scripts/installer-art/) 下的 HTML 源生成，
位图产物已入库，打包时不需要重新生成。改配色或升版本号后重跑：

```bash
pnpm installer-art
```

## 目前的状态

| 部分 | 状态 |
|---|---|
| 前端核心界面 | ✅ 主面板、灵动岛、连接设置与真实异步状态已接通 |
| 双窗口（灵动岛 + 主面板） | ✅ 已配置 |
| 系统托盘菜单、左键打开主面板 | ✅ 已实现 |
| 主面板关闭只隐藏、不退出 | ✅ 已实现 |
| **本地会话记录采集** | ✅ 增量读取 `~/.claude/projects` 与 `~/.codex/sessions`，按 message.id / response_id 去重 |
| **SQLite 存储与统计** | ✅ 今日 / 本周 / 本月 / 累计、按小时或按日趋势、分页请求日志 |
| **编程套餐额度查询** | ✅ cc-switch 全量对齐：智谱 GLM（个人/国际/团队）、Kimi、MiniMax、ZenMux、OpenCode Go、火山方舟、Grok；按连接 base_url 域名自动识别，5h/周/月窗口直连查询 |
| 额度 / 余额的只读网络查询 | ✅ Claude/Codex Auth、官方 API 管理用量与 sub2api 已有独立适配；真实账号联调范围见 `todo.md` |
| 「获取」读取本机凭证 | ✅ 支持 Claude/Codex CLI 本机凭证发现、刷新与验证 |
| 灵动岛拖动吸附 | ✅ 原生拖动、四边吸附、停靠条、恢复与开关已接通 |

尚未完成或仍需真实环境验证的项目统一记录在 [`todo.md`](./todo.md)，不以静态组件存在代替功能完成。

## 常见问题

**`'tauri' 不是内部或外部命令`**
尚未安装根目录中的 Tauri CLI 时会这样。先在仓库根目录跑一次
`pnpm install`。

**`rustc: not installed`**
按上面第 1 步装 Rust，装完**必须重开终端**。

**首次 `pnpm tauri dev` 卡很久**
正常。Rust 在编译几百个依赖，只有第一次慢。

**`EACCES: permission denied` 绑不上端口**
Windows 会保留若干端口段（`netsh interface ipv4 show excludedportrange protocol=tcp` 可查）。
本项目已避开，用 5173。若这台机器把 5173 也保留了，改 `frontend/vite.config.ts` 的 `server.port`
和 `backend/tauri.conf.json` 的 `devUrl`，两处保持一致即可。
