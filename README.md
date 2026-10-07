**官网：[https://haishishushu.github.io/cc-usage-website/](https://haishishushu.github.io/cc-usage-website/)**

<div align="center">

# CC Usage

**让 AI 用量，一眼看得见。**

常驻桌面的灵动岛 + 完整统计主面板，集中查看 AI 编程工具的本机用量与服务商额度。

[![Release](https://img.shields.io/github/v/release/haishishushu/cc-usage?label=release&logo=github)](https://github.com/haishishushu/cc-usage/releases/latest)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-blue)](https://haishishushu.github.io/cc-usage-website/#/download)

[下载安装](https://haishishushu.github.io/cc-usage-website/#/download) · [在线文档](https://haishishushu.github.io/cc-usage-website/#/docs) · [更新日志](https://haishishushu.github.io/cc-usage-website/#/changelog) · [反馈问题](https://github.com/haishishushu/cc-usage/issues)

简体中文 | [English](./README.en.md)

<img src="./docs/assets/readme/island-collapsed.png" alt="CC Usage 桌面灵动岛" width="428" />

</div>

> 文中的界面图片用于说明操作与布局，图中数值不代表你的真实用量。实际数据取决于本机留存记录、连接授权和服务商返回结果。

## 目录

- [核心功能](#核心功能)
- [平台与额度支持](#平台与额度支持)
- [下载与安装](#下载与安装)
- [首次使用](#首次使用)
- [日常操作](#日常操作)
- [读懂统计数据](#读懂统计数据)
- [更新与数据保留](#更新与数据保留)
- [隐私与本地存储](#隐私与本地存储)
- [常见问题](#常见问题)
- [开发与构建](#开发与构建)
- [反馈与许可证](#反馈与许可证)
- [Community 社区](#community-社区)
- [Star History](#star-history)

## 核心功能

| 功能 | 可以做什么 |
| --- | --- |
| 桌面灵动岛 | 查看当前连接的额度、重置倒计时和支持来源的会话增量；双击展开，拖向屏幕边缘停靠 |
| 统计主面板 | 按平台、时间范围和模型查看 Token、缓存、请求数与可估算的费用；经本地代理使用过的 API Key 连接可切换为「仅此连接」查看 |
| 请求日志 | 分页查看记录中的模型、输入输出、费用及可获得的请求状态、耗时 |
| 连接管理 | 读取本机已配置的授权或 API 连接，检测、启用、断开、编辑与移除 |
| 套餐额度 | 对支持的服务地址识别额度服务商，显示其真实返回的窗口 |
| 本地数据管理 | 导入、导出统计记录和会话标题，设置历史保留期限 |
| 常驻与更新 | 托盘入口、窗口位置记忆、单实例启动、应用内检查与安装更新 |

本地统计与在线额度是两条独立链路：**有本机会话不代表能查询账号额度；有账号额度也不代表本机留存了会话记录。**

## 平台与额度支持

### AI 工具平台

| 平台 | 已接入的主要能力 | 使用边界 |
| --- | --- | --- |
| Claude | Claude Code 本机会话、Token 与费用估算、官方订阅及支持网关的额度查询 | 额度需要有效授权；未经本地代理的本机统计按平台汇总，经代理的 API Key 连接可按连接归账 |
| Codex | Codex CLI 本机会话、Token 与费用估算、官方订阅及支持网关的额度查询 | 未经本地代理的本机用量按平台合并；经代理的 API Key 连接可按连接归账，OAuth 不参与 |
| Gemini | Gemini CLI 本机会话与 Token 增量（含思考、工具 Token）、按官方价目的费用估算 | 在线剩余额度未接入；费用为按 API 标价的估算 |
| Grok | 本机 OAuth 对应的 SuperGrok 订阅剩余百分比 | 未接入本机会话统计；API 预付余额需要独立管理授权 |
| Zcode | 本机用量记录、Token 增量、会话完成 / 失败 / 运行中状态、BigModel Coding Plan Key 的套餐查询 | 账号 OAuth 额度未接入 |
| Trae | 本机来源检测入口 | 不支持本机会话统计；个人额度未验证，企业额度需管理员授权 |
| Qoder | 国内版与国际版本机记录、已记录积分 | 在线剩余积分未接入；字段完整度取决于来源记录 |
| Workbuddy | 本机用量、缓存与已上报积分 | 已消耗积分不等于剩余积分；在线个人剩余积分未接入 |

平台目录与能力以应用中的实际提示为准。Windows、macOS、Linux 均有构建目标，但第三方应用的数据目录和系统托盘行为存在差异，不能把某系统安装包可用等同于全部来源均已验证。

### 额度服务商

平台表示“使用哪个工具”，服务商表示“由谁提供额度”。例如，在 Claude 中使用 Kimi 编程套餐，工具平台仍是 Claude。

| 服务商 | 识别与配置 | 可显示内容 |
| --- | --- | --- |
| Claude / Codex 官方订阅 | 读取对应 CLI 的本机登录凭证 | 服务端返回的订阅窗口和重置时间 |
| 智谱 GLM / Z.ai | 按服务地址识别国内或国际版；团队版补充组织与项目 ID | 套餐窗口使用情况 |
| Kimi | 识别 Coding 服务地址 | 5 小时与周窗口 |
| MiniMax | 识别国内或国际服务地址 | 5 小时窗口，以及接口返回的周窗口 |
| ZenMux | 按支持的服务地址查询 | 额度窗口与接口提供的金额信息 |
| OpenCode Go | 识别 Go 服务地址 | 滚动、周、月窗口 |
| 火山方舟 | 识别套餐地址；补充查询用 AccessKey ID 与 Secret AccessKey | 会话、周、月窗口 |
| Grok | 使用对应的本机 OAuth 授权 | SuperGrok 订阅剩余百分比 |

具体窗口以账户套餐和接口返回为准。未知、不支持或查询失败会显示相应状态，不应理解为“剩余为零”。

## 下载与安装

从[官网下载页](https://haishishushu.github.io/cc-usage-website/#/download)选择系统，也可以在 [GitHub Releases](https://github.com/haishishushu/cc-usage/releases) 选择指定版本。

| 系统 | 应选择的文件 | 安装方式 |
| --- | --- | --- |
| Windows 10 / 11 x64 | `Windows-x86_64-Setup.exe` | 运行安装向导，完成后启动 CC Usage |
| macOS Apple Silicon | `macOS-arm64.dmg` | 打开磁盘映像，将应用拖入“应用程序” |
| macOS Intel | `macOS-x86_64.dmg` | 打开磁盘映像，将应用拖入“应用程序” |
| Linux x64 | `Linux-x86_64.AppImage` | 在文件属性中允许作为程序执行，再启动 |
| Debian / Ubuntu 系 Linux x64 | `Linux-x86_64.deb` | 使用系统软件安装器安装 |

- macOS 芯片类型可在“关于本机”中确认。
- `.sig` 是更新签名，`latest.json` 是更新清单，都不是手动安装包。
- 每个版本拥有独立发行页，历史版本应从对应版本页下载。
- 如系统提示无法运行，先核对系统架构、下载来源和发行说明；Linux 还需满足相应桌面运行依赖。

## 首次使用

### 1. 启动应用

首次安装后，主面板自动居中打开；灵动岛先在屏幕上方居中展示约 5 秒，然后在允许吸附且未被主动展开等情况下贴向上边缘。

后续启动会恢复已保存的显示偏好和窗口位置。多次点击快捷方式会唤起已有实例，不会重复运行多个程序。

### 2. 准备数据来源

先在需要监控的 AI 工具中完成登录或配置，并实际产生一次会话。CC Usage 读取本机已有记录，不会凭空生成使用数据。

- **官方订阅**：先在对应 CLI 中登录，然后读取本机授权。
- **本机 API 配置**：先在 CC Switch 中配置并应用到对应平台，再由 CC Usage 读取 Key 和服务地址。
- **本机来源监控**：先运行对应应用并产生记录，再检测来源；该操作不会切换外部应用账号。
- **手动 API 连接**：仅在所选平台提供该入口时填写服务地址与 Key；按对话框说明确认支持范围。

### 3. 添加并检测连接

1. 打开主面板，进入 **设置 → 连接管理**。
2. 选择目标平台，点击添加连接，选择界面提供的连接类型。
3. 按提示读取本机配置或填写允许手动输入的信息。
4. 添加完成后，点击连接行中的 **检测**，查看可用状态及错误说明。
5. 对需要额外查询凭证的套餐，按对应设置项补充信息后重新检测。

![连接管理：选择平台、添加连接并检测](./docs/assets/readme/panel-settings.png)

检测成功说明当前连接或来源可访问；是否能显示 Token、额度或余额，仍取决于上方支持表。

### 4. 选择灵动岛连接

展开灵动岛，在连接切换入口选择需要展示的连接。该选择会保存到本地，软件重启或正常覆盖更新后继续读取。

如果原连接已被删除、断开或凭证失效，需要恢复原连接或重新选择；应用不会悄悄替换成同平台的其他账号。

### 5. 确认统计生效

在对应 AI 工具中再完成一次请求，回到主面板选择相同平台，查看今日统计与请求日志。若没有数据，先检查时间筛选、来源路径及记录是否已写入，再看[常见问题](#常见问题)。

## 日常操作

### 主面板：从总量到单条请求

1. 在顶部选择平台。
2. 选择今日、本周、本月、累计或自定义时间，并按需筛选模型。
3. 查看 Token 与缓存拆分，再沿趋势图定位使用高峰。
4. 在请求日志中查看具体记录，使用分页与跳转继续浏览。
5. 选中经本地代理使用过的 API Key 连接后，可把本机统计范围切换为「仅此连接」。

![Token 与缓存统计](./docs/assets/readme/panel-stats.png)

![趋势图与请求日志](./docs/assets/readme/panel-logs.png)

请求耗时、首字时间、状态码等仅在数据来源提供时展示；缺失字段不代表请求失败。

### 灵动岛：随时查看当前连接

| 收缩态 | 展开态 |
| --- | --- |
| ![灵动岛收缩态](./docs/assets/readme/island-collapsed.png) | ![灵动岛展开态](./docs/assets/readme/island-expanded.png) |
| 快速查看关键额度与活动信息 | 查看连接、会话明细及支持的本机增量 |

- **双击**展开或收起。
- **拖向屏幕边缘**可吸附停靠；是否启用吸附由设置控制。
- 在设置中调整透明度、大小、停靠条大小、刷新间隔及置顶偏好。
- 免打扰会减少提示与动效，不停止后台采集。
- 隐藏后可通过主面板或托盘菜单重新显示。
- **右键**灵动岛打开菜单：切换连接、显示位置、始终置顶，以及**开启分身 / 销毁分身**。每个分身可各自选择连接、独立停靠与定位，重启后原样恢复；至少保留一个灵动岛，菜单右上角的数字是当前灵动岛总数。

### 托盘与窗口

![托盘菜单](./docs/assets/readme/tray-menu.png)

- 左键单击托盘图标打开主面板；右键打开操作菜单。
- 关闭主面板后，后台采集与灵动岛继续运行；完全退出请使用退出入口。
- 再次打开主面板会恢复保存的位置和尺寸。更换显示器后如窗口不可见，可使用重置位置功能。

## 读懂统计数据

| 指标 | 含义与注意事项 |
| --- | --- |
| 本地 Token | 来自本机留存的会话记录，按平台聚合，不能视为单个账号或 Key 的独立账单 |
| 新增输入 | 排除可识别的缓存重读部分，用来观察新输入的消耗 |
| 缓存读取 / 创建 | 分别反映复用已有缓存和创建缓存；不同来源提供的字段不同 |
| 灵动岛增量 | 反映支持来源的新鲜 Token 增量，与含缓存重读的总量口径不同 |
| 订阅额度 | 服务商返回的账号或套餐窗口，可能涵盖其他设备上的使用 |
| 估算费用 | 按可识别模型和价目估算；不是最终账单，不等同于订阅套餐扣款 |
| 积分 | 来源已上报的消耗积分；除非明确标注，否则不是账户剩余积分 |
| `—` / 不支持 / 查询失败 | 分别按界面状态理解；不能统一当作数值零 |

因此，本地 Token、灵动岛增量和官方额度无需完全一致。跨设备使用、历史记录缺失、缓存统计方式和服务商刷新延迟都可能造成差异。

## 更新与数据保留

1. 在应用内检查更新，发现新版本后下载更新包。
2. 下载及校验完成后，点击安装，按提示完成更新与重启。
3. 再次打开后检查灵动岛连接和主面板数据。

设置与统计保存在应用数据目录，正常覆盖更新会继续使用原目录。最近选择的灵动岛连接、显示偏好和主面板位置会持久化；删除用户数据、换系统账号或卸载时清理数据不属于覆盖更新。

**统计导出与完整备份有区别：**

- 设置中的 **导入 / 导出** 用于统计记录和会话标题，导入会去重；不包含连接与凭证。
- 如需备份完整本机状态，先通过设置打开数据目录，再从托盘完全退出应用，复制整个目录。
- 完整目录可能包含凭证，只应保存在可信位置。跨设备恢复后仍可能需要在原工具中重新登录。

发行方式和版本递增规则见[版本号规则](./docs/release-versioning.md)。每版安装包归属对应的独立 GitHub Release。

## 隐私与本地存储

```text
本机会话文件 ──增量读取、去重──→ 本地 SQLite ──→ 主面板 / 灵动岛
服务商接口   ──使用当前连接查询额度──────────→ 额度窗口
更新服务     ──检查版本、下载更新包──────────→ 应用更新
```

- 统计记录保存在本机 SQLite 数据库 `usage.db`，运行偏好保存在 `settings.json`。
- 在设置的数据区域使用打开目录入口查看实际保存位置。Windows 默认应用目录通常为 `%APPDATA%\dev.ningz.cc-usage`。
- 额度检测、凭证刷新、模型查询等会访问对应服务，并按接口要求发送授权信息；不能将“本地存储”理解为“完全不联网”。
- 更新检查会访问配置的 GitHub Pages / GitHub 更新地址。
- 默认的本机会话采集读取原始记录；如主动启用本地代理，模型请求会经过本地代理转发至配置的上游，相关 CLI 配置也可能改变。
- 手动 API Key 保存在本机，界面列表以脱敏形式展示；不要公开完整数据目录或凭证文件。

## 常见问题

<details>
<summary>安装后没有统计数据怎么办？</summary>

确认目标 AI 工具已经产生本机会话，选择正确平台与时间范围，并检查来源是否可读。只有在线账号但没有本机记录时，不会自动补出跨设备历史。Trae 等未接入本机会话的平台也不会显示完整 Token 统计。

</details>

<details>
<summary>为什么有 Token，却没有订阅额度？</summary>

本机统计与在线额度独立。检查该平台是否支持额度查询、授权是否有效、服务地址能否识别，以及是否需要套餐辅助凭证。API Key 可用不代表拥有订阅额度查询权限。

</details>

<details>
<summary>为什么切换账号后本地 Token 没有跟着变？</summary>

本地统计按平台汇总，同平台连接共享这些记录，目前不能区分不同账号或 Key 的独立用量。连接的在线额度按其授权查询。

</details>

<details>
<summary>灵动岛或主面板找不到了怎么办？</summary>

从系统托盘打开主面板，确认已启用灵动岛。若曾更换显示器或分辨率，使用菜单中的重置窗口位置功能。Windows 托盘图标也可能被收纳到隐藏图标区域。

</details>

<details>
<summary>更新后原连接不可用怎么办？</summary>

先等待初始化完成，再检测原连接。已保存的选择不等于凭证永久有效；登录过期时应先在原 CLI 或应用中重新登录。若数据目录被清理或更换了系统用户，需要重新接入或从备份恢复。

</details>

<details>
<summary>检查或下载更新失败怎么办？</summary>

检查 GitHub 与更新地址是否可访问、网络代理是否正常。也可以从对应版本的 GitHub Release 下载适合系统的安装包进行覆盖安装，并保留应用数据目录。

</details>

<details>
<summary>为什么浏览器预览的数据与桌面应用不同？</summary>

浏览器预览使用示例数据展示界面，不读取本机桌面应用的真实凭证和统计。查看真实用量需要运行桌面版本。

</details>

## 开发与构建

技术栈：**Tauri 2 · Rust · React 19 · TypeScript · Vite · Tailwind CSS 4 · SQLite**。

### 环境与启动

项目发布流水线使用 Node.js 22、pnpm 10 和 Rust stable。桌面编译还需要目标系统的 Tauri 构建依赖；Linux 依赖列表和各平台打包参数见[发布工作流](./.github/workflows/release.yml)。

在仓库根目录执行：

```bash
pnpm install
pnpm --dir frontend install

# 浏览器界面预览，使用示例数据
pnpm dev

# 桌面开发，连接 Rust 后端和本机数据
pnpm tauri dev

# 前端类型检查与生产构建
pnpm build

# 桌面打包；默认配置目标为 Windows NSIS
pnpm tauri build
```

macOS 和 Linux 打包应参照发布工作流选择对应 bundle 目标。产物位于 `backend/target/` 下对应构建目标的 `release/bundle/`，不是所有平台都使用 `nsis/`。

首次 Rust 编译可能耗时较长，请查看终端构建输出。若提示找不到 Tauri CLI，确认已在仓库根目录安装依赖；开发端口冲突时，检查 Vite 地址与 Tauri 的 `devUrl` 是否一致。

### 项目结构

```text
cc-usage/
├─ frontend/          React 多窗口前端与浏览器示例数据
│  └─ src/
│     ├─ views/       主面板、灵动岛、设置、托盘等入口
│     ├─ components/  各功能区域与通用组件
│     └─ lib/         API 桥接、平台目录、设置和数据 hooks
├─ backend/           Rust / Tauri 后端
│  └─ src/            采集、数据库、连接、额度、窗口与更新
├─ docs/              使用配图、版本规则与验证记录
├─ scripts/           发布校验、版本计算与验收脚本
└─ .github/workflows/ 多平台构建与发行
```

采集器增量读取本机记录并去重写入 SQLite；额度模块查询当前连接对应的服务；窗口和托盘共享持久化设置。平台能力定义见[平台目录](./frontend/src/lib/platformCatalog.json)。

## 反馈与许可证

- [提交问题或建议](https://github.com/haishishushu/cc-usage/issues)：请提供版本、操作系统、复现步骤、预期与实际结果；截图和日志请隐藏凭证。
- [在线使用文档](https://haishishushu.github.io/cc-usage-website/#/docs) · [发行记录](https://github.com/haishishushu/cc-usage/releases) · [路线图](./todo.md)
- 仓库原说明标注为 MIT，独立的 LICENSE 文件尚未补充。欢迎提交有明确问题描述和验证说明的改进。

## Community 社区

[linux.do](https://linux.do/) - A thriving developer community.

## Star History

<a href="https://www.star-history.com/?repos=haishishushu%2Fcc-usage&amp;type=date&amp;legend=top-left">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/chart?repos=haishishushu/cc-usage&amp;type=date&amp;legend=top-left&amp;theme=dark" />
    <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/chart?repos=haishishushu/cc-usage&amp;type=date&amp;legend=top-left" />
    <img alt="CC Usage Star History" src="https://api.star-history.com/chart?repos=haishishushu/cc-usage&amp;type=date&amp;legend=top-left" width="100%" />
  </picture>
</a>
