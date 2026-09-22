use std::{fs, io::{Read, Write}, path::Path};
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;

fn read_backup(path: &Path) -> Result<String, String> {
    const LIMIT: u64 = 100 * 1024 * 1024;
    let file = fs::File::open(path).map_err(|e| format!("无法读取备份：{e}"))?;
    let mut bytes = Vec::new();
    file.take(LIMIT + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > LIMIT { return Err("备份超过 100 MB，请拆分后导入".into()); }
    let text = String::from_utf8(bytes).map_err(|_| "备份必须使用 UTF-8 编码")?;
    Ok(text.trim_start_matches('\u{feff}').to_string())
}

fn write_backup(path: &Path, json: &str) -> Result<(), String> {
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
    let tmp = path.with_file_name(format!(".ai-usage-export-{}-{nonce}.tmp", std::process::id()));
    let mut file = fs::OpenOptions::new().create_new(true).write(true).open(&tmp).map_err(|e| e.to_string())?;
    let result = (|| {
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)
    })();
    if result.is_err() { let _ = fs::remove_file(&tmp); }
    result.map_err(|e: std::io::Error| format!("保存失败：{e}"))
}

#[tauri::command]
pub async fn import_data_file(app: tauri::AppHandle, db: State<'_, super::Db>) -> Result<Option<super::db::ImportSummary>, String> {
    let window = app.get_webview_window(super::MAIN).ok_or("请从主面板导入数据")?;
    let dialog = app.dialog().file().set_parent(&window).set_title("导入统计备份").add_filter("JSON 备份", &["json"]);
    let selected = tauri::async_runtime::spawn_blocking(move || -> Result<Option<String>, String> {
        let Some(file) = dialog.blocking_pick_file() else { return Ok(None); };
        read_backup(&file.into_path().map_err(|e| e.to_string())?).map(Some)
    }).await.map_err(|e| e.to_string())??;
    match selected {
        None => Ok(None),
        Some(json) => super::import_data(app, db, json).await.map(Some),
    }
}

#[tauri::command]
pub async fn export_data_file(app: tauri::AppHandle, db: State<'_, super::Db>, platform: Option<String>) -> Result<Option<String>, String> {
    if platform.as_deref().is_some_and(|p| !matches!(p, "claude" | "codex")) { return Err("导出范围无效".into()); }
    let window = app.get_webview_window(super::MAIN).ok_or("请从主面板导出数据")?;
    let name = format!("cc-usage-{}-{}.json", platform.as_deref().unwrap_or("all"), chrono::Local::now().format("%Y-%m-%d"));
    let dialog = app.dialog().file().set_parent(&window).set_title("导出统计备份").add_filter("JSON 备份", &["json"]).set_file_name(name);
    let selected = tauri::async_runtime::spawn_blocking(move || dialog.blocking_save_file()).await.map_err(|e| e.to_string())?;
    let Some(file) = selected else { return Ok(None); };
    let path = file.into_path().map_err(|e| e.to_string())?;
    let json = super::export_data(db, platform).await?;
    tauri::async_runtime::spawn_blocking(move || {
        write_backup(&path, &json)?;
        Ok(Some(path.display().to_string()))
    }).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_roundtrip_replaces_existing_backup_and_accepts_utf8_bom() {
        let dir = std::env::temp_dir().join(format!("island-backup-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("backup.json");
        fs::write(&path, "old").unwrap();
        write_backup(&path, "\u{feff}{\"name\":\"统计\"}").unwrap();
        assert_eq!(read_backup(&path).unwrap(), "{\"name\":\"统计\"}");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_file(path).unwrap();
        fs::remove_dir(dir).unwrap();
    }
}
