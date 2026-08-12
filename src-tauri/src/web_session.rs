//! 每个账号独立的 Web 会话存储与 Dashboard billing 接口调用。
//!
//! 登录时在内置 WebView 中完成授权；登录成功后把浏览器 cookie 快照
//! （尤其是 `zed.session`）按账号落到本地文件。后续刷额度时直接用这些
//! cookie 请求 `https://cloud.zed.dev/frontend/billing/usage`，不必再开浏览器。
//!
//! WebView 侧的数据隔离：
//! - macOS：`data_store_identifier`（WKWebView 独立 store）
//! - 其它平台：`data_directory` 指向 `browser_profiles/<profile_id>/`

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

const ZED_CLOUD_BASE_URL: &str = "https://cloud.zed.dev";
const DASHBOARD_ORIGIN: &str = "https://dashboard.zed.dev";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCookie {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default)]
    pub secure: bool,
    #[serde(default)]
    pub http_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WebSession {
    /// 与 WebView `data_store_identifier` / 本地 profile 目录一一对应。
    pub profile_id: String,
    #[serde(default)]
    pub cookies: Vec<StoredCookie>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub captured_at: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct BillingUsage {
    pub plan_raw: Option<String>,
    pub token_spend_used_cents: Option<i64>,
    pub token_spend_limit_cents: Option<i64>,
    pub token_spend_remaining_cents: Option<i64>,
    pub edit_predictions_used: Option<i64>,
    pub edit_predictions_limit_raw: Option<String>,
    pub edit_predictions_remaining_raw: Option<String>,
    pub billing_period_end_at: Option<i64>,
    #[allow(dead_code)]
    pub raw: Option<Value>,
}

fn data_root() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("无法获取用户主目录")?;
    let dir = home.join(".config").join("zed-account-switcher");
    fs::create_dir_all(&dir).map_err(|e| format!("创建数据目录失败: {e}"))?;
    Ok(dir)
}

pub fn browser_profiles_dir() -> Result<PathBuf, String> {
    let dir = data_root()?.join("browser_profiles");
    fs::create_dir_all(&dir).map_err(|e| format!("创建浏览器配置目录失败: {e}"))?;
    Ok(dir)
}

pub fn profile_data_dir(profile_id: &str) -> Result<PathBuf, String> {
    let safe = sanitize_profile_id(profile_id);
    let dir = browser_profiles_dir()?.join(&safe);
    fs::create_dir_all(&dir).map_err(|e| format!("创建 profile 目录失败: {e}"))?;
    Ok(dir)
}

pub fn session_file_path(account_id: &str) -> Result<PathBuf, String> {
    let safe = sanitize_profile_id(account_id);
    Ok(data_root()?
        .join("web_sessions")
        .join(format!("{safe}.json")))
}

fn sanitize_profile_id(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// 为一次新登录或某个账号生成稳定的 16 字节 store 标识。
pub fn profile_id_to_store_bytes(profile_id: &str) -> [u8; 16] {
    if let Ok(uuid) = Uuid::parse_str(profile_id) {
        return *uuid.as_bytes();
    }
    *Uuid::new_v5(&Uuid::NAMESPACE_URL, profile_id.as_bytes()).as_bytes()
}

pub fn new_profile_id() -> String {
    Uuid::new_v4().to_string()
}

pub fn load_session(account_id: &str) -> Option<WebSession> {
    let path = session_file_path(account_id).ok()?;
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn save_session(account_id: &str, session: &WebSession) -> Result<(), String> {
    let path = session_file_path(account_id)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建 web_sessions 目录失败: {e}"))?;
    }
    let content = serde_json::to_string_pretty(session).map_err(|e| e.to_string())?;
    fs::write(path, content).map_err(|e| format!("写入 web session 失败: {e}"))
}

pub fn delete_session(account_id: &str) -> Result<(), String> {
    let existing = load_session(account_id);
    let path = session_file_path(account_id)?;
    if path.exists() {
        fs::remove_file(&path).map_err(|e| format!("删除 web session 失败: {e}"))?;
    }
    if let Some(session) = existing {
        let dir = browser_profiles_dir()?.join(sanitize_profile_id(&session.profile_id));
        if dir.exists() {
            let _ = fs::remove_dir_all(dir);
        }
    }
    let fallback = browser_profiles_dir()?.join(sanitize_profile_id(account_id));
    if fallback.exists() {
        let _ = fs::remove_dir_all(fallback);
    }
    Ok(())
}

pub fn session_has_zed_cookie(session: &WebSession) -> bool {
    session.cookies.iter().any(|c| c.name == "zed.session")
}

/// 把 cookie 列表序列化成 HTTP Cookie 头。
pub fn cookie_header(cookies: &[StoredCookie]) -> String {
    cookies
        .iter()
        .filter(|c| !c.name.is_empty())
        .map(|c| format!("{}={}", c.name, c.value))
        .collect::<Vec<_>>()
        .join("; ")
}

fn json_str(value: &Value, path: &[&str]) -> Option<String> {
    let mut cur = value;
    for key in path {
        cur = cur.get(*key)?;
    }
    match cur {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn json_i64(value: &Value, path: &[&str]) -> Option<i64> {
    let mut cur = value;
    for key in path {
        cur = cur.get(*key)?;
    }
    match cur {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|v| v.round() as i64)),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn pick_i64(value: &Value, paths: &[&[&str]]) -> Option<i64> {
    for path in paths {
        if let Some(v) = json_i64(value, path) {
            return Some(v);
        }
    }
    None
}

fn pick_str(value: &Value, paths: &[&[&str]]) -> Option<String> {
    for path in paths {
        if let Some(v) = json_str(value, path) {
            let t = v.trim().to_string();
            if !t.is_empty() {
                return Some(t);
            }
        }
    }
    None
}

/// 带 Dashboard 同源头信息请求 cloud.zed.dev 的 frontend 接口。
fn fetch_frontend_json(session: &WebSession, path: &str) -> Result<Value, String> {
    if session.cookies.is_empty() {
        return Err("该账号还没有 Web 会话 cookie，请重新通过内置浏览器登录".into());
    }
    if !session_has_zed_cookie(session) {
        return Err("未找到 zed.session，Web 接口无法鉴权，请重新登录".into());
    }

    let cookie = cookie_header(&session.cookies);
    let url = format!("{ZED_CLOUD_BASE_URL}{path}");
    let response = ureq::get(&url)
        .set("Accept", "application/json")
        .set("Content-Type", "application/json")
        .set("Cookie", &cookie)
        .set("Origin", DASHBOARD_ORIGIN)
        .set("Referer", &format!("{DASHBOARD_ORIGIN}/"))
        .set(
            "User-Agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) ZedAccountManager/0.1",
        )
        .call()
        .map_err(|e| format!("请求 {path} 失败: {e}"))?;

    let status = response.status();
    if status != 200 {
        let body = response.into_string().unwrap_or_default();
        return Err(format!(
            "{path} 返回 HTTP {status}: {}",
            body.chars().take(200).collect::<String>()
        ));
    }

    response
        .into_json()
        .map_err(|e| format!("解析 {path} 失败: {e}"))
}

/// `/frontend/session` 返回的用户资料（邮箱、GitHub 用户名、套餐）。
#[derive(Debug, Clone, Default)]
pub struct SessionProfile {
    pub email: Option<String>,
    pub github_login: Option<String>,
    pub username: Option<String>,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub plan_raw: Option<String>,
}

/// 使用已保存的 Web 会话 cookie 调用 Dashboard 的 session 接口拉取个人资料。
pub fn fetch_session_profile(session: &WebSession) -> Result<SessionProfile, String> {
    let raw = fetch_frontend_json(session, "/frontend/session")?;

    // 优先取个人 organization 的 plan（is_personal = true）。
    let plan_raw = raw
        .get("organizations")
        .and_then(Value::as_array)
        .and_then(|orgs| {
            orgs.iter()
                .find(|o| o.get("is_personal").and_then(Value::as_bool).unwrap_or(false))
                .or_else(|| orgs.first())
        })
        .and_then(|org| org.get("plan"))
        .and_then(Value::as_str)
        .map(|s| s.to_string());

    Ok(SessionProfile {
        email: pick_str(&raw, &[&["user", "email"]]),
        github_login: pick_str(&raw, &[&["user", "github_login"]]),
        username: pick_str(&raw, &[&["user", "username"]]),
        name: pick_str(&raw, &[&["user", "name"]]),
        avatar_url: pick_str(&raw, &[&["user", "avatar_url"]]),
        plan_raw,
    })
}

/// 使用已保存的 Web 会话 cookie 调用 Dashboard 的 billing/usage 接口。
pub fn fetch_billing_usage(session: &WebSession) -> Result<BillingUsage, String> {
    let raw = fetch_frontend_json(session, "/frontend/billing/usage")?;

    Ok(BillingUsage {
        plan_raw: pick_str(
            &raw,
            &[
                &["subscription", "name"],
                &["plan"],
                &["plan", "name"],
                &["plan", "plan_v3"],
                &["plan", "plan"],
            ],
        ),
        token_spend_used_cents: pick_i64(
            &raw,
            &[
                &["current_usage", "token_spend", "spend_in_cents"],
                &["current_usage", "token_spend_in_cents"],
                &["current_usage", "token_spend", "used"],
                &["token_spend", "used"],
                &["usage", "token_spend", "used"],
            ],
        ),
        token_spend_limit_cents: pick_i64(
            &raw,
            &[
                &["current_usage", "token_spend", "limit_in_cents"],
                &["current_usage", "token_spend", "limit"],
                &["token_spend", "limit"],
                &["usage", "token_spend", "limit"],
            ],
        ),
        token_spend_remaining_cents: pick_i64(
            &raw,
            &[
                &["current_usage", "token_spend", "remaining_in_cents"],
                &["current_usage", "token_spend", "remaining"],
                &["token_spend", "remaining"],
                &["usage", "token_spend", "remaining"],
            ],
        ),
        edit_predictions_used: pick_i64(
            &raw,
            &[
                &["current_usage", "edit_predictions", "used"],
                &["edit_predictions", "used"],
                &["usage", "edit_predictions", "used"],
            ],
        ),
        edit_predictions_limit_raw: pick_str(
            &raw,
            &[
                &["current_usage", "edit_predictions", "limit"],
                &["edit_predictions", "limit"],
                &["usage", "edit_predictions", "limit"],
            ],
        ),
        edit_predictions_remaining_raw: pick_str(
            &raw,
            &[
                &["current_usage", "edit_predictions", "remaining"],
                &["edit_predictions", "remaining"],
            ],
        ),
        billing_period_end_at: pick_i64(
            &raw,
            &[
                &["subscription", "period", "end_at"],
                &["period", "end_at"],
                &["billing_period_end_at"],
            ],
        ),
        raw: Some(raw),
    })
}

/// 从 WebView cookie 列表里筛出 zed 相关域名的 cookie。
pub fn filter_zed_cookies(cookies: Vec<StoredCookie>) -> Vec<StoredCookie> {
    cookies
        .into_iter()
        .filter(|c| {
            if c.name == "zed.session" {
                return true;
            }
            let domain = c.domain.as_deref().unwrap_or("").to_ascii_lowercase();
            domain.contains("zed.dev")
        })
        .collect()
}
