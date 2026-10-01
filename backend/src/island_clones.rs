//! 灵动岛分身（画布 22）
//!
//! 本体继续沿用 [`Settings`] 里的 `island_*` 与 `dock` 字段，旧配置零迁移；
//! 每个分身在 `island_clones` 里单独一条记录。这里只做**纯数据**判定：
//! 创建、销毁、接任、按窗口标签找岛，窗口的创建与落位由 `lib.rs` 完成。

use serde::{Deserialize, Serialize};

use crate::dock::DockState;
use crate::settings::Settings;

/// 含本体在内的灵动岛总数上限，防止误操作开出一屏窗口。
pub const MAX_ISLANDS: usize = 8;
/// 本体窗口标签，与 tauri.conf 一致。
pub const PRIMARY_LABEL: &str = "island";
const CLONE_PREFIX: &str = "island-";

/// 一个分身的持久化配置。字段与本体的 `island_*` 一一对应。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct IslandClone {
    pub id: String,
    pub platform: String,
    pub kind: String,
    pub connection_id: Option<String>,
    pub connection_name: Option<String>,
    pub source_id: Option<String>,
    pub dock: DockState,
    /// 自由态窗口左上角的物理像素位置；None 表示尚未记录，重建时错开本体。
    pub position: Option<(i32, i32)>,
}

/// 本体与分身统一成同一形状，供命令、托盘与前端使用。
#[derive(Debug, Clone, PartialEq)]
pub struct IslandProfile {
    pub platform: String,
    pub kind: String,
    pub connection_id: Option<String>,
    pub connection_name: Option<String>,
    pub source_id: Option<String>,
    pub dock: DockState,
}

/// 选中连接后要写回的字段；None 表示清空选择。
#[derive(Debug, Clone, PartialEq)]
pub struct SelectedConnection {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub kind: String,
    pub source_id: String,
}

/// 销毁后 `lib.rs` 需要执行的窗口动作。
#[derive(Debug, Clone, PartialEq)]
pub enum DestroyPlan {
    /// 关闭该分身窗口即可。
    CloseClone(String),
    /// 本体被销毁：关闭接任的分身窗口，本体窗口按接任者的停靠 / 位置落位。
    PromoteClone { closed: IslandClone },
}

/// `None` 代表本体。
pub fn label(id: Option<&str>) -> String {
    match id {
        Some(id) => format!("{CLONE_PREFIX}{id}"),
        None => PRIMARY_LABEL.to_string(),
    }
}

/// 外层 `None`：不是灵动岛窗口；`Some(None)`：本体；`Some(Some(id))`：分身。
pub fn id_from_label(label: &str) -> Option<Option<String>> {
    if label == PRIMARY_LABEL { return Some(None); }
    label.strip_prefix(CLONE_PREFIX).filter(|id| !id.is_empty()).map(|id| Some(id.to_string()))
}

pub fn is_island_label(label: &str) -> bool { id_from_label(label).is_some() }

pub fn count(settings: &Settings) -> usize { 1 + settings.island_clones.len() }

fn clone_index(settings: &Settings, id: &str) -> Option<usize> {
    settings.island_clones.iter().position(|clone| clone.id == id)
}

pub fn profile(settings: &Settings, id: Option<&str>) -> Option<IslandProfile> {
    match id {
        None => Some(IslandProfile {
            platform: settings.island_platform.clone(),
            kind: settings.island_kind.clone(),
            connection_id: settings.island_connection_id.clone(),
            connection_name: settings.island_connection_name.clone(),
            source_id: settings.island_source_id.clone(),
            dock: settings.dock.clone(),
        }),
        Some(id) => settings.island_clones.iter().find(|clone| clone.id == id).map(|clone| IslandProfile {
            platform: clone.platform.clone(),
            kind: clone.kind.clone(),
            connection_id: clone.connection_id.clone(),
            connection_name: clone.connection_name.clone(),
            source_id: clone.source_id.clone(),
            dock: clone.dock.clone(),
        }),
    }
}

/// 返回 false 表示目标岛不存在，调用方不应发出变更事件。
pub fn set_dock(settings: &mut Settings, id: Option<&str>, dock: DockState) -> bool {
    match id {
        None => { settings.dock = dock; true }
        Some(id) => match clone_index(settings, id) {
            Some(index) => { settings.island_clones[index].dock = dock; true }
            None => false,
        },
    }
}

pub fn set_position(settings: &mut Settings, id: &str, position: (i32, i32)) -> bool {
    match clone_index(settings, id) {
        Some(index) if settings.island_clones[index].position != Some(position) => {
            settings.island_clones[index].position = Some(position);
            true
        }
        _ => false,
    }
}

/// 与本体 `set_island_connection` 同一套写回规则。
pub fn apply_connection(settings: &mut Settings, id: Option<&str>, selected: Option<&SelectedConnection>) -> bool {
    match id {
        None => {
            match selected {
                Some(c) => {
                    settings.island_connection_id = Some(c.id.clone());
                    settings.island_connection_name = Some(c.name.clone());
                    settings.island_platform = c.platform.clone();
                    settings.island_kind = c.kind.clone();
                    settings.island_source_id = Some(c.source_id.clone());
                }
                None => {
                    settings.island_connection_id = None;
                    settings.island_connection_name = None;
                }
            }
            true
        }
        Some(id) => {
            let Some(index) = clone_index(settings, id) else { return false };
            let clone = &mut settings.island_clones[index];
            match selected {
                Some(c) => {
                    clone.connection_id = Some(c.id.clone());
                    clone.connection_name = Some(c.name.clone());
                    clone.platform = c.platform.clone();
                    clone.kind = c.kind.clone();
                    clone.source_id = Some(c.source_id.clone());
                }
                None => {
                    clone.connection_id = None;
                    clone.connection_name = None;
                }
            }
            true
        }
    }
}

/// 连接改名后同步本体与所有分身的快照名；返回是否有改动。
pub fn rename_connection(settings: &mut Settings, connection_id: &str, name: &str) -> bool {
    let mut changed = false;
    if settings.island_connection_id.as_deref() == Some(connection_id)
        && settings.island_connection_name.as_deref() != Some(name) {
        settings.island_connection_name = Some(name.to_string());
        changed = true;
    }
    for clone in &mut settings.island_clones {
        if clone.connection_id.as_deref() == Some(connection_id)
            && clone.connection_name.as_deref() != Some(name) {
            clone.connection_name = Some(name.to_string());
            changed = true;
        }
    }
    changed
}

fn fresh_id(settings: &Settings) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut candidate = format!("{:x}", nanos & 0xffff_ffff_ffff);
    let mut salt = 0u32;
    while settings.island_clones.iter().any(|clone| clone.id == candidate) {
        salt += 1;
        candidate = format!("{:x}{salt:x}", nanos & 0xffff_ffff_ffff);
    }
    candidate
}

/// 以来源岛的连接为初始值新建分身；停靠与位置留空，由窗口创建时错开落位。
pub fn create_clone(settings: &mut Settings, source: Option<&str>) -> Result<IslandClone, String> {
    if count(settings) >= MAX_ISLANDS {
        return Err(format!("灵动岛最多 {MAX_ISLANDS} 个"));
    }
    let source = profile(settings, source).ok_or("来源灵动岛不存在")?;
    let clone = IslandClone {
        id: fresh_id(settings),
        platform: source.platform,
        kind: source.kind,
        connection_id: source.connection_id,
        connection_name: source.connection_name,
        source_id: source.source_id,
        dock: DockState::default(),
        position: None,
    };
    settings.island_clones.push(clone.clone());
    Ok(clone)
}

/// 销毁目标岛。总数为 1 时拒绝；销毁本体时第一个分身的配置接任本体。
pub fn destroy(settings: &mut Settings, target: Option<&str>) -> Result<DestroyPlan, String> {
    if count(settings) <= 1 {
        return Err("至少保留一个灵动岛".into());
    }
    match target {
        Some(id) => {
            let index = clone_index(settings, id).ok_or("要销毁的分身不存在")?;
            settings.island_clones.remove(index);
            Ok(DestroyPlan::CloseClone(id.to_string()))
        }
        None => {
            let promoted = settings.island_clones.remove(0);
            settings.island_platform = promoted.platform.clone();
            settings.island_kind = promoted.kind.clone();
            settings.island_connection_id = promoted.connection_id.clone();
            settings.island_connection_name = promoted.connection_name.clone();
            settings.island_source_id = promoted.source_id.clone();
            settings.dock = promoted.dock.clone();
            Ok(DestroyPlan::PromoteClone { closed: promoted })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dock::Edge;

    fn selected(id: &str) -> SelectedConnection {
        SelectedConnection { id: id.into(), name: format!("{id} 名"), platform: "codex".into(), kind: "auth".into(), source_id: "local:codex".into() }
    }

    #[test]
    fn labels_round_trip_between_primary_and_clones() {
        assert_eq!(label(None), "island");
        assert_eq!(label(Some("ab12")), "island-ab12");
        assert_eq!(id_from_label("island"), Some(None));
        assert_eq!(id_from_label("island-ab12"), Some(Some("ab12".into())));
        assert_eq!(id_from_label("island-"), None);
        assert_eq!(id_from_label("main"), None);
        assert!(is_island_label("island-x") && !is_island_label("context-menu"));
    }

    #[test]
    fn clone_copies_source_connection_and_respects_limit() {
        let mut s = Settings::default();
        s.island_connection_id = Some("c1".into());
        s.island_connection_name = Some("官方订阅".into());
        s.dock = DockState { edge: Some(Edge::Top), offset: 400.0, monitor: None };
        let first = create_clone(&mut s, None).unwrap();
        assert_eq!(first.connection_id.as_deref(), Some("c1"));
        assert_eq!(first.dock.edge, None, "分身不继承本体的停靠");
        assert_eq!(count(&s), 2);
        apply_connection(&mut s, Some(&first.id), Some(&selected("c2")));
        let second = create_clone(&mut s, Some(&first.id)).unwrap();
        assert_eq!(second.connection_id.as_deref(), Some("c2"));
        assert_eq!(second.platform, "codex");
        assert_ne!(first.id, second.id);
        while count(&s) < MAX_ISLANDS { create_clone(&mut s, None).unwrap(); }
        assert!(create_clone(&mut s, None).is_err());
        assert_eq!(count(&s), MAX_ISLANDS);
    }

    #[test]
    fn destroying_keeps_at_least_one_and_promotes_first_clone() {
        let mut s = Settings::default();
        assert!(destroy(&mut s, None).is_err());
        let a = create_clone(&mut s, None).unwrap();
        apply_connection(&mut s, Some(&a.id), Some(&selected("codex-work")));
        set_dock(&mut s, Some(&a.id), DockState { edge: Some(Edge::Right), offset: 300.0, monitor: Some("M2".into()) });
        let b = create_clone(&mut s, None).unwrap();
        assert_eq!(destroy(&mut s, Some(&b.id)).unwrap(), DestroyPlan::CloseClone(b.id.clone()));
        assert!(destroy(&mut s, Some("missing")).is_err());
        let plan = destroy(&mut s, None).unwrap();
        assert!(matches!(plan, DestroyPlan::PromoteClone { ref closed } if closed.id == a.id));
        assert_eq!(s.island_connection_id.as_deref(), Some("codex-work"));
        assert_eq!(s.island_platform, "codex");
        assert_eq!(s.dock.edge, Some(Edge::Right));
        assert_eq!(s.dock.monitor.as_deref(), Some("M2"));
        assert_eq!(count(&s), 1);
        assert!(destroy(&mut s, None).is_err());
    }

    #[test]
    fn profile_dock_position_and_rename_apply_to_the_right_island() {
        let mut s = Settings::default();
        s.island_connection_id = Some("c1".into());
        let a = create_clone(&mut s, None).unwrap();
        assert_eq!(profile(&s, Some("nope")), None);
        assert!(set_dock(&mut s, Some(&a.id), DockState { edge: Some(Edge::Left), offset: 10.0, monitor: None }));
        assert!(!set_dock(&mut s, Some("nope"), DockState::default()));
        assert_eq!(profile(&s, Some(&a.id)).unwrap().dock.edge, Some(Edge::Left));
        assert_eq!(profile(&s, None).unwrap().dock.edge, None);
        assert!(set_position(&mut s, &a.id, (10, 20)));
        assert!(!set_position(&mut s, &a.id, (10, 20)), "相同位置不算改动");
        assert!(rename_connection(&mut s, "c1", "新名字"));
        assert_eq!(s.island_connection_name.as_deref(), Some("新名字"));
        assert_eq!(s.island_clones[0].connection_name.as_deref(), Some("新名字"));
        assert!(!rename_connection(&mut s, "c1", "新名字"));
        apply_connection(&mut s, None, None);
        assert_eq!(s.island_connection_id, None);
        assert_eq!(s.island_clones[0].connection_id.as_deref(), Some("c1"), "分身不受本体清空影响");
        apply_connection(&mut s, Some(&a.id), None);
        assert_eq!(s.island_clones[0].connection_id, None);
    }

    #[test]
    fn old_settings_without_clones_still_load() {
        let s: Settings = serde_json::from_str(r#"{"island_platform":"codex"}"#).unwrap();
        assert!(s.island_clones.is_empty());
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.island_clones, s.island_clones);
    }
}
