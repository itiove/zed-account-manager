//! 暴露给 Vue 前端的 Tauri command 层。

use serde::Serialize;
use tauri::AppHandle;

use crate::{accounts, oauth, zed_process};

#[derive(Debug, Clone, Serialize)]
pub struct PendingLoginDto {
    pub login_id: String,
    pub verification_uri: String,
    pub profile_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PrepareBrowserDto {
    pub account_id: String,
    pub profile_id: String,
    pub initial_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum LoginPollResult {
    Pending,
    Success { account: accounts::AccountSummary },
    Error { message: String },
}

#[tauri::command]
pub fn list_accounts() -> Vec<accounts::AccountSummary> {
    let current = accounts::current_account_id();
    accounts::list_accounts()
        .iter()
        .map(|a| accounts::to_summary(a, current.as_deref()))
        .collect()
}

/// 开始 OAuth：只返回授权 URL + 隔离 profile，不弹新窗口。
/// RSA 密钥生成较慢，放到阻塞线程池，避免冻结主线程 UI。
#[tauri::command]
pub async fn login_start(app: AppHandle) -> Result<PendingLoginDto, String> {
    let pending = tauri::async_runtime::spawn_blocking(move || oauth::start_login(&app))
        .await
        .map_err(|e| format!("启动登录任务失败: {e}"))??;
    Ok(PendingLoginDto {
        login_id: pending.login_id,
        verification_uri: pending.verification_uri,
        profile_id: pending.profile_id,
    })
}

#[tauri::command]
pub fn login_cancel(app: AppHandle) -> Result<(), String> {
    oauth::cancel_login(&app)
}

/// 轮询登录；成功时抓 cookie 并写入账号（前端负责关闭内嵌浏览器 UI）。
#[tauri::command]
pub async fn login_poll(app: AppHandle, login_id: String) -> LoginPollResult {
    match oauth::poll_login(&login_id) {
        Ok(Some((user_id, access_token))) => {
            match oauth::finalize_login(&app, &login_id, user_id, access_token).await {
                Ok(result) => {
                    // upsert 内部有多个阻塞 HTTP 请求，放到阻塞线程池执行。
                    let upsert = tauri::async_runtime::spawn_blocking(move || {
                        accounts::upsert_account_from_credentials(
                            &result.user_id,
                            &result.access_token,
                            result.web_session,
                        )
                    })
                    .await
                    .map_err(|e| format!("保存账号任务失败: {e}"))
                    .and_then(|r| r);
                    match upsert {
                        Ok(account) => LoginPollResult::Success {
                            account: accounts::to_summary(&account, None),
                        },
                        Err(e) => LoginPollResult::Error {
                            message: format!("登录成功但保存账号失败: {e}"),
                        },
                    }
                }
                Err(e) => LoginPollResult::Error {
                    message: format!("登录收尾失败: {e}"),
                },
            }
        }
        Ok(None) => LoginPollResult::Pending,
        Err(message) => LoginPollResult::Error { message },
    }
}

/// 切换账号：写 Keychain + 重启 Zed 进程，均为阻塞操作，放线程池执行。
#[tauri::command]
pub async fn switch_account(account_id: String) -> Result<accounts::AccountSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let account = accounts::switch_to_account(&account_id)?;
        if let Err(e) = zed_process::restart_to_apply_credentials() {
            return Err(format!(
                "已写入 Keychain，但自动重启 Zed 失败（可以手动退出并重新打开 Zed 完成切换）: {e}"
            ));
        }
        Ok(accounts::to_summary(&account, Some(&account_id)))
    })
    .await
    .map_err(|e| format!("切换任务失败: {e}"))?
}

/// 刷新额度：内部是多个阻塞 HTTP 请求，放线程池执行避免冻结 UI。
#[tauri::command]
pub async fn refresh_quota(
    account_id: Option<String>,
) -> Result<Vec<accounts::AccountSummary>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let current = accounts::current_account_id();
        match account_id {
            Some(id) => {
                let account = accounts::refresh_account(&id)?;
                Ok(vec![accounts::to_summary(&account, current.as_deref())])
            }
            None => {
                let results = accounts::refresh_all_accounts();
                let mut summaries = Vec::new();
                let mut first_error: Option<String> = None;
                for result in results {
                    match result {
                        Ok(account) => {
                            summaries.push(accounts::to_summary(&account, current.as_deref()))
                        }
                        Err(e) => {
                            if first_error.is_none() {
                                first_error = Some(e);
                            }
                        }
                    }
                }
                if summaries.is_empty() {
                    if let Some(e) = first_error {
                        return Err(e);
                    }
                }
                Ok(summaries)
            }
        }
    })
    .await
    .map_err(|e| format!("刷新任务失败: {e}"))?
}

/// 返回账号对应的 web profile（前端再 browser_open）。
#[tauri::command]
pub fn prepare_account_browser(account_id: String) -> Result<PrepareBrowserDto, String> {
    let profile_id = accounts::ensure_web_profile(&account_id)?;
    Ok(PrepareBrowserDto {
        account_id,
        profile_id,
        initial_url: "https://dashboard.zed.dev/".to_string(),
    })
}

/// 从当前内嵌浏览器抓 cookie 并刷新额度。
#[tauri::command]
pub async fn recapture_web_session(
    app: AppHandle,
    account_id: String,
) -> Result<accounts::AccountSummary, String> {
    let profile_id = accounts::ensure_web_profile(&account_id)?;
    let _session = oauth::recapture_account_session(&app, &account_id, &profile_id).await?;
    let account = accounts::refresh_account(&account_id)?;
    let current = accounts::current_account_id();
    Ok(accounts::to_summary(&account, current.as_deref()))
}

#[tauri::command]
pub fn logout_current() -> Result<(), String> {
    accounts::logout_current()?;
    zed_process::restart_to_clear_credentials()
        .map_err(|e| format!("已清空本地登录态，但自动重启 Zed 失败（可手动重启 Zed 完成）: {e}"))
}

#[tauri::command]
pub fn remove_account(account_id: String) -> Result<(), String> {
    accounts::remove_account(&account_id)
}
