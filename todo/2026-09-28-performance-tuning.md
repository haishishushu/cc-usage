# 性能调优（2026-09-28）

来源：三路只读性能审查（采集链路 / DB·命令·代理·网络 / 前端），高优先级项已由小的逐条核对源码。
鼠鼠要求：全部优化；“停靠时是否暂停轮询”按小的推荐处理。
约束：在工作区已有的未提交改动（启动窗、主面板隐藏、idx_usage_events_request）之上修改，不回退；不执行 Git 写操作。

基线工具：`cargo test --release --lib perf_bench -- --ignored --nocapture`（复制本机真实库计时，只输出耗时）。

## P0 小改动大收益

- [x] P0-1 requests 增加 `(platform, session_id, ts)` 索引（live_usage 相关子查询、清理 NOT EXISTS）
- [x] P0-2 live-usage 推送：后端区分“数据变化”与“心跳续期”（`data_changed`）；前端总览 / 费用 / 积分只在数据变化时刷新，1.5 秒首次立即 + 尾沿合并
- [x] P0-3 额度/余额/API 用量定时轮询：后端新增 `max_age_ms` 刷新模式；前端定时器取“间隔 − 15 秒”，窗口重置补查取 1 秒
- [x] P0-4 `tray_summary_get` 改为 async 并走只读连接，不在主线程拿 DB 锁
- [x] P0-5 `scan_paths` 只在变化路径属于本机来源目录时才扫对应来源

## P1 结构性

- [x] P1-1 读写分离：统计/日志/实时快照走 3 连接只读池；采集按文件持锁，未变化文件在锁外预过滤
- [x] P1-2 本机来源：Qoder/WorkBuddy 按行边界增量续读；Zcode、会话元数据按库戳记跳过；Qoder 快照懒加载
- [x] P1-3 灵动岛追数不再整树每帧重渲染（LiveCounts 外部存储 + 叶子订阅）；sessions 引用稳定；FLIP 只在列表变化时测量
- [x] P1-4 心跳降本：位点一次读入、目录枚举不再逐项 stat、Codex 标题索引按戳记跳过、只补绑失效监听
- [x] P1-5 设置页连接预览：面板隐藏时暂停全部轮询与订阅；卡片改用 useLiveToday，不跑追数动画
- [x] P1-6 停靠未探出：额度照常（停靠条画水位），余额/API 用量/费用/积分暂停，探出补查；更正原误导注释

## P2 顺手优化

- [x] P2-1 Claude/Codex 每行只解析一次 JSON
- [x] P2-2 写入路径统一 prepare_cached（语句缓存 64）；清理下限查询复用
- [x] P2-3 灵动岛非原生平台不查 source_metrics
- [x] P2-4 代理流结束时 try_lock，拿不到入待关联队列
- [x] P2-5 托盘摘要窗口保温复用：离开后隐藏 60 秒再销毁，重新悬停复用并重读；隐藏期间不轮询
- [x] P2-6 额度查询 HTTP 客户端复用（120 秒重建以跟随系统代理）
- [x] P2-7 token_totals 四周期合并为一次扫描、usage_breakdown 命中率分母合并；分页实测 0.6ms 不改
- [x] P2-8 字体 woff2 子集化 + preload：Inter 876→145KB、JetBrains Mono 187→52KB；安装包配图脚本同步改用 woff2；移除 public 里的 TTF
- [x] P2-9 托盘图标颜色与尺寸未变不重复 set_icon
- [x] P2-10 Gemini 解析 O(n²) → HashMap
- [x] P2-11 SQLite PRAGMA：cache_size、temp_store、busy_timeout、启动 optimize
- [x] P2-12 useSettings 每窗口共享一份快照；连接切换器按需加载（灵动岛首屏共享块 95.7→7KB）；图表路径与系列 memo（演示数据仅 7KB，保留静态引入）

## 验证

- [x] cargo test：210 通过（新增只读池、四周期合并口径、本机增量续读、定向扫描、心跳预过滤、最大缓存年龄）
- [x] 前端 node --test 125 通过（新增节流 / 变化判定）、tsc、vite build 通过
- [x] 浏览器验收（端口改 5174 的临时副本）：hidden-panel-polling、panel-visibility、session-list-scroll、motion-performance 通过；settings-no-flash 补 UpdateProvider/ToastProvider 后通过（原脚本缺 Provider，与本次无关）；usage-refresh-stability 读取已不存在的 `chart.bars` 字段，脚本过期，与本次无关
- [ ] 真实桌面端运行验证：本机正在运行安装版（单实例），未启动开发版以免打断；需鼠鼠重启到新构建后观察
- [x] 真实数据计时（release）：心跳 52.5→13.0ms；写事件 15.9→0.06ms；本机来源 15.0→0.37ms；live_usage codex 22.4→0.53ms；总览 codex/累计 117→73ms；冷启动回填 3.3GB 6.2s
