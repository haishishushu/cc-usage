# 灵动岛分身（2026-10-01）

来源：鼠鼠要求灵动岛右键菜单可以「开启分身 / 销毁分身」，至少保留一个灵动岛，右键菜单右上角用纯数字显示当前灵动岛数量；先画好 pen 画布再写代码。

## 已确认的需求

- 每个分身**各自独立选连接**，位置、停靠各自独立并持久化。
- 右上角数字 = **全部灵动岛总数**（含本体），只有一个岛时显示 1。
- 数字放在**右键菜单面板的右上角**，不画在灵动岛本体上。
- 重启软件后分身**自动恢复**（数量、位置、各自连接）。

## 小的的推断（未经鼠鼠确认的默认值，有异议随时改）

- 分身上限 8 个，防止误操作开出一屏窗口。
- 「开启分身」以右键所在岛的连接为初始值，新窗口在其右下 24px 自由态出现。
- 「销毁分身」销毁右键所在的岛；总数为 1 时禁用。销毁本体时由第一个分身接任本体。
- 托盘菜单保持不变：不含分身项，也不显示数字；托盘摘要、主面板默认连接跟随本体。
- 始终置顶、透明度 / 大小、免打扰、显示 / 隐藏灵动岛为全局，作用于所有岛。
- 「重置窗口位置」：本体回主屏居中，分身解除停靠并依次向右下错开。

## 清单

### 设计稿

- [x] D-1 在 pen 画布 18D 下方新建「22 灵动岛右键菜单 · 分身管理」浅色画板：仅本体（总数 1，销毁禁用）、三个岛（总数 3，销毁可用）、切换连接子菜单体现分身独立选择、桌面多岛示意、规则注记
- [x] D-2 复制一份深色变体「22D」
- [x] D-3 截图自检：无裁切、对齐、对比度

### 方案与计划

- [x] P-1 在对话里给出方案对比（本体 + 分身列表 vs 全量统一列表）并获鼠鼠批准
- [x] P-2 写设计说明到 `docs/superpowers/specs/2026-10-01-island-clones-design.md`
- [x] P-3 鼠鼠批准方案后直接按本清单实施（未另出计划文档）

### 后端（Rust）

- [x] B-1 `settings.rs`：新增 `island_clones: Vec<IslandClone>`（id、connection_id/name、platform、kind、source_id、dock、free 位置），旧字段继续代表本体
- [x] B-2 `lib.rs`：窗口标签 `island` / `island-<id>`，`ensure_island_window` 泛化为按 id 创建；启动时重建全部分身
- [x] B-3 命令按来源窗口定位岛：`resize_island`、`island_drag`、`dock_hover`、`dock_release`、`dock_undock`、`island_menu`
- [x] B-4 右键菜单携带目标岛 id：`context_menu::open(app, Some(id))`，`menu_action` 透传到 `handle_tray_menu`
- [x] B-5 菜单动作 `clone_create` / `clone_destroy` 经 `menu_action` 透传（总数为 1 拒绝；销毁本体时分身接任）；总数由前端按设置计算，无需新命令
- [x] B-6 `set_island_connection` 增加可选 `island` 参数；连接改名 / 移除同步更新分身条目
- [x] B-7 `island_topmost`、`set_island_visible`、`reset_window_layout` 作用于全部岛
- [x] B-8 `Moved / ScaleFactorChanged` 事件对所有岛标签生效；`dock-changed`、`dock-hint` 只发给对应窗口
- [x] B-9 `capabilities/default.json` 加入分身窗口标签

### 前端（React）

- [x] F-1 `useIslandProfile`：按 URL 的 `clone` 参数从设置里取本岛的连接与停靠，形状与现有 `island_*` 一致
- [x] F-2 `IslandWindow.tsx` 改读本岛 profile；切换连接时带上本岛 id
- [x] F-3 `ContextMenuWindow.tsx`：新增「开启分身 / 销毁分身」两行、右上角总数数字；勾选态按目标岛显示
- [x] F-4 `api.ts` 新增命令封装与设置类型

### 验证

- [x] V-1 `cargo test`：215 通过（含分身纯逻辑 5 项）（新增：设置升级兼容、分身创建 / 销毁 / 接任、总数下限）
- [x] V-2 前端 `node --test` 130 通过、`tsc`、`vite build` 通过
- [x] V-3 真实桌面端（隔离目录 + CDP 驱动，`scripts/acceptance/island-clones.e2e.mjs`）：仅本体时总数 1、销毁禁用 → 开两个分身总数 3、位置落盘 → 销毁本体由第一个分身接任、本体窗口保留 → 重启后分身按同一 id 与位置重建、总数 2 → 销毁后设置清空。隔离目录无连接，未覆盖“各选不同连接”的真机点选；该逻辑由后端 `apply_connection` 单测与前端 `islandProfile` 单测覆盖
- [ ] V-4 鼠鼠在安装版上实际用两个不同连接各开一个分身，确认额度各自显示

### 官网同步（cc-usage-website）

- [x] W-1 中英文文档：功能表、「开启灵动岛」步骤、灵动岛指南新增「右键菜单与分身」章节
- [x] W-2 更新日志：顶部新增「灵动岛分身 · 开发中」条目与英文映射；正式发布时改成版本号与提交号
- [x] W-3 预览翻译表：设置页「本体仅使用一个连接，分身各自选择」新文案的英文映射
- [x] W-4 内嵌桌面预览由 CI 从 cc-usage 主分支重建；本地 `pnpm build` 已验证可通过
