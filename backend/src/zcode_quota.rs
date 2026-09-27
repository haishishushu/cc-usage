//! 读取本机 ZCode 配置中的 BigModel Coding Plan API Key，查询该 Key 的套餐额度。
//! 仅发送到现有 coding_plan 模块的固定官方域名；不读取或上报 ZCode OAuth 凭证。

use crate::coding_plan::{self, PlanProvider};
use crate::quota::QuotaState;
use serde_json::Value;

fn bigmodel_coding_key(config: &Value) -> Option<&str> {
    config.pointer("/config/providerConfigRules/providerRules")?.as_array()?
        .iter()
        .filter(|rule| rule.get("providerId").and_then(Value::as_str) == Some("bigmodel-api")
            && rule.pointer("/config/access/type").and_then(Value::as_str) == Some("zhipu-coding-plan-api-key"))
        .find_map(|rule| rule.pointer("/config/access/apiKey")?.as_str()
            .map(str::trim).filter(|key| !key.is_empty()))
}

pub(crate) fn local_zcode_quota() -> QuotaState {
    let Some(home) = crate::creds::home() else {
        return QuotaState::Unauthorized { reason: "无法定位本机 ZCode 配置目录".into() };
    };
    let path = home.join(".zcode").join("v2").join("provider_config.json");
    let config = match std::fs::read_to_string(path) {
        Ok(config) => config,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return QuotaState::Unauthorized { reason: "未找到本机 ZCode 提供商配置".into() };
        }
        Err(_) => return QuotaState::Failed { reason: "读取 ZCode 提供商配置失败".into() },
    };
    let Ok(config) = serde_json::from_str::<Value>(&config) else {
        return QuotaState::Failed { reason: "ZCode 提供商配置格式无效".into() };
    };
    let Some(key) = bigmodel_coding_key(&config) else {
        return QuotaState::Unsupported {
            reason: "本机 ZCode 未配置 BigModel Coding Plan API Key；账号登录额度尚不能直查".into(),
        };
    };
    coding_plan::query_plan_direct(PlanProvider::ZhipuCn, key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_only_bigmodel_coding_plan_key() {
        let config = serde_json::json!({"config":{"providerConfigRules":{"providerRules":[
            {"providerId":"other","config":{"access":{"type":"zhipu-coding-plan-api-key","apiKey":"wrong"}}},
            {"providerId":"bigmodel-api","config":{"access":{"type":"zhipu-coding-plan-api-key","apiKey":"correct"}}}
        ]}}});
        assert_eq!(bigmodel_coding_key(&config), Some("correct"));
    }

    #[test]
    fn rejects_other_access_modes_and_blank_keys() {
        let oauth = serde_json::json!({"config":{"providerConfigRules":{"providerRules":[
            {"providerId":"bigmodel-api","config":{"access":{"type":"oauth","apiKey":"wrong"}}}
        ]}}});
        let blank = serde_json::json!({"config":{"providerConfigRules":{"providerRules":[
            {"providerId":"bigmodel-api","config":{"access":{"type":"zhipu-coding-plan-api-key","apiKey":" "}}}
        ]}}});
        assert_eq!(bigmodel_coding_key(&oauth), None);
        assert_eq!(bigmodel_coding_key(&blank), None);
    }

    #[test]
    fn skips_blank_matching_rule_before_valid_key() {
        let config = serde_json::json!({"config":{"providerConfigRules":{"providerRules":[
            {"providerId":"bigmodel-api","config":{"access":{"type":"zhipu-coding-plan-api-key","apiKey":" "}}},
            {"providerId":"bigmodel-api","config":{"access":{"type":"zhipu-coding-plan-api-key","apiKey":"valid"}}}
        ]}}});
        assert_eq!(bigmodel_coding_key(&config), Some("valid"));
    }

    #[test]
    #[ignore = "需要本机 ZCode BigModel Key 与网络；仅显式执行"]
    fn local_key_can_query_remaining_credits() {
        let state = local_zcode_quota();
        assert!(matches!(state, QuotaState::Ok { ref windows, .. }
            if windows.iter().any(|window| window.remaining_text.is_some())),
            "本机 Key 未能取得精确剩余积分：{state:?}");
    }
}
