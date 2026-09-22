//! 启用连接 = 写入 CLI 配置（画布 18 后续需求，对齐 cc-switch 的切换作用）。
//!
//! 三种来源的连接（官方订阅 / API 本机读取 / API 手动填写）启用时，
//! 都把连接保存的凭证与地址写回对应 CLI 的配置文件，使新会话即刻生效：
//! - Claude API  ：`~/.claude/settings.json` 的 `env.ANTHROPIC_AUTH_TOKEN`（+ `ANTHROPIC_BASE_URL`）
//! - Claude Auth ：仅选择当前完整授权，拒绝拼接不同账号的 token
//! - Codex  API  ：`~/.codex/auth.json` 的 `OPENAI_API_KEY`（+ config.toml 当前 provider 的 base_url）
//! - Codex  Auth ：保留当前完整授权，清除 API 模式覆盖
//!
//! 写入路径与 `creds::read_secret` 的读取路径一一对应；每次写文件前先把原文件
//! 备份到应用数据目录，写失败不留半截内容（write_atomic 风格）。

use std::path::{Path, PathBuf};

/// 写回结果说明（给 Toast 的补充文案），不含任何凭证内容。
pub struct ApplyNote {
    pub message: String,
}

pub fn apply_to_cli(
    platform: &str,
    kind: &str,
    secret: &str,
    base_url: Option<&str>,
    backup_dir: &Path,
) -> Result<ApplyNote, String> {
    match (platform, kind) {
        ("claude", "api") => apply_claude_api(secret, base_url, backup_dir),
        ("claude", _) => apply_claude_auth(secret, backup_dir),
        ("codex", "api") => apply_codex_api(secret, base_url, backup_dir),
        ("codex", _) => apply_codex_auth(secret, backup_dir),
        _ => Err(format!("暂不支持把 {platform} 连接写入 CLI 配置")),
    }
}

/* ─────────────────────────── Claude ─────────────────────────── */

fn claude_settings_path() -> Result<PathBuf, String> {
    claude_home().map(|dir| dir.join("settings.json"))
}

fn claude_credentials_path() -> Result<PathBuf, String> {
    claude_home().map(|dir| dir.join(".credentials.json"))
}

/// settings.json 的 env 段写入：token 必写；base_url 有则写、无则删（官方 API 直连）。
fn apply_claude_api(
    secret: &str,
    base_url: Option<&str>,
    backup_dir: &Path,
) -> Result<ApplyNote, String> {
    let path = claude_settings_path()?;
    update_json(&path, backup_dir, |v| {
        let env = v
            .as_object_mut()
            .ok_or_else(|| "settings.json 顶层不是对象".to_string())?
            .entry("env")
            .or_insert_with(|| serde_json::Value::Object(Default::default()));
        let env = env
            .as_object_mut()
            .ok_or_else(|| "settings.json 的 env 段不是对象".to_string())?;
        env.remove("ANTHROPIC_API_KEY");
        env.insert(
            "ANTHROPIC_AUTH_TOKEN".into(),
            serde_json::Value::String(secret.to_string()),
        );
        match base_url {
            Some(url) => {
                env.insert(
                    "ANTHROPIC_BASE_URL".into(),
                    serde_json::Value::String(url.to_string()),
                );
            }
            None => {
                env.remove("ANTHROPIC_BASE_URL");
            }
        }
        Ok(())
    })?;
    let where_url = match base_url {
        Some(url) => format!("、Base URL {url}"),
        None => "（官方直连，已移除自定义 Base URL）".to_string(),
    };
    Ok(ApplyNote {
        message: format!("已写入 ~/.claude/settings.json：AUTH_TOKEN{where_url}，新会话生效"),
    })
}

/// OAuth 必须保留同一身份的完整凭证，不能用保存的 accessToken 覆盖另一账号。
fn apply_claude_auth(secret: &str, backup_dir: &Path) -> Result<ApplyNote, String> {
    verify_current_auth(
        &claude_credentials_path()?,
        "/claudeAiOauth/accessToken",
        secret,
    )?;
    let path = claude_settings_path()?;
    if path.exists() {
        update_json(&path, backup_dir, |v| {
            let object = v.as_object_mut().ok_or("settings.json 不是对象")?;
            object.remove("apiKeyHelper");
            if let Some(env) = object.get_mut("env").and_then(|v| v.as_object_mut()) {
                for key in [
                    "ANTHROPIC_API_KEY",
                    "ANTHROPIC_AUTH_TOKEN",
                    "ANTHROPIC_BASE_URL",
                ] {
                    env.remove(key);
                }
            }
            Ok(())
        })?;
    }
    Ok(ApplyNote {
        message: "已选择当前本机完整授权，并清除配置中的 API 覆盖；终端环境变量仍由终端管理".into(),
    })
}

/* ─────────────────────────── Codex ─────────────────────────── */

fn codex_auth_path() -> Result<PathBuf, String> {
    codex_home().map(|dir| dir.join("auth.json"))
}

fn codex_config_path() -> Result<PathBuf, String> {
    codex_home().map(|dir| dir.join("config.toml"))
}

fn apply_codex_api(
    secret: &str,
    base_url: Option<&str>,
    backup_dir: &Path,
) -> Result<ApplyNote, String> {
    let auth = codex_auth_path()?;
    let config = codex_config_path()?;
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&auth).map_err(|e| e.to_string())?)
            .map_err(|_| "auth.json 无效")?;
    let object = value.as_object_mut().ok_or("auth.json 不是对象")?;
    object.remove("tokens");
    object.remove("last_refresh");
    object.remove("auth_mode");
    object.insert(
        "OPENAI_API_KEY".into(),
        serde_json::Value::String(secret.into()),
    );
    let document = codex_provider_config(
        &std::fs::read_to_string(&config).map_err(|e| e.to_string())?,
        base_url,
    )?;
    write_pair(
        &auth,
        serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?,
        &config,
        document.into_bytes(),
        backup_dir,
    )?;
    Ok(ApplyNote {
        message: "Key 与服务地址已一起写入 Codex 配置，新会话生效；原配置已备份".into(),
    })
}

fn apply_codex_auth(secret: &str, backup_dir: &Path) -> Result<ApplyNote, String> {
    let auth = codex_auth_path()?;
    let config = codex_config_path()?;
    verify_current_auth(&auth, "/tokens/access_token", secret)?;
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&auth).map_err(|e| e.to_string())?)
            .map_err(|_| "auth.json 无效")?;
    let object = value.as_object_mut().ok_or("auth.json 不是对象")?;
    object.remove("OPENAI_API_KEY");
    object.remove("auth_mode");
    let document = codex_provider_config(
        &std::fs::read_to_string(&config).map_err(|e| e.to_string())?,
        None,
    )?;
    write_pair(
        &auth,
        serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?,
        &config,
        document.into_bytes(),
        backup_dir,
    )?;
    Ok(ApplyNote {
        message: "已选择当前本机完整授权及官方服务；未拼接不同账号的 access/refresh token".into(),
    })
}

fn verify_current_auth(path: &Path, pointer: &str, secret: &str) -> Result<(), String> {
    let value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|_| "授权文件格式无效")?;
    if secret.is_empty() || value.pointer(pointer).and_then(|v| v.as_str()) != Some(secret) {
        return Err("保存的授权与当前 CLI 登录不同；请在对应 CLI 登录目标账号，再重新获取完整授权。不能仅替换 access token".into());
    }
    Ok(())
}

fn codex_provider_config(text: &str, url: Option<&str>) -> Result<String, String> {
    let mut document = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|_| "config.toml 格式无效")?;
    if let Some(url) = url {
        let mut provider = toml_edit::Table::new();
        provider["name"] = toml_edit::value("CC Usage");
        provider["base_url"] = toml_edit::value(url);
        provider["wire_api"] = toml_edit::value("responses");
        provider["requires_openai_auth"] = toml_edit::value(true);
        if document.get("model_providers").is_none() {
            document["model_providers"] = toml_edit::Item::Table(toml_edit::Table::new());
        }
        let providers = document
            .get_mut("model_providers")
            .and_then(|v| v.as_table_like_mut())
            .ok_or("model_providers 格式无效")?;
        providers.insert("cc-usage", toml_edit::Item::Table(provider));
        document["model_provider"] = toml_edit::value("cc-usage");
    } else {
        document["model_provider"] = toml_edit::value("openai");
    }
    Ok(document.to_string())
}

/// 先验证并备份全部文件；第二个写入失败时恢复第一个，不能返回半成功。
fn write_pair(
    first: &Path,
    first_bytes: Vec<u8>,
    second: &Path,
    second_bytes: Vec<u8>,
    backup_dir: &Path,
) -> Result<(), String> {
    write_pair_using(
        first,
        first_bytes,
        second,
        second_bytes,
        backup_dir,
        write_atomic,
    )
}
fn write_pair_using(
    first: &Path,
    first_bytes: Vec<u8>,
    second: &Path,
    second_bytes: Vec<u8>,
    backup_dir: &Path,
    mut write: impl FnMut(&Path, Vec<u8>) -> Result<(), String>,
) -> Result<(), String> {
    let old_first = std::fs::read(first).map_err(|e| e.to_string())?;
    let old_second = std::fs::read(second).map_err(|e| e.to_string())?;
    backup_file(first, backup_dir)?;
    backup_file(second, backup_dir)?;
    if std::fs::read(first).ok().as_ref() != Some(&old_first)
        || std::fs::read(second).ok().as_ref() != Some(&old_second)
    {
        return Err("配置在准备期间发生变化，请重新读取后再启用".into());
    }
    write(first, first_bytes.clone())?;
    if let Err(error) = write(second, second_bytes) {
        if std::fs::read(first).ok().as_ref() != Some(&first_bytes) {
            return Err(format!(
                "{error}；首个文件被外部修改，无法自动回滚，请从备份恢复"
            ));
        }
        write(first, old_first).map_err(|rollback| format!("{error}；回滚失败：{rollback}"))?;
        return Err(format!("{error}；已回滚首个文件"));
    }
    Ok(())
}

/* ─────────────────────────── 通用工具 ─────────────────────────── */

fn claude_home() -> Result<PathBuf, String> {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .map(Ok)
        .unwrap_or_else(|| dirs_home().map(|p| p.join(".claude")))
}
fn codex_home() -> Result<PathBuf, String> {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .map(Ok)
        .unwrap_or_else(|| dirs_home().map(|p| p.join(".codex")))
}
fn dirs_home() -> Result<PathBuf, String> {
    crate::creds::home().ok_or_else(|| "未找到用户主目录".to_string())
}

/// 读 JSON → 修改 → 备份 → 原子写。文件不存在时如实报错，不代用户创建。
fn update_json(
    path: &Path,
    backup_dir: &Path,
    edit: impl FnOnce(&mut serde_json::Value) -> Result<(), String>,
) -> Result<(), String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("读取 {} 失败：{e}", path.display()))?;
    let mut value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("{} 不是有效 JSON：{e}", path.display()))?;
    edit(&mut value)?;
    backup_file(path, backup_dir)?;
    let bytes = serde_json::to_vec_pretty(&value)
        .map_err(|e| format!("{} 序列化失败：{e}", path.display()))?;
    write_atomic(path, bytes)
}

/// 写前备份：原样复制到应用数据目录的 cli-backups/，文件名带毫秒时间戳。
fn backup_file(path: &Path, backup_dir: &Path) -> Result<(), String> {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return Err("备份失败：文件名无法识别".into());
    };
    std::fs::create_dir_all(backup_dir).map_err(|e| format!("创建备份目录失败：{e}"))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let target = backup_dir.join(format!("{name}.{stamp}.bak"));
    std::fs::copy(path, target).map_err(|e| format!("备份 {} 失败：{e}", path.display()))?;
    Ok(())
}

fn write_atomic(path: &Path, bytes: Vec<u8>) -> Result<(), String> {
    use std::io::Write;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let temporary = path.with_extension(format!("ccusage-{}-{nonce}.tmp", std::process::id()));
    let write = || -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)
    };
    write().map_err(|error| {
        let _ = std::fs::remove_file(&temporary);
        format!("写入 {} 失败：{error}", path.display())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn codex_switch_clears_selected_gateway_and_keeps_unrelated_configuration() {
        let original="model = 'test-model'\nmodel_provider = 'old'\n[model_providers.old]\nbase_url = 'https://old.example/v1'\n";
        let gateway = codex_provider_config(original, Some("https://new.example/v1")).unwrap();
        let parsed = gateway.parse::<toml_edit::DocumentMut>().unwrap();
        assert_eq!(parsed["model_provider"].as_str(), Some("cc-usage"));
        assert_eq!(
            parsed["model_providers"]["cc-usage"]["base_url"].as_str(),
            Some("https://new.example/v1")
        );
        assert_eq!(
            parsed["model_providers"]["cc-usage"]["requires_openai_auth"].as_bool(),
            Some(true)
        );
        let official = codex_provider_config(&gateway, None)
            .unwrap()
            .parse::<toml_edit::DocumentMut>()
            .unwrap();
        assert_eq!(official["model_provider"].as_str(), Some("openai"));
        assert_eq!(official["model"].as_str(), Some("test-model"));
    }
    #[test]
    fn failed_second_write_restores_first_file() {
        let root = std::env::temp_dir().join(format!(
            "ai-usage-pair-test-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let a = root.join("auth.json");
        let b = root.join("config.toml");
        std::fs::write(&a, b"old-auth").unwrap();
        std::fs::write(&b, b"old-config").unwrap();
        let error = write_pair_using(
            &a,
            b"new-auth".to_vec(),
            &b,
            b"new-config".to_vec(),
            &root.join("backups"),
            |path, bytes| {
                if path == b {
                    Err("injected second write failure".into())
                } else {
                    write_atomic(path, bytes)
                }
            },
        )
        .unwrap_err();
        assert!(error.contains("已回滚"));
        assert_eq!(std::fs::read(&a).unwrap(), b"old-auth");
        assert_eq!(std::fs::read(&b).unwrap(), b"old-config");
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn mismatched_oauth_is_rejected_without_replacing_refresh_token() {
        let path = std::env::temp_dir().join(format!(
            "ai-usage-oauth-test-{}-{}.json",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let value = br#"{"tokens":{"access_token":"account-a","refresh_token":"refresh-a"}}"#;
        std::fs::write(&path, value).unwrap();
        assert!(verify_current_auth(&path, "/tokens/access_token", "account-b").is_err());
        assert!(verify_current_auth(&path, "/tokens/access_token", "account-a").is_ok());
        assert_eq!(std::fs::read(&path).unwrap(), value);
        std::fs::remove_file(path).unwrap();
    }
}
