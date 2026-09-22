//! 官方 Key 的只读验证，独立于本机监控和消费版 OAuth。
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn blocked_or_incomplete_xai_response_is_not_valid() {
        let good = json!({"api_key_id":"synthetic", "api_key_blocked":false,"api_key_disabled":false,"team_blocked":false});
        assert!(validate_body("grok", &good).is_ok());
        for field in ["api_key_blocked", "api_key_disabled", "team_blocked"] {
            let mut blocked = good.clone();
            blocked[field] = json!(true);
            assert!(validate_body("grok", &blocked).is_err());
            let mut missing = good.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(validate_body("grok", &missing).is_err());
        }
        assert!(validate_body("gemini", &json!({"data":[{"id":"gemini-test"}]})).is_ok());
        assert!(validate_body("gemini", &json!({"data":[]})).is_err());
        assert!(validate_body("gemini", &json!({"error":"bad key"})).is_err());
    }
    #[test]
    fn official_only_never_sends_a_key_to_arbitrary_base() {
        assert!(check_base("grok", Some("https://api.x.ai/v1")).is_ok());
        assert!(check_base(
            "gemini",
            Some("https://generativelanguage.googleapis.com/v1beta/openai/")
        )
        .is_ok());
        for bad in [
            "http://api.x.ai",
            "https://api.x.ai.evil.test",
            "https://api.x.ai:444",
            "https://user@api.x.ai",
            "https://api.x.ai/other",
            "https://api.x.ai?key=test",
        ] {
            assert!(check_base("grok", Some(bad)).is_err());
        }
    }
}

pub fn supported(platform: &str) -> bool {
    matches!(platform, "gemini" | "grok")
}
pub fn check_base(platform: &str, base: Option<&str>) -> Result<(), String> {
    let (host, paths) = match platform {
        "gemini" => (
            "generativelanguage.googleapis.com",
            &["", "/v1beta", "/v1beta/openai"][..],
        ),
        "grok" => ("api.x.ai", &["", "/v1"][..]),
        _ => return Err("该平台尚未支持官方 Key 检测".into()),
    };
    let Some(base) = base.filter(|s| !s.trim().is_empty()) else {
        return Ok(());
    };
    let url = reqwest::Url::parse(base).map_err(|_| "官方地址格式无效")?;
    if url.scheme() != "https"
        || url.host_str() != Some(host)
        || url.port_or_known_default() != Some(443)
        || !paths.contains(&url.path().trim_end_matches('/'))
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("此平台的 Key 检测仅支持官方 HTTPS 地址".into());
    }
    Ok(())
}
fn validate_body(platform: &str, value: &serde_json::Value) -> Result<(), String> {
    if platform == "grok" {
        if value
            .get("api_key_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .is_none()
        {
            return Err("xAI 未返回可识别的 Key 信息".into());
        }
        for field in ["api_key_blocked", "api_key_disabled", "team_blocked"] {
            match value.get(field).and_then(|v| v.as_bool()) {
                Some(false) => (),
                Some(true) => return Err("xAI Key 或所属团队已停用／受限".into()),
                None => return Err("xAI 响应缺少 Key 状态，无法确认可用性".into()),
            }
        }
        return Ok(());
    }
    if platform == "gemini"
        && value
            .get("data")
            .and_then(|v| v.as_array())
            .is_some_and(|items| {
                items.iter().any(|v| {
                    v.get("id")
                        .and_then(|v| v.as_str())
                        .is_some_and(|s| !s.is_empty())
                })
            })
    {
        Ok(())
    } else {
        Err("官方接口未返回可识别的模型列表".into())
    }
}
pub fn validate(platform: &str, secret: &str, base: Option<&str>) -> Result<(), String> {
    check_base(platform, base)?;
    if secret.trim().is_empty() {
        return Err("请填写 API Key".into());
    }
    let endpoint = if platform == "grok" {
        "https://api.x.ai/v1/api-key"
    } else {
        "https://generativelanguage.googleapis.com/v1beta/openai/models"
    };
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "无法建立验证请求")?;
    let response = client
        .get(endpoint)
        .bearer_auth(secret)
        .send()
        .map_err(|_| "官方 Key 检测连接失败，请检查网络")?;
    if !response.status().is_success() {
        return Err(match response.status().as_u16() {
            401 => "官方接口拒绝此 Key，请检查凭证".into(),
            403 => "当前 Key 无权访问此官方接口".into(),
            429 => "官方接口限流，请稍后重试".into(),
            status => format!("官方 Key 检测失败：HTTP {status}"),
        });
    }
    let body = response.json().map_err(|_| "官方接口响应不是有效 JSON")?;
    validate_body(platform, &body)
}
