# CC Usage · 前端

Windows 桌面 AI 用量监控工具的前端实现。设计参考 `../prd/pencil-new.pen`，
行为与验收范围以 `../prd/Pencil-UI设计需求.md` 和 `../todo.md` 为准。

## 技术栈

Tauri 2 + Rust + **React 19 + TypeScript + Vite + Tailwind CSS 4 + shadcn/ui** + SQLite

本目录只包含 React 前端；Rust / Tauri / SQLite 后端位于 `../backend`，已接入窗口、托盘、采集、统计与只读查询。

## 运行

```bash
pnpm install
pnpm dev        # http://localhost:5173
pnpm build      # 输出到 dist/，供 Tauri 打包
pnpm typecheck
```

预览页顶部可切换：主面板 / 灵动岛 / 画布对照，以及浅色 / 深色主题。
Tauri 下用查询参数区分窗口：`?window=island` 为灵动岛，默认为主面板。

## 目录结构

```
src/
  index.css                 设计变量（@theme inline + :root/.dark），对应附录 A.2
  types.ts                  平台 / 连接 / 来源 / 额度窗口等领域类型
  lib/quota.ts              额度与余额的水位着色阈值
  mock/data.ts              样例数据，全部取自 pencil-new.pen，未自行编造
  components/
    brand/                  PlatformLogo（四个平台的矢量路径）、AppIcon
    ui/primitives.tsx       Button / Chip / StatusBadge / Toggle / Segmented 等
    island/                 UsageRow、TokenDelta、SessionList、UsageIsland、DockedIsland
    panel/                  Chrome、UsageCard、UsageChart、RequestLogTable、Pagination、
                            BalanceCard、DateRangePicker
    settings/               ConnectionTable、ConnectionDialogs
    tray/                   TrayMenu、TrayPlatformSubmenu、TrayTooltip
  views/                    IslandWindow、MainPanelWindow、OverviewView、SettingsView
```

## 实现时已遵守的关键约束

来自需求文档附录 A.5，代码里也有对应注释：

1. **5h / 7d 标签颜色不随水位变**——标签是窗口身份色，只有进度条按 75% / 90% 换色。
2. **所有数字用等宽数字**（`.tnum`），百分比列固定 34px、倒计时列固定 56px 并右对齐。
3. **增量区宽度固定 112px**，空闲时保留空白，出现或消失不改变灵动岛宽度。
4. **进度条按百分比精确绘制**。
5. **失败 ≠ 0% ≠ 满格**，三者可区分。
6. **深色不是反色**，逐项取附录 A.2 的值。
7. **文字绿 `#06834A` 与进度条绿 `#06C167` 是两个值**，文字一律用前者。
8. **应用图标不随主题变化**；平台标识随主题换色。
9. **文本溢出单行截断加「…」**（`truncate` + `title`），数值与金额不省略。
10. **日志表窄窗口横向滚动，不隐藏列**，七列宽度之和恒为 1064。
11. **趋势图靠等分 flex 行对齐**，不用绝对定位。
12. **组件不硬编码颜色**，一律走 CSS 变量。

## 实现状态

- Tauri 桌面端使用真实连接、额度、余额、本地会话采集与 SQLite 统计；`src/mock/data.ts` 只服务浏览器设计预览。
- 字体使用系统字体回退链，离线运行不请求 Google Fonts。
- 原生托盘、窗口拖动与贴边停靠由 Rust 侧实现，浏览器中的托盘组件只用于设计预览。
- 未完成与待联调项目统一记录在 `../todo.md`。
