# 五平台能力补全（2026-10-01）

来源：鼠鼠要求把 Claude / Codex / Gemini / Zcode / WorkBuddy 没完善的功能补齐。小的先盘点（见对话），分三类：数据来源不提供（不造数）、需外部接口或授权（无已验证途径，不做）、代码层可完善。鼠鼠在可完善项里选定下列四组；Zcode 多供应商套餐查询本次不做。

## 选定范围

### 1. Zcode 会话状态真实化

- [x] Z-1 核对本机库 model_usage.status：completed → done，error / cancelled → failed，其余进行中且 10 分钟内 → running，更旧 → unknown
- [x] Z-2 后端会话查询对 Zcode 输出真实状态，不再按“最近活跃”兜底
- [x] Z-3 前端灵动岛与主面板按真实状态展示 Zcode 会话；运行会话判定复用
- [x] Z-4 单元测试：合成 SQLite 覆盖三种状态

### 3. Gemini tokens.tool 解析

- [x] G-1 解析 tokens.tool 并落库（新增可空列，旧库迁移）
- [x] G-2 主面板 / 来源面板展示工具 Token，缺失显示未知
- [x] G-3 解析与迁移测试

### 4. Gemini / Zcode 费用估算

- [x] P-1 取官方公开价目（Gemini、BigModel GLM）并请鼠鼠确认：只做 Gemini（美元、精确 id、Pro 按 20 万 Token 分档、思考按输出价、工具按输入价）；Zcode 因人民币计价、订阅制与主力模型无 API 标价不做
- [x] P-2 价目表写入 pricing.rs，能力表开启 cost_estimate，界面标注“估算”
- [x] P-3 价目覆盖测试；未覆盖模型计入 uncovered_tokens

### 5. 代理流量按连接归账（架构，先设计后实现）

- [x] A-1 设计说明：请求记录新增连接标记，代理按平台当前启用连接打标
- [x] A-2 鼠鼠确认设计后实施：库表迁移、代理写入、统计查询按连接筛选、界面入口
- [x] A-3 测试：代理记录带连接 id；未经代理的记录保持无归属

### 6. 文档债清理

- [x] D-1 todo.md OPT-06 等过时条目更新为现状
- [x] D-2 docs/superpowers/plans/2026-09-22-eight-platform-integration.md 复选框按代码现状勾选
- [x] D-3 spec 现状表、README 与官网文档同步新能力

## 验证

- [x] V-1 cargo test、前端 node --test、tsc、vite build
- [ ] V-2 官网 pnpm test / build
- [x] V-3 Git 提交并推送（2026-10-07 鼠鼠授权）
