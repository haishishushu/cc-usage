//! 前后端共用的平台目录。来源是否存在由本机检测决定，不能由图标推断。
use serde::Deserialize;
use std::sync::LazyLock;

#[derive(Deserialize)]
pub struct Platform {
    pub id: String,
    pub name: String,
    pub capabilities: Vec<String>,
    pub limitation: Option<String>,
}

static CATALOG: LazyLock<Vec<Platform>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../frontend/src/lib/platformCatalog.json"))
        .expect("随程序打包的平台目录必须有效")
});

pub fn get(id: &str) -> Option<&'static Platform> {
    CATALOG.iter().find(|p| p.id == id)
}
pub fn known(id: &str) -> bool {
    get(id).is_some()
}
pub fn native(id: &str) -> bool {
    known(id) && !matches!(id, "claude" | "codex")
}
pub fn supports(id: &str, capability: &str) -> bool {
    get(id).is_some_and(|p| p.capabilities.iter().any(|c| c == capability))
}
pub fn limitation(id: &str) -> String {
    get(id)
        .and_then(|p| p.limitation.clone())
        .unwrap_or_else(|| "该平台未提供此项数据".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_eight_platforms_have_unique_ids() {
        assert_eq!(CATALOG.len(), 8);
        for id in [
            "claude",
            "codex",
            "gemini",
            "grok",
            "zcode",
            "trae",
            "qoder",
            "workbuddy",
        ] {
            assert_eq!(CATALOG.iter().filter(|p| p.id == id).count(), 1);
        }
        assert!(!known("tare"));
        assert!(!supports("trae", "local_sessions"));
    }
}
