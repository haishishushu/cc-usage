# 八平台接入验收记录

本次交付是当前工作区代码，未打包安装，未提交 Git。已完成可验证来源的前后端链路；Trae IDE 和部分线上账户额度仍有明确缺口，不能称八个平台全部功能已完成或已实测。

## 改动总结

1. 八平台共用能力目录，连接操作按能力开放。本机来源检测与线上凭证检测分开，监控开关只控制本应用，不冒称登录账号切换。
2. 新增 Gemini CLI、Zcode、Qoder 国内／国际、Workbuddy 双目录采集。请求稳定去重、快照覆盖更新、上下文独立存储，备份导入重复执行不累加。
3. 缓存创建保留真实 0；缺失为 `null`，前端显示 `—`。输入是否含缓存由记录口径决定，命中量不重复叠加到输入；上游总量不会再加一次思考 Token。
4. 原生数据接入统计、趋势、请求日志、会话、灵动岛、托盘及导入导出。积分、Token、上下文占用分别展示。缺少精确 Token 的 Qoder 数据仍可展示上报积分。
5. watcher 监听来源目录和 SQLite/WAL；坏文件不阻断其他会话，半行不消费，回退和历史清理不复活旧记录，事件游标保持单调。
6. 修复 Claude / Codex 切换配置优先级，Auth 不拼接不同身份的 access/refresh token；两文件更新失败恢复原文件。测试仅写隔离临时目录。
7. Gemini 官方模型列表和 xAI Key 信息接口用于只读 Key 检测；xAI 三个停用字段缺失或任一为 true 均不判成功。固定官方 HTTPS 端点、禁止重定向，不调用生成或猜测的余额接口。

## 逐平台矩阵

| 平台 | 连接 | Token / 缓存 | 会话 / 灵动岛 | 额度 / 余额 | 本次验证 |
| --- | --- | --- | --- | --- | --- |
| Claude | 保留 Auth/API；修复配置应用 | 现有完整采集；真实零与缺失分离 | 保留原有生命周期链路 | 保留现有按凭证能力查询 | 回归测试；此前本机日志与数据库核对；未执行真实账号切换 |
| Codex | 保留 Auth/API；修复 provider 切换和身份验证 | 保留采集；含缓存输入不重复相加 | 保留生命周期及灵动岛 | 保留现有额度／组织用量／网关能力 | 回归及隔离配置测试；未执行真实账号切换 |
| Gemini | 本机 CLI 监控；官方 Key 只读检测 | JSON/JSONL 回放；缓存创建未上报时未知 | 接共享本机指标；无事件不推断正在运行 | 线上订阅额度未实现 | 官方格式夹具、Key 响应／地址单测、界面验证；无本机 CLI 样本和真实 Key 联调 |
| Grok | 本机 OAuth 与 xAI Key 两条独立路径 | 本机会话格式未验证，未完成 | 可选平台；无数据不编造 | 复用原有消费版 OAuth 积分模块；Management 余额未实现 | Key 响应／地址单测、界面验证；本机无 OAuth 样本，不能宣称真实额度联调成功 |
| Zcode | 原生数据库监控 | model_usage 的真实 Token／缓存；失败占位零未知 | 关联 session 标题和真实完成／失败状态 | 数值剩余额度未接通 | 只读本机 SQLite：21 行，其中 14 条总量已知，7 条失败行总量未知 |
| Trae | 只有来源检测和限制说明 | IDE 格式未取得，未完成 | 无真实数据链路，不编造 | 未完成 | 入口和缺失状态；等待实际 IDE 数据路径／样本，不使用 trae-agent 替代 |
| Qoder | 国内／国际来源独立；空实例不阻断有效实例 | 已有样本 Token 不可用，保留未知；上报积分独立 | 会话和上下文快照；灵动岛显示真实可用指标 | 线上额度未接通；不解密或猜测 token | 本机两条去重用量记录；Token 已知条数为 0；幂等、字段与注册测试 |
| Workbuddy | 双目录本机来源监控 | 真实 Token／缓存／积分；function_call 用量也采集 | 关联会话；used/size 仅用于上下文；岛和托盘同单位 | 账户余额未接通；请求积分非余额 | 本机 255 条；已上报总量 25,045,365，命中 23,976,096；隔离桌面链路通过 |

上述平台合计为本机来源统计，不作为某个 API Key 的独立账单。上游写入字段明确为 0 时显示 0，不解释为“已合并计费”；缺字段时无法反推真实写入量。

## 验证证据

- `cargo test --manifest-path backend/Cargo.toml --lib`：189 passed、0 failed、7 ignored。ignored 中可能访问真实凭证／网络的测试未运行。
- `cargo test --manifest-path backend/Cargo.toml --lib read_only_native_smoke -- --ignored --nocapture`：1 passed。仅只读真实来源，写内存应用库；核对重复扫描零增量和备份往返。执行时尚未添加官方 Key 检测，后续改动不涉及采集解析。
- `node --test frontend/src/lib/*.test.mjs`：105 passed，0 failed。
- `pnpm --dir frontend build`：TypeScript 和 Vite 成功。已有 `@tauri-apps/api/window.js` 静态／动态导入混用提示仍在，不能称前端零警告。
- `git diff --check`：通过；存在工作区换行转换提示，无 whitespace error。
- Playwright：八平台添加入口逐一检查；Gemini/Grok 手动 Key 可选，本机 Key 读取禁用；空 Key 验证阻止提交；未实现的模型／强度设置不暴露。未发送真实或虚构 Key 到远程接口。
- 独立 Tauri profile：导入合成 Workbuddy 数据（总量 1020、命中 800、创建缺失、积分 0.65、上下文 25%）；灵动岛显示积分 0.65，托盘显示 0.6500 积分，导出再导入新增 0 条。本次桌面测试可执行文件构建于新增官方 Key 检测前，官方 Key 逻辑另由单测和前端检查覆盖。
- 截图：`output/playwright/native-island-desktop.png`、`native-add-dialog.png`、`gemini-key-dialog.png`，已人工查看无关键文案截断。
- 只读复核所发现问题均按执行记录修正；测试不是八平台真实账号网络联调的替代。

## 官方依据

- [Gemini OpenAI 兼容模型列表](https://ai.google.dev/gemini-api/docs/openai)：只读 GET `/v1beta/openai/models`，Bearer API Key。
- [xAI Key 信息](https://docs.x.ai/developers/rest-api-reference/inference/other)：GET `/v1/api-key`，成功响应还需判断 Key／团队停用标记。
- [Gemini CLI 记录与回放](https://github.com/google-gemini/gemini-cli/blob/main/packages/core/src/services/chatRecordingService.ts)：快照、替换及回退语义。

## 尚未完成

- Trae IDE 的实际会话／用量／额度数据格式。已集中询问鼠鼠本机安装或数据路径，尚无回答。
- Gemini / Grok 无真实可用凭证样本，本次未验证线上成功响应；不要求鼠鼠在聊天中粘贴密钥。
- Zcode、Qoder、Workbuddy 的线上数值剩余额度，以及 xAI 专用 Management 余额链路。接口、凭证作用域或有效响应尚未完整验证，当前返回具体不可用原因。
- 全部平台的登录切换不能用本机监控替代；目前仅 Claude / Codex 实现外部配置应用。其他平台由原应用管理登录。
