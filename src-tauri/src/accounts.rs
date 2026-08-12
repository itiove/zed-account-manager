use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

use crate::keychain;
use crate::web_session::{self, WebSession};

const ZED_SERVER_URL: &str = "https://zed.dev";
const ZED_CLOUD_BASE_URL: &str = "https://cloud.zed.dev";

/// 单个账号的完整本地存储结构（含明文 access_token，与 Zed 官方 Keychain
/// 里存放的内容一致，仅用于本地"账号池"之间切换）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAccount {
    pub id: String,
    pub user_id: String,
    pub access_token: String,
    pub github_login: Option<String>,
    pub display_name: Option<String>,
    /// Zed 账号邮箱（来自 /frontend/session）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// GitHub 头像 URL（来自 /frontend/session）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    pub plan_raw: Option<String>,
    pub token_spend_used_cents: Option<i64>,
    pub token_spend_limit_cents: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_spend_remaining_cents: Option<i64>,
    pub edit_predictions_used: Option<i64>,
    pub edit_predictions_limit_raw: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edit_predictions_remaining_raw: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub billing_period_end_at: Option<i64>,
    /// 独立浏览器会话 profile id（与 WebView data_store 绑定）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_profile_id: Option<String>,
    pub created_at: i64,
    pub last_used: i64,
    pub last_refreshed_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_quota_error: Option<String>,
}

/// 暴露给前端的精简视图（不带 access_token）。
#[derive(Debug, Clone, Serialize)]
pub struct AccountSummary {
    pub id: String,
    pub github_login: Option<String>,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
    pub plan_raw: Option<String>,
    pub is_current: bool,
    pub token_spend_used_cents: Option<i64>,
    pub token_spend_limit_cents: Option<i64>,
    pub token_spend_remaining_cents: Option<i64>,
    pub edit_predictions_used: Option<i64>,
    pub edit_predictions_limit_raw: Option<String>,
    pub edit_predictions_remaining_raw: Option<String>,
    pub billing_period_end_at: Option<i64>,
    pub last_refreshed_at: Option<i64>,
    pub last_quota_error: Option<String>,
    /// 是否已保存可用的 Web 会话（含 zed.session）。
    pub has_web_session: bool,
    pub web_profile_id: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct AccountIndex {
    current_account_id: Option<String>,
}

fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

fn data_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("无法获取用户主目录")?;
    let dir = home.join(".config").join("zed-account-switcher");
    fs::create_dir_all(&dir).map_err(|e| format!("创建数据目录失败: {e}"))?;
    Ok(dir)
}

fn accounts_dir() -> Result<PathBuf, String> {
    let dir = data_dir()?.join("accounts");
    fs::create_dir_all(&dir).map_err(|e| format!("创建账号目录失败: {e}"))?;
    Ok(dir)
}

fn index_path() -> Result<PathBuf, String> {
    Ok(data_dir()?.join("index.json"))
}

fn sanitize_component(value: &str) -> String {
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

fn build_account_id(user_id: &str) -> String {
    format!("zed_{}", sanitize_component(user_id))
}

fn load_index() -> AccountIndex {
    index_path()
        .ok()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

fn save_index(index: &AccountIndex) -> Result<(), String> {
    let path = index_path()?;
    let content = serde_json::to_string_pretty(index).map_err(|e| e.to_string())?;
    fs::write(path, content).map_err(|e| format!("写入索引失败: {e}"))
}

fn account_file_path(account_id: &str) -> Result<PathBuf, String> {
    Ok(accounts_dir()?.join(format!("{}.json", sanitize_component(account_id))))
}

pub fn load_account(account_id: &str) -> Option<StoredAccount> {
    let path = account_file_path(account_id).ok()?;
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn save_account(account: &StoredAccount) -> Result<(), String> {
    let path = account_file_path(&account.id)?;
    let content = serde_json::to_string_pretty(account).map_err(|e| e.to_string())?;
    fs::write(path, content).map_err(|e| format!("写入账号失败: {e}"))
}

pub fn list_accounts() -> Vec<StoredAccount> {
    let Ok(dir) = accounts_dir() else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut accounts: Vec<_> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
        .filter_map(|entry| fs::read_to_string(entry.path()).ok())
        .filter_map(|content| serde_json::from_str::<StoredAccount>(&content).ok())
        .collect();
    accounts.sort_by(|a, b| b.last_used.cmp(&a.last_used));
    accounts
}

pub fn current_account_id() -> Option<String> {
    load_index().current_account_id
}

pub fn set_current_account_id(account_id: Option<&str>) -> Result<(), String> {
    let mut index = load_index();
    index.current_account_id = account_id.map(|s| s.to_string());
    save_index(&index)
}

pub fn to_summary(account: &StoredAccount, current_id: Option<&str>) -> AccountSummary {
    let session = web_session::load_session(&account.id);
    let has_web_session = session
        .as_ref()
        .map(web_session::session_has_zed_cookie)
        .unwrap_or(false);

    AccountSummary {
        id: account.id.clone(),
        github_login: account.github_login.clone(),
        display_name: account.display_name.clone(),
        email: account.email.clone(),
        avatar_url: account.avatar_url.clone(),
        plan_raw: account.plan_raw.clone(),
        is_current: current_id == Some(account.id.as_str()),
        token_spend_used_cents: account.token_spend_used_cents,
        token_spend_limit_cents: account.token_spend_limit_cents,
        token_spend_remaining_cents: account.token_spend_remaining_cents,
        edit_predictions_used: account.edit_predictions_used,
        edit_predictions_limit_raw: account.edit_predictions_limit_raw.clone(),
        edit_predictions_remaining_raw: account.edit_predictions_remaining_raw.clone(),
        billing_period_end_at: account.billing_period_end_at,
        last_refreshed_at: account.last_refreshed_at,
        last_quota_error: account.last_quota_error.clone(),
        has_web_session,
        web_profile_id: account.web_profile_id.clone(),
    }
}

fn json_str(value: &Value, path: &[&str]) -> Option<String> {
    let mut cur = value;
    for key in path {
        cur = cur.get(*key)?;
    }
    cur.as_str().map(|s| s.to_string())
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

/// 调用 `cloud.zed.dev` 官方客户端接口拉取用户信息 + 用量。
fn fetch_user_bundle(user_id: &str, access_token: &str) -> Result<Value, String> {
    let url = format!("{ZED_CLOUD_BASE_URL}/client/users/me");
    let authorization = format!("{} {}", user_id.trim(), access_token.trim());
    let response = ureq::get(&url)
        .set("Authorization", &authorization)
        .set("User-Agent", "zed-account-switcher/0.1")
        .call()
        .map_err(|e| format!("请求 Zed 用户信息失败: {e}"))?;
    response
        .into_json::<Value>()
        .map_err(|e| format!("解析 Zed 用户信息失败: {e}"))
}

fn apply_client_me_fields(account: &mut StoredAccount, bundle: &Value) {
    if let Some(v) = json_str(bundle, &["user", "github_login"])
        .or_else(|| json_str(bundle, &["github_login"]))
    {
        account.github_login = Some(v);
    }
    if let Some(v) = json_str(bundle, &["user", "name"]).or_else(|| json_str(bundle, &["name"])) {
        account.display_name = Some(v);
    }
    if let Some(v) = json_str(bundle, &["plan", "plan_v3"]).or_else(|| json_str(bundle, &["plan", "plan"]))
    {
        account.plan_raw = Some(v);
    }
    if let Some(v) = json_i64(bundle, &["plan", "usage", "token_spend", "used"]) {
        account.token_spend_used_cents = Some(v);
    }
    if let Some(v) = json_i64(bundle, &["plan", "usage", "token_spend", "limit"]) {
        account.token_spend_limit_cents = Some(v);
    }
    if let Some(v) = json_i64(bundle, &["plan", "usage", "token_spend", "remaining"]) {
        account.token_spend_remaining_cents = Some(v);
    }
    if let Some(v) = json_i64(bundle, &["plan", "usage", "edit_predictions", "used"]) {
        account.edit_predictions_used = Some(v);
    }
    if let Some(v) = bundle
        .get("plan")
        .and_then(|p| p.get("usage"))
        .and_then(|u| u.get("edit_predictions"))
        .and_then(|e| e.get("limit"))
        .and_then(|limit| match limit {
            Value::String(s) => Some(s.clone()),
            other => Some(other.to_string().trim_matches('"').to_string()),
        })
    {
        account.edit_predictions_limit_raw = Some(v);
    }
    if let Some(v) = json_i64(bundle, &["plan", "subscription_period", "ended_at"]) {
        account.billing_period_end_at = Some(v);
    }
}

/// 用桌面端凭据 + 可选 Web 会话写入/刷新本地账号。
pub fn upsert_account_from_credentials(
    user_id: &str,
    access_token: &str,
    web_session: Option<WebSession>,
) -> Result<StoredAccount, String> {
    let account_id = build_account_id(user_id);
    let existing = load_account(&account_id);

    let profile_id = web_session
        .as_ref()
        .map(|s| s.profile_id.clone())
        .or_else(|| existing.as_ref().and_then(|a| a.web_profile_id.clone()))
        .unwrap_or_else(web_session::new_profile_id);

    if let Some(mut session) = web_session {
        session.profile_id = profile_id.clone();
        web_session::save_session(&account_id, &session)?;
    }

    let mut account = StoredAccount {
        id: account_id,
        user_id: user_id.to_string(),
        access_token: access_token.to_string(),
        github_login: existing.as_ref().and_then(|a| a.github_login.clone()),
        display_name: existing.as_ref().and_then(|a| a.display_name.clone()),
        email: existing.as_ref().and_then(|a| a.email.clone()),
        avatar_url: existing.as_ref().and_then(|a| a.avatar_url.clone()),
        plan_raw: existing.as_ref().and_then(|a| a.plan_raw.clone()),
        token_spend_used_cents: existing.as_ref().and_then(|a| a.token_spend_used_cents),
        token_spend_limit_cents: existing.as_ref().and_then(|a| a.token_spend_limit_cents),
        token_spend_remaining_cents: existing
            .as_ref()
            .and_then(|a| a.token_spend_remaining_cents),
        edit_predictions_used: existing.as_ref().and_then(|a| a.edit_predictions_used),
        edit_predictions_limit_raw: existing
            .as_ref()
            .and_then(|a| a.edit_predictions_limit_raw.clone()),
        edit_predictions_remaining_raw: existing
            .as_ref()
            .and_then(|a| a.edit_predictions_remaining_raw.clone()),
        billing_period_end_at: existing.as_ref().and_then(|a| a.billing_period_end_at),
        web_profile_id: Some(profile_id),
        created_at: existing
            .as_ref()
            .map(|a| a.created_at)
            .unwrap_or_else(now_ts),
        last_used: now_ts(),
        last_refreshed_at: None,
        last_quota_error: None,
    };

    // 先拉客户端接口（身份 + 基础额度）。
    match fetch_user_bundle(user_id, access_token) {
        Ok(bundle) => {
            apply_client_me_fields(&mut account, &bundle);
            account.last_quota_error = None;
        }
        Err(e) => {
            account.last_quota_error = Some(e);
        }
    }

    // 再尝试 Web 接口（字段更贴近 Dashboard）。
    if let Some(session) = web_session::load_session(&account.id) {
        if web_session::session_has_zed_cookie(&session) {
            // 个人资料：邮箱 / GitHub 用户名 / 套餐。
            if let Ok(profile) = web_session::fetch_session_profile(&session) {
                if profile.email.is_some() {
                    account.email = profile.email;
                }
                if profile.avatar_url.is_some() {
                    account.avatar_url = profile.avatar_url;
                }
                if let Some(login) = profile.github_login.or(profile.username) {
                    account.github_login = Some(login);
                }
                if profile.name.is_some() {
                    account.display_name = profile.name;
                }
                if profile.plan_raw.is_some() {
                    account.plan_raw = profile.plan_raw;
                }
            }

            match web_session::fetch_billing_usage(&session) {
                Ok(usage) => {
                    if let Some(v) = usage.plan_raw {
                        account.plan_raw = Some(v);
                    }
                    if usage.token_spend_used_cents.is_some() {
                        account.token_spend_used_cents = usage.token_spend_used_cents;
                    }
                    if usage.token_spend_limit_cents.is_some() {
                        account.token_spend_limit_cents = usage.token_spend_limit_cents;
                    }
                    if usage.token_spend_remaining_cents.is_some() {
                        account.token_spend_remaining_cents = usage.token_spend_remaining_cents;
                    }
                    if usage.edit_predictions_used.is_some() {
                        account.edit_predictions_used = usage.edit_predictions_used;
                    }
                    if usage.edit_predictions_limit_raw.is_some() {
                        account.edit_predictions_limit_raw = usage.edit_predictions_limit_raw;
                    }
                    if usage.edit_predictions_remaining_raw.is_some() {
                        account.edit_predictions_remaining_raw =
                            usage.edit_predictions_remaining_raw;
                    }
                    if usage.billing_period_end_at.is_some() {
                        account.billing_period_end_at = usage.billing_period_end_at;
                    }
                    // Web 接口成功则清掉额度错误（即便 client/me 失败也算刷到了）。
                    if account.token_spend_used_cents.is_some()
                        || account.edit_predictions_used.is_some()
                        || account.plan_raw.is_some()
                    {
                        account.last_quota_error = None;
                    }
                }
                Err(e) => {
                    // 仅在 client 侧也没数据时记录错误。
                    if account.last_quota_error.is_none()
                        && account.token_spend_used_cents.is_none()
                        && account.plan_raw.is_none()
                    {
                        account.last_quota_error = Some(e);
                    }
                }
            }
        }
    }

    account.last_refreshed_at = Some(now_ts());
    save_account(&account)?;
    Ok(account)
}

/// 刷新单个账号额度。
pub fn refresh_account(account_id: &str) -> Result<StoredAccount, String> {
    let stored = load_account(account_id).ok_or_else(|| format!("账号不存在: {account_id}"))?;
    upsert_account_from_credentials(&stored.user_id, &stored.access_token, None)
}

/// 并行刷新所有账号（每个账号 2~3 个 HTTP 请求，串行会非常慢）。
pub fn refresh_all_accounts() -> Vec<Result<StoredAccount, String>> {
    let handles: Vec<_> = list_accounts()
        .into_iter()
        .map(|account| std::thread::spawn(move || refresh_account(&account.id)))
        .collect();
    handles
        .into_iter()
        .map(|h| {
            h.join()
                .unwrap_or_else(|_| Err("刷新线程异常退出".to_string()))
        })
        .collect()
}

/// 切换到指定账号：写入 Keychain，更新“当前账号”指针。
pub fn switch_to_account(account_id: &str) -> Result<StoredAccount, String> {
    let mut account = load_account(account_id).ok_or_else(|| format!("账号不存在: {account_id}"))?;
    keychain::write_credentials(ZED_SERVER_URL, &account.user_id, &account.access_token)?;
    set_current_account_id(Some(account_id))?;
    account.last_used = now_ts();
    save_account(&account)?;
    Ok(account)
}

pub fn logout_current() -> Result<(), String> {
    keychain::delete_credentials(ZED_SERVER_URL)?;
    set_current_account_id(None)
}

pub fn remove_account(account_id: &str) -> Result<(), String> {
    let path = account_file_path(account_id)?;
    if path.exists() {
        fs::remove_file(&path).map_err(|e| format!("删除账号文件失败: {e}"))?;
    }
    let _ = web_session::delete_session(account_id);
    let mut index = load_index();
    if index.current_account_id.as_deref() == Some(account_id) {
        index.current_account_id = None;
        save_index(&index)?;
    }
    Ok(())
}

pub fn ensure_web_profile(account_id: &str) -> Result<String, String> {
    let mut account =
        load_account(account_id).ok_or_else(|| format!("账号不存在: {account_id}"))?;
    if let Some(id) = account.web_profile_id.clone() {
        return Ok(id);
    }
    let id = web_session::new_profile_id();
    account.web_profile_id = Some(id.clone());
    save_account(&account)?;
    // 初始化空 session，绑定 profile。
    web_session::save_session(
        account_id,
        &WebSession {
            profile_id: id.clone(),
            cookies: vec![],
            captured_at: None,
        },
    )?;
    Ok(id)
}
