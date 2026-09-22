//! 应用自更新（画布 17 · 更新功能）
//!
//! 走 tauri-plugin-updater：更新端点与签名公钥配置在 tauri.conf.json，
//! 发布时用 `tauri signer` 私钥对安装包签名，Release 附 latest.json。
//! 前端只在启动 1 秒后的静默检查与设置页手动检查时调用 `check_app_update_available`；
//! `install_update_and_restart` 下载期间经 `update-download-progress` 事件推送进度，
//! 点击标题栏绿色按钮即直接进入本流程，无二次确认。

use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;

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

/// 只查询是否有新版本，不下载。端点不可达或公钥校验失败时返回 Err，
/// 由前端决定静默（启动检查失败不打扰）或提示（手动检查 toast）。
#[tauri::command]
pub async fn check_app_update_available(app: AppHandle) -> Result<UpdateCheck, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    Ok(match updater.check().await.map_err(|e| e.to_string())? {
        Some(update) => UpdateCheck {
            available: true,
            current_version: update.current_version,
            available_version: update.version,
            notes: update.body,
            pub_date: update.date.map(|d| d.to_string()),
        },
        None => UpdateCheck {
            available: false,
            current_version: app.package_info().version.to_string(),
            available_version: String::new(),
            notes: None,
            pub_date: None,
        },
    })
}

/// 下载 → 校验签名 → 安装 → 重启。
///
/// Windows：NSIS 安装器拉起后当前进程直接退出，新版本由安装器自动重启；
/// 插件 install 内部会硬退出（不走 Drop），因此托盘移除必须前置，
/// 否则安装器接管期间会残留无法自行消失的死图标。
/// macOS / Linux：install 原地替换后正常返回，手动 restart 进新版本。
/// 已是最新时返回 false（前端亮绿灯后一般不会出现）。
#[tauri::command]
pub async fn install_update_and_restart(app: AppHandle) -> Result<bool, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let Some(update) = updater.check().await.map_err(|e| e.to_string())? else {
        return Ok(false);
    };

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

    #[cfg(target_os = "windows")]
    let _ = app.remove_tray_by_id("main-tray");

    update
        .install(bytes)
        .map_err(|e| format!("安装更新失败：{e}"))?;

    #[cfg(not(target_os = "windows"))]
    app.restart();

    Ok(true)
}
