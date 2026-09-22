# 已审查设置界面实施与验收

## 改动总结

- 按 `prd/pencil-new.pen` 审查稿实现连接管理、灵动岛、常规、外观、数据的统一设置页。导航与一级标题同名同序，点击定位预留约 16px，滚动同步高亮。
- 连接只提供启用／使用中、连接／断开、移除。操作按钮均为 86×32，使用中灰色禁用；断开使用 Unplug 图标。表格居中、名称与类型徽标成组，长名称省略并提供 title。
- 移除重复的平台／连接／来源配置区。指标预览按连接数自动扩展为两列多行。
- 读取 CC Switch 已应用到本机 Claude / Codex 的配置；不直接编辑 CC Switch 或 CLI 文件。按平台、类型、完整凭证和地址去重，新记录为断开状态。Codex 从选中的 config.toml provider 读取地址与 env_key；缺少自定义地址时停止读取并提示。
- 后端阻止启用未连接项目；恢复连接先验证保存的凭证。断开保留选择和凭证，不自动切换账号。移除仅删除本应用副本，不删除历史统计。
- Tauri 原生导入文件选择器、导出另存为；取消返回空结果。导入限制 100MB、支持 UTF-8 BOM、复用校验和事务去重。导出在目标目录临时写入并同步后替换，写入成功才显示路径。
- 设置页在窗口失去可见状态时保持挂载，防止原生弹窗或窗口切换期间丢失操作状态。

## 已执行验证

- `pnpm --dir frontend build`：通过（保留已有 window 模块静态／动态导入提示）。
- `cargo test --manifest-path backend/Cargo.toml --lib`：71 通过、4 忽略、0 失败；忽略项目需要真实本机数据或授权。
- `cargo build --manifest-path backend/Cargo.toml`：通过。
- `node --experimental-strip-types --test frontend/src/lib/*.test.mjs`：23 通过。
- 隔离桌面 `approved-settings.cjs`：暂停／恢复保留选择、未连接启用被拒绝、单一启用、9 个按钮尺寸、两列三卡、导航位置与高亮通过。
- 隔离桌面 `settings.cjs`：置顶、停靠、免打扰、透明度、大小、刷新、余额提醒、主题跨窗口同步、保留时间、托盘同步、无效输入拒绝与页面重载通过。
- `settings-layout.cjs`：数据分区定位、页面文件输入移除、桌面截图通过。
- 新增文件测试覆盖已有备份替换、UTF-8 中文/BOM 读写及临时文件清理；原有数据库测试覆盖备份去重和不导出凭证。

## 验收边界

电脑自动化工具报告 `Computer Use native pipe is unavailable`，重试和重置后仍失败。因此系统原生弹窗的文件选择、取消与覆盖确认尚未完成人工操作验收，不能把文件函数测试视为这部分交互已验收。真实用户凭证未参与网络测试，桌面验证使用隔离数据库与本地模拟网关。

截图位于 `output/acceptance/approved-settings-light.png`、`approved-settings-dark.png`、`approved-settings-data.png`。
