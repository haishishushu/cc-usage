# 单智能体验收工具

仅用于本仓库开发验收。UI 自动化使用 Playwright CLI，数据库/HTTP 使用真实应用代码；网关服务返回明确的合成数据，不能替代真实账号联调。

## 隔离桌面流程

1. `cargo build --manifest-path backend/Cargo.toml`，另开终端运行 `pnpm --dir frontend dev`。
2. 确认 9337 与 18765 没有旧验收进程占用，运行 `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/acceptance/start.ps1`。每次生成新的带标记的隔离目录，禁用本机日志采集/凭证发现/开机启动修改；WebView 缓存也独立。
3. `npx --yes --package @playwright/cli playwright-cli -s=acceptance open http://localhost:5173`
4. 依次通过 `run-code --filename` 执行 `desktop.cjs`、`http.cjs`、`windows.cjs`、`geometry.cjs`、`visual.cjs`。桌面脚本执行前核验数据路径属于 output/acceptance/profile；只在新测试目录运行完整增删测试。
5. `prepare-visual.ps1` 临时创建前端状态夹具，运行 `states.cjs` 后删除 `frontend/__acceptance.html` 和 `frontend/__acceptance.tsx` 两个临时文件；不删除其他前端文件。
6. `performance.ps1 -Minutes 10` 每 15 秒保存进程树工作集、私有内存、CPU 累计时间与响应状态，报告位于 output/acceptance。每次测试须确认 processes.json 的 PID 仍属于本次进程。
7. 完成后关闭 acceptance 浏览器，并仅停止 processes.json 中本次创建的 app/gateway PID。不要结束其他同名应用。

环境变量 `CC_USAGE_TEST_DIR` 只在 debug 构建生效；路径必须为绝对路径且已有 `.acceptance-profile` 标记。正式安装包忽略该开关。CDP 端口仅由启动脚本设置，不写入产品代码。

## 真实只读接口

明确获用户授权后执行忽略的 `local_auth_read_only_smoke`（Codex）或 `local_claude_gateway_read_only_smoke`（Claude 网关）。不打印 Key、token、账号、历史记录或完整响应，不发送模型请求。其他账号类型缺失时记录缺口。

## Windows Sandbox 安装检查

运行 `prepare-sandbox.ps1` 生成 output/acceptance/sandbox/install-check.wsb。仅在已安装 Windows Sandbox 的电脑打开；脚本拒绝在普通宿主用户下执行。检查安装、同版本覆盖升级、默认卸载保留数据、升级保留开机启动及卸载删除启动项；它不等于跨版本升级或每个 WebView2 版本已验收。报告写入 sandbox-result.json。

依据：[Tauri NSIS hooks](https://v2.tauri.app/distribute/windows-installer/)、[Windows Sandbox 配置](https://learn.microsoft.com/en-us/windows/security/application-security/application-isolation/windows-sandbox/windows-sandbox-configure-using-wsb-file)。包和脚本映射为只读，仅结果目录可写。
