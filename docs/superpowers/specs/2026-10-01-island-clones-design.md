# 灵动岛分身 · 设计说明（2026-10-01）

鼠鼠已批准方案 A（本体 + 分身列表）。设计稿见 pen 画布 22 / 22D。

## 目标

- 灵动岛右键菜单新增「开启分身」「销毁分身」，至少保留一个灵动岛。
- 右键菜单面板右上角以纯数字显示灵动岛总数（含本体）。
- 每个岛各自独立选择连接、停靠边与偏移、自由态位置，重启后原样恢复。

## 数据模型

- 本体继续使用 `Settings.island_platform / island_kind / island_connection_id / island_connection_name / island_source_id / dock`，旧配置零迁移。
- 新增 `Settings.island_clones: Vec<IslandClone>`，每条含 `id`、`platform`、`kind`、`connection_id`、`connection_name`、`source_id`、`dock`、`position`（自由态左上角物理像素）。
- 窗口标签：本体 `island`，分身 `island-<id>`；capability 用 `island-*` 通配。

## 后端职责（`island_clones.rs` 纯逻辑 + `lib.rs` 窗口操作）

- `profile(settings, id)`：把本体与分身统一成同一形状供命令与托盘使用。
- `create_clone(settings, source)`：复制来源岛的连接；总数达 8 拒绝。
- `destroy(settings, target)`：总数为 1 拒绝；销毁分身直接移除；销毁本体时第一个分身的配置接任本体，并关闭该分身窗口、把本体窗口挪到它的位置。
- 命令按调用方窗口定位岛：`resize_island`、`island_drag`、`dock_hover`、`dock_release`、`dock_undock`、`island_menu`。
- 右键菜单打开时记录目标岛；`menu_action` 透传给 `handle_tray_menu`，`pos_*`、`clone_create`、`clone_destroy`、切换连接都作用于目标岛。
- `dock-changed`、`dock-hint` 只发给对应岛窗口；`settings-changed` 仍广播。
- 全局项（始终置顶、显示 / 隐藏、重置位置）遍历全部岛窗口。
- 连接改名 / 移除同步更新分身条目。

## 前端职责

- `lib/islandProfile.ts`：从 URL `clone` 参数取岛 id，从设置里取本岛 profile。
- `IslandWindow.tsx` 改读 profile；岛内切换连接带上本岛 id。
- `ContextMenuWindow.tsx`：两个新菜单项、右上角总数、勾选态按目标岛显示。

## 不做的事

- 托盘菜单不加分身项、不显示数字。
- 设置页「灵动岛显示配置」仍只管本体。
