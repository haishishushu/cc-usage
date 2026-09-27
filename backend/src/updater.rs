//! 应用自更新（画布 17 · 更新功能）
//!
//! 走 tauri-plugin-updater：更新端点与签名公钥配置在 tauri.conf.json，
//! 发布时用 `tauri signer` 私钥对安装包签名，Release 附 latest.json。
//! 前端只在启动 1 秒后的静默检查与设置页手动检查时调用 `check_app_update_available`；
//! `download_app_update` 下载并校验签名，`install_downloaded_update_and_restart`
//! 在用户点击安装后使用已下载的安装包，避免再次下载。

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_updater::{Update, Updater, UpdaterExt};
use url::Url;

/// 按本机代理设置构造 updater。
///
/// 必须显式注入代理：插件内部的 reqwest 与本项目依赖的不是同一个大版本
/// （插件 0.13 / 本项目 0.12），Cargo 的特性统一不跨大版本，我们开启的
/// `system-proxy` 传不过去。不显式设置的话，开着系统代理的用户在检查更新时
/// 仍会直连 GitHub 而超时。
fn updater_with_proxy(app: &AppHandle) -> Result<Updater, String> {
    let mut builder = app.updater_builder();
    if let Some(proxy) = system_proxy() {
        builder = builder.proxy(proxy);
    }
    builder.build().map_err(|e| e.to_string())
}

/// 读取本机代理地址。优先环境变量（跨平台惯例，也便于用户临时覆盖），
/// 其次 Windows 系统代理（注册表）。都没有则返回 None 走直连。
fn system_proxy() -> Option<Url> {
    for key in [
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
        "HTTP_PROXY",
        "http_proxy",
    ] {
        if let Some(value) = std::env::var_os(key) {
            let value = value.to_string_lossy();
            if let Some(url) = parse_proxy_address(&value) {
                return Some(url);
            }
        }
    }
    windows_system_proxy()
}

/// 读取 Windows「Internet 选项」里的系统代理（Clash / v2ray 等客户端写在这里）。
///
/// 用 `reg query` 而不是引入注册表 crate：只在这一处用到，避免为单点功能
/// 增加依赖；子进程隐藏窗口，避免 GUI 下闪黑框。
#[cfg(target_os = "windows")]
fn windows_system_proxy() -> Option<Url> {
    const INTERNET_SETTINGS: &str =
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";

    if !matches!(
        reg_query(INTERNET_SETTINGS, "ProxyEnable").as_deref(),
        Some(value) if value.trim_start_matches("0x").trim_start_matches('0') != ""
    ) {
        return None;
    }
    parse_proxy_address(&reg_query(INTERNET_SETTINGS, "ProxyServer")?)
}

#[cfg(not(target_os = "windows"))]
fn windows_system_proxy() -> Option<Url> {
    None
}

/// 取注册表单个值的数据部分；查不到或进程失败时返回 None。
#[cfg(target_os = "windows")]
fn reg_query(path: &str, name: &str) -> Option<String> {
    use std::os::windows::process::CommandExt;
    /// 不给子进程分配控制台窗口
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let output = std::process::Command::new("reg")
        .args(["query", path, "/v", name])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_reg_value(&String::from_utf8_lossy(&output.stdout), name)
}

/// 从 `reg query` 输出里取出指定值的数据部分。
///
/// 输出形如：`    ProxyServer    REG_SZ    127.0.0.1:7890`，
/// 值本身可能含空格，所以按类型列切分而不是取最后一段。
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn parse_reg_value(stdout: &str, name: &str) -> Option<String> {
    stdout.lines().find_map(|line| {
        let rest = line.trim().strip_prefix(name)?;
        let rest = rest.trim_start();
        // 跳过类型列（REG_SZ / REG_DWORD 等）
        let (_, data) = rest.split_once(char::is_whitespace)?;
        let data = data.trim();
        (!data.is_empty()).then(|| data.to_string())
    })
}

/// 归一化代理地址为带协议的 URL。
///
/// 兼容系统代理的两种写法：整串 `host:port`，以及分协议的
/// `http=host:port;https=host:port`（取 https，其次 http）。
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn parse_proxy_address(raw: &str) -> Option<Url> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    let address = if raw.contains('=') {
        let mut https = None;
        let mut http = None;
        for part in raw.split(';') {
            let Some((scheme, value)) = part.split_once('=') else {
                continue;
            };
            match scheme.trim().to_ascii_lowercase().as_str() {
                "https" => https = Some(value.trim()),
                "http" => http = Some(value.trim()),
                _ => {}
            }
        }
        https.or(http)?.to_string()
    } else {
        raw.to_string()
    };

    let address = if address.contains("://") {
        address
    } else {
        format!("http://{address}")
    };
    Url::parse(&address).ok()
}

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateCheck {
    pub available: bool,
    pub current_version: String,
    pub available_version: String,
    pub notes: Option<String>,
    pub pub_date: Option<String>,
}

/// 下载进度事件负载；total 在拿到 content-length 前为 None
#[derive(Clone, serde::Serialize)]
pub struct UpdateDownloadProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
}

struct PreparedUpdate {
    update: Update,
    bytes: Vec<u8>,
}

pub struct UpdateDownloadState {
    prepared: Mutex<Option<PreparedUpdate>>,
    downloading: AtomicBool,
}

impl Default for UpdateDownloadState {
    fn default() -> Self {
        Self {
            prepared: Mutex::new(None),
            downloading: AtomicBool::new(false),
        }
    }
}

struct DownloadGuard<'a>(&'a AtomicBool);

impl Drop for DownloadGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

fn update_check(update: &Update) -> UpdateCheck {
    UpdateCheck {
        available: true,
        current_version: update.current_version.clone(),
        available_version: update.version.clone(),
        notes: update.body.clone(),
        pub_date: update.date.map(|date| date.to_string()),
    }
}

/// 只查询是否有新版本，不下载。端点不可达或公钥校验失败时返回 Err，
/// 由前端决定静默（启动检查失败不打扰）或提示（手动检查 toast）。
#[tauri::command]
pub async fn check_app_update_available(app: AppHandle) -> Result<UpdateCheck, String> {
    let updater = updater_with_proxy(&app)?;
    Ok(match updater.check().await.map_err(|e| e.to_string())? {
        Some(update) => update_check(&update),
        None => UpdateCheck {
            available: false,
            current_version: app.package_info().version.to_string(),
            available_version: String::new(),
            notes: None,
            pub_date: None,
        },
    })
}

/// 下载并校验签名，保留安装包供用户在安装界面一键安装。
#[tauri::command]
pub async fn download_app_update(
    app: AppHandle,
    state: State<'_, UpdateDownloadState>,
) -> Result<UpdateCheck, String> {
    if state.downloading.swap(true, Ordering::AcqRel) {
        return Err("更新包正在下载中".into());
    }
    let _guard = DownloadGuard(&state.downloading);
    *state.prepared.lock().map_err(|e| e.to_string())? = None;
    async {
        let updater = updater_with_proxy(&app)?;
        let update = updater
            .check()
            .await
            .map_err(|e| e.to_string())?
            .ok_or("当前已是最新版本")?;
        let info = update_check(&update);
        let progress_app = app.clone();
        let mut downloaded: u64 = 0;
        let bytes = update
            .download(
                move |chunk, total| {
                    downloaded = downloaded.saturating_add(chunk as u64);
                    let _ = progress_app.emit(
                        "update-download-progress",
                        UpdateDownloadProgress { downloaded, total },
                    );
                },
                || {},
            )
            .await
            .map_err(|e| format!("下载更新失败：{e}"))?;
        *state.prepared.lock().map_err(|e| e.to_string())? =
            Some(PreparedUpdate { update, bytes });
        Ok(info)
    }
    .await
}

/// 安装已校验的更新包并重启。
///
/// Windows：NSIS 安装器拉起后当前进程直接退出，新版本由安装器自动重启；
/// 插件 install 内部会硬退出（不走 Drop），因此托盘移除必须前置，
/// 否则安装器接管期间会残留无法自行消失的死图标。
/// macOS / Linux：install 原地替换后正常返回，手动 restart 进新版本。
#[tauri::command]
pub async fn install_downloaded_update_and_restart(
    app: AppHandle,
    state: State<'_, UpdateDownloadState>,
) -> Result<(), String> {
    let prepared = state
        .prepared
        .lock()
        .map_err(|e| e.to_string())?
        .take()
        .ok_or("安装包尚未下载完成，请先点击更新版本按钮")?;

    #[cfg(target_os = "windows")]
    let _ = app.remove_tray_by_id("main-tray");

    if let Err(error) = prepared.update.install(&prepared.bytes) {
        *state.prepared.lock().map_err(|e| e.to_string())? = Some(prepared);
        return Err(format!("安装更新失败：{error}"));
    }

    #[cfg(not(target_os = "windows"))]
    app.restart();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{parse_proxy_address, parse_reg_value};

    #[test]
    fn proxy_address_accepts_bare_host_port() {
        let url = parse_proxy_address("127.0.0.1:7890").expect("应解析出代理地址");
        assert_eq!(url.as_str(), "http://127.0.0.1:7890/");
    }

    #[test]
    fn proxy_address_keeps_explicit_scheme() {
        let url = parse_proxy_address("http://127.0.0.1:7890").expect("应解析出代理地址");
        assert_eq!(url.as_str(), "http://127.0.0.1:7890/");
    }

    #[test]
    fn proxy_address_prefers_https_entry_in_per_scheme_form() {
        let url = parse_proxy_address("http=10.0.0.1:1080;https=10.0.0.2:1081;ftp=10.0.0.3:1082")
            .expect("应解析出代理地址");
        assert_eq!(url.as_str(), "http://10.0.0.2:1081/");
    }

    #[test]
    fn proxy_address_falls_back_to_http_entry() {
        let url = parse_proxy_address("ftp=10.0.0.3:1082;http=10.0.0.1:1080")
            .expect("应解析出代理地址");
        assert_eq!(url.as_str(), "http://10.0.0.1:1080/");
    }

    #[test]
    fn proxy_address_rejects_blank_and_unusable_values() {
        assert!(parse_proxy_address("").is_none());
        assert!(parse_proxy_address("   ").is_none());
        // 只有无法识别的协议段时不猜测
        assert!(parse_proxy_address("ftp=10.0.0.3:1082").is_none());
    }

    #[test]
    fn reg_value_reads_data_column() {
        let stdout = "\r\nHKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings\r\n    ProxyServer    REG_SZ    127.0.0.1:7890\r\n\r\n";
        assert_eq!(
            parse_reg_value(stdout, "ProxyServer").as_deref(),
            Some("127.0.0.1:7890")
        );
    }

    #[test]
    fn reg_value_reads_dword_and_missing_name() {
        let stdout = "    ProxyEnable    REG_DWORD    0x1\r\n";
        assert_eq!(parse_reg_value(stdout, "ProxyEnable").as_deref(), Some("0x1"));
        assert!(parse_reg_value(stdout, "ProxyServer").is_none());
    }

    #[test]
    fn reg_value_keeps_spaces_inside_data() {
        let stdout = "    ProxyOverride    REG_SZ    <local>;*.cn; 10.0.0.1\r\n";
        assert_eq!(
            parse_reg_value(stdout, "ProxyOverride").as_deref(),
            Some("<local>;*.cn; 10.0.0.1")
        );
    }
}
