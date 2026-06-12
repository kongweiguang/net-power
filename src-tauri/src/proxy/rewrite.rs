//! @author kongweiguang
//! HTTP header 和 body 改写策略。该模块只处理纯数据转换，便于单元测试覆盖。

use crate::error::{AppError, AppResult};
use crate::models::{BodyRewriteRule, HeaderRule};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use url::form_urlencoded;

/// 简化后的 header 集合。小写 key 用于匹配，原始写出统一由代理层生成。
pub type HeaderMap = BTreeMap<String, String>;

/// 应用启用的 Header 规则。
pub fn apply_header_rules(headers: &mut HeaderMap, rules: &[HeaderRule], phase: &str) {
    for rule in rules
        .iter()
        .filter(|rule| rule.enabled && rule.phase == phase)
    {
        let name = rule.name.to_ascii_lowercase();
        match rule.action.as_str() {
            "set" => {
                if let Some(value) = &rule.value {
                    headers.insert(name, value.clone());
                }
            }
            "remove" => {
                headers.remove(&name);
            }
            _ => {}
        }
    }
}

/// 根据 Content-Type 与规则配置改写请求 body。
pub fn rewrite_body(
    body: &[u8],
    content_type: Option<&str>,
    content_encoding: Option<&str>,
    max_rewrite_body_bytes: u64,
    skip_compressed_body: bool,
    rules: &[BodyRewriteRule],
) -> AppResult<RewriteOutcome> {
    let enabled: Vec<&BodyRewriteRule> = rules.iter().filter(|rule| rule.enabled).collect();
    if enabled.is_empty() {
        return Ok(RewriteOutcome::unchanged());
    }
    if body.len() as u64 > max_rewrite_body_bytes {
        return Ok(RewriteOutcome::skipped("body 超过最大可改写大小"));
    }
    if skip_compressed_body && content_encoding.is_some_and(|value| !value.trim().is_empty()) {
        return Ok(RewriteOutcome::skipped("压缩 body 已跳过"));
    }

    let content_type = content_type.unwrap_or_default().to_ascii_lowercase();
    let body_kind = detect_body_kind(body, &content_type, &enabled);
    match body_kind {
        BodyKind::Json => rewrite_json_body(body, &enabled),
        BodyKind::Form => rewrite_form_body(body, &enabled),
        BodyKind::Unknown => Ok(RewriteOutcome::unchanged()),
    }
}

/// Body 改写结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewriteOutcome {
    /// 是否发生改写。
    pub changed: bool,
    /// 新 body。未改写时为空，由调用方继续使用原 body。
    pub body: Vec<u8>,
    /// 跳过原因，可用于写 warning 日志。
    pub warning: Option<String>,
}

impl RewriteOutcome {
    fn changed(body: Vec<u8>) -> Self {
        Self {
            changed: true,
            body,
            warning: None,
        }
    }

    fn changed_with_warning(body: Vec<u8>, warning: Option<String>) -> Self {
        Self {
            changed: true,
            body,
            warning,
        }
    }

    fn unchanged() -> Self {
        Self {
            changed: false,
            body: Vec::new(),
            warning: None,
        }
    }

    fn skipped(reason: &str) -> Self {
        Self {
            changed: false,
            body: Vec::new(),
            warning: Some(reason.to_string()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BodyKind {
    Json,
    Form,
    Unknown,
}

fn detect_body_kind(body: &[u8], content_type: &str, rules: &[&BodyRewriteRule]) -> BodyKind {
    let content_is_json = content_type.contains("application/json")
        || content_type.contains("text/json")
        || content_type.contains("+json");
    let content_is_form = content_type.contains("application/x-www-form-urlencoded");

    if content_is_json || (content_type.is_empty() && looks_like_json(body)) {
        return BodyKind::Json;
    }
    if content_is_form {
        return BodyKind::Form;
    }
    if rules.iter().any(|rule| rule.body_type == "json") {
        return BodyKind::Json;
    }
    if rules.iter().any(|rule| rule.body_type == "form") {
        return BodyKind::Form;
    }
    BodyKind::Unknown
}

fn looks_like_json(body: &[u8]) -> bool {
    body.iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace())
        .is_some_and(|byte| matches!(byte, b'{' | b'['))
}

fn rewrite_json_body(body: &[u8], rules: &[&BodyRewriteRule]) -> AppResult<RewriteOutcome> {
    let mut value: Value = serde_json::from_slice(body)?;
    let mut changed = false;
    let mut warnings = Vec::new();
    for rule in rules
        .iter()
        .filter(|rule| rule.body_type == "auto" || rule.body_type == "json")
    {
        let replacement: Value = serde_json::from_str(&rule.value_json)?;
        if set_json_path(&mut value, &rule.path, replacement)? {
            changed = true;
        } else {
            warnings.push(format!("Body 改写规则路径未命中: {}", rule.path));
        }
    }
    let warning = (!warnings.is_empty()).then(|| warnings.join("; "));
    if changed {
        Ok(RewriteOutcome::changed_with_warning(
            serde_json::to_vec(&value)?,
            warning,
        ))
    } else if let Some(warning) = warning {
        Ok(RewriteOutcome::skipped(&warning))
    } else {
        Ok(RewriteOutcome::unchanged())
    }
}

fn set_json_path(root: &mut Value, path: &str, replacement: Value) -> AppResult<bool> {
    let parts: Vec<&str> = path.split('.').filter(|part| !part.is_empty()).collect();
    if parts.is_empty() {
        return Err(AppError::InvalidInput("JSON 改写路径不能为空".to_string()));
    }

    let mut current = root;
    for part in &parts[..parts.len() - 1] {
        if let Ok(index) = part.parse::<usize>() {
            let array = current.as_array_mut().ok_or_else(|| {
                AppError::InvalidInput(format!("路径 {path} 需要数组节点 {part}"))
            })?;
            let Some(next) = array.get_mut(index) else {
                return Ok(false);
            };
            current = next;
        } else {
            if !current.is_object() {
                *current = Value::Object(Map::new());
            }
            let object = current.as_object_mut().ok_or_else(|| {
                AppError::InvalidInput(format!("路径 {path} 需要对象节点 {part}"))
            })?;
            current = object
                .entry((*part).to_string())
                .or_insert_with(|| Value::Object(Map::new()));
        }
    }

    let leaf = parts[parts.len() - 1];
    if let Ok(index) = leaf.parse::<usize>() {
        let array = current
            .as_array_mut()
            .ok_or_else(|| AppError::InvalidInput(format!("路径 {path} 叶子节点不是数组")))?;
        if let Some(slot) = array.get_mut(index) {
            *slot = replacement;
            Ok(true)
        } else {
            Ok(false)
        }
    } else {
        if !current.is_object() {
            *current = Value::Object(Map::new());
        }
        let object = current
            .as_object_mut()
            .ok_or_else(|| AppError::InvalidInput(format!("路径 {path} 叶子节点不是对象")))?;
        object.insert(leaf.to_string(), replacement);
        Ok(true)
    }
}

fn rewrite_form_body(body: &[u8], rules: &[&BodyRewriteRule]) -> AppResult<RewriteOutcome> {
    let original = String::from_utf8_lossy(body);
    let mut pairs: Vec<(String, String)> = form_urlencoded::parse(original.as_bytes())
        .into_owned()
        .collect();
    let mut changed = false;
    for rule in rules
        .iter()
        .filter(|rule| rule.body_type == "auto" || rule.body_type == "form")
    {
        let value: Value = serde_json::from_str(&rule.value_json)?;
        let rendered = match value {
            Value::String(value) => value,
            Value::Null => String::new(),
            other => other.to_string(),
        };
        if let Some((_, current)) = pairs.iter_mut().find(|(key, _)| key == &rule.path) {
            *current = rendered;
        } else {
            pairs.push((rule.path.clone(), rendered));
        }
        changed = true;
    }
    if !changed {
        return Ok(RewriteOutcome::unchanged());
    }
    let mut encoded = form_urlencoded::Serializer::new(String::new());
    encoded.extend_pairs(
        pairs
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str())),
    );
    Ok(RewriteOutcome::changed(encoded.finish().into_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header_rule(phase: &str, action: &str, name: &str, value: Option<&str>) -> HeaderRule {
        HeaderRule {
            id: "h1".to_string(),
            phase: phase.to_string(),
            action: action.to_string(),
            name: name.to_string(),
            value: value.map(ToOwned::to_owned),
            enabled: true,
            sort_order: 0,
        }
    }

    fn rule(path: &str, value_json: &str, body_type: &str) -> BodyRewriteRule {
        BodyRewriteRule {
            id: "r1".to_string(),
            body_type: body_type.to_string(),
            path: path.to_string(),
            value_json: value_json.to_string(),
            enabled: true,
            sort_order: 0,
        }
    }

    #[test]
    fn applies_header_set_and_remove_for_matching_phase() {
        let mut headers = HeaderMap::from([
            ("x-old".to_string(), "1".to_string()),
            ("x-keep".to_string(), "ok".to_string()),
        ]);
        apply_header_rules(
            &mut headers,
            &[
                header_rule("request", "set", "X-New", Some("2")),
                header_rule("request", "remove", "X-Old", None),
                header_rule("response", "set", "X-Skip", Some("3")),
            ],
            "request",
        );

        assert_eq!(headers.get("x-new"), Some(&"2".to_string()));
        assert!(!headers.contains_key("x-old"));
        assert_eq!(headers.get("x-keep"), Some(&"ok".to_string()));
        assert!(!headers.contains_key("x-skip"));
    }

    #[test]
    fn rewrites_nested_json_path() {
        let outcome = rewrite_body(
            br#"{"user":{"name":"old"},"items":[{"status":"old"}]}"#,
            Some("application/json"),
            None,
            1024,
            true,
            &[
                rule("user.name", "\"new\"", "json"),
                rule("items.0.status", "\"ok\"", "json"),
            ],
        )
        .expect("JSON body 应可改写");
        assert_eq!(
            String::from_utf8(outcome.body).expect("应是 UTF-8"),
            r#"{"items":[{"status":"ok"}],"user":{"name":"new"}}"#
        );
    }

    #[test]
    fn rewrites_json_without_content_type_when_body_looks_like_json() {
        let outcome = rewrite_body(
            br#"{"a":1}"#,
            None,
            None,
            1024,
            true,
            &[rule("a", "2", "auto")],
        )
        .expect("缺失 Content-Type 但像 JSON 时应可改写");
        assert_eq!(
            String::from_utf8(outcome.body).expect("应是 UTF-8"),
            r#"{"a":2}"#
        );
    }

    #[test]
    fn skips_json_array_out_of_range_without_expanding() {
        let outcome = rewrite_body(
            br#"{"items":[{"status":"old"}]}"#,
            Some("application/json"),
            None,
            1024,
            true,
            &[rule("items.3.status", "\"new\"", "json")],
        )
        .expect("数组越界应跳过而不是报错");
        assert!(!outcome.changed);
        assert_eq!(
            outcome.warning.as_deref(),
            Some("Body 改写规则路径未命中: items.3.status")
        );
    }

    #[test]
    fn rewrites_form_body() {
        let outcome = rewrite_body(
            b"a=1&b=2",
            Some("application/x-www-form-urlencoded"),
            None,
            1024,
            true,
            &[rule("b", "\"changed\"", "form"), rule("c", "3", "form")],
        )
        .expect("form body 应可改写");
        assert_eq!(
            String::from_utf8(outcome.body).expect("应是 UTF-8"),
            "a=1&b=changed&c=3"
        );
    }

    #[test]
    fn form_content_type_wins_when_json_rules_are_also_present() {
        let outcome = rewrite_body(
            b"a=1&b=2",
            Some("application/x-www-form-urlencoded; charset=utf-8"),
            None,
            1024,
            true,
            &[
                rule("json.only", "\"ignored\"", "json"),
                rule("b", "\"ok\"", "form"),
            ],
        )
        .expect("form Content-Type 不应被 JSON 规则误导");
        assert_eq!(
            String::from_utf8(outcome.body).expect("应是 UTF-8"),
            "a=1&b=ok"
        );
    }

    #[test]
    fn skips_compressed_body() {
        let outcome = rewrite_body(
            br#"{"a":1}"#,
            Some("application/json"),
            Some("gzip"),
            1024,
            true,
            &[rule("a", "2", "json")],
        )
        .expect("跳过压缩 body 不应报错");
        assert!(!outcome.changed);
        assert!(outcome.warning.is_some());
    }

    #[test]
    fn skips_body_larger_than_rewrite_limit() {
        let outcome = rewrite_body(
            br#"{"a":1}"#,
            Some("application/json"),
            None,
            4,
            true,
            &[rule("a", "2", "json")],
        )
        .expect("超过大小限制应跳过");
        assert!(!outcome.changed);
        assert_eq!(outcome.warning.as_deref(), Some("body 超过最大可改写大小"));
    }
}
