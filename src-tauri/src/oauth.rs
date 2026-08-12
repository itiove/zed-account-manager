//! Zed native_app_signin 协议 + 主窗口内嵌隔离浏览器收 cookie。
//!
//! 登录流程不再打开新窗口：
//! 1. `start_login` 只起本地回调服务，返回 verification_uri / profile_id
//! 2. 前端在主窗口展示 BrowserPanel，调用 `browser_open` 挂载内容 WebView
//! 3. 回调完成后 `finalize_login` 从内嵌 WebView 抓 cookie 并清理状态

use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine;
use rand::rngs::OsRng;
use rsa::pkcs1::EncodeRsaPublicKey;
use rsa::{Oaep, Pkcs1v15Encrypt, RsaPrivateKey, RsaPublicKey};
use serde::Deserialize;
use sha2::Sha256;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::AppHandle;
use uuid::Uuid;

use crate::in_app_browser;
use crate::web_session::{self, StoredCookie, WebSession};

const ZED_SERVER_URL: &str = "https://zed.dev";
const LOGIN_TIMEOUT_SECONDS: u64 = 300;

#[derive(Debug, Deserialize)]
struct CallbackQuery {
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PendingLogin {
    pub login_id: String,
    pub verification_uri: String,
    pub profile_id: String,
}

#[derive(Debug, Clone)]
pub struct LoginResult {
    pub user_id: String,
    pub access_token: String,
    #[allow(dead_code)]
    pub profile_id: String,
    pub web_session: Option<WebSession>,
}

struct LoginState {
    login_id: String,
    profile_id: String,
    private_key: RsaPrivateKey,
    result: Mutex<Option<Result<(String, String), String>>>,
}

static ACTIVE_LOGIN: std::sync::LazyLock<Mutex<Option<Arc<LoginState>>>> =
    std::sync::LazyLock::new(|| Mutex::new(None));

fn decode_b64(value: &str) -> Result<Vec<u8>, String> {
    URL_SAFE_NO_PAD
        .decode(value.as_bytes())
        .or_else(|_| URL_SAFE.decode(value.as_bytes()))
        .map_err(|e| format!("base64 解码失败: {e}"))
}

async fn sleep_ms(ms: u64) {
    let _ = tauri::async_runtime::spawn_blocking(move || {
        std::thread::sleep(Duration::from_millis(ms));
    })
    .await;
}

/// 仅启动 OAuth 回调服务，不打开任何新窗口。
pub fn start_login(_app: &AppHandle) -> Result<PendingLogin, String> {
    // 取消上一次未完成登录
    *ACTIVE_LOGIN.lock().unwrap() = None;

    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .map_err(|e| format!("绑定本地端口失败: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    drop(listener);

    let private_key =
        RsaPrivateKey::new(&mut OsRng, 2048).map_err(|e| format!("生成 RSA 密钥失败: {e}"))?;
    let public_key = RsaPublicKey::from(&private_key);
    let public_der = public_key
        .to_pkcs1_der()
        .map_err(|e| format!("编码公钥失败: {e}"))?;
    let public_key_b64 = URL_SAFE_NO_PAD.encode(public_der.as_bytes());

    let login_id = format!("zas_{}", Uuid::new_v4().simple());
    let profile_id = web_session::new_profile_id();
    let verification_uri = format!(
        "{ZED_SERVER_URL}/native_app_signin?native_app_port={port}&native_app_public_key={public_key_b64}"
    );

    let state = Arc::new(LoginState {
        login_id: login_id.clone(),
        profile_id: profile_id.clone(),
        private_key,
        result: Mutex::new(None),
    });
    *ACTIVE_LOGIN.lock().unwrap() = Some(state.clone());

    std::thread::spawn(move || {
        let server = match tiny_http::Server::http(("127.0.0.1", port)) {
            Ok(server) => server,
            Err(e) => {
                *state.result.lock().unwrap() = Some(Err(format!("重新绑定端口失败: {e}")));
                return;
            }
        };

        let deadline = std::time::Instant::now() + Duration::from_secs(LOGIN_TIMEOUT_SECONDS);
        loop {
            if std::time::Instant::now() > deadline {
                *state.result.lock().unwrap() = Some(Err("登录超时（5 分钟）".to_string()));
                return;
            }
            let Ok(Some(request)) = server.recv_timeout(Duration::from_secs(1)) else {
                continue;
            };

            let url = format!("http://127.0.0.1:{port}{}", request.url());
            let parsed = match url::Url::parse(&url) {
                Ok(u) => u,
                Err(_) => continue,
            };
            let query: CallbackQuery =
                match serde_urlencoded::from_str(parsed.query().unwrap_or_default()) {
                    Ok(q) => q,
                    Err(_) => continue,
                };

            if let Some(err) = query.error.filter(|e| !e.is_empty()) {
                *state.result.lock().unwrap() = Some(Err(format!("授权失败: {err}")));
                let _ = request.respond(html_response(
                    false,
                    "授权失败",
                    "授权过程被中断或拒绝，请返回应用重试。",
                ));
                return;
            }

            let (Some(user_id), Some(encrypted_token)) = (query.user_id, query.access_token) else {
                continue;
            };

            let outcome = (|| -> Result<(String, String), String> {
                let encrypted = decode_b64(&encrypted_token)?;
                let decrypted = state
                    .private_key
                    .decrypt(Oaep::new::<Sha256>(), &encrypted)
                    .or_else(|_| state.private_key.decrypt(Pkcs1v15Encrypt, &encrypted))
                    .map_err(|e| format!("解密 access_token 失败: {e}"))?;
                let access_token = String::from_utf8(decrypted)
                    .map_err(|e| format!("access_token 不是合法 UTF-8: {e}"))?;
                Ok((user_id, access_token))
            })();

            let _ = request.respond(if outcome.is_ok() {
                html_response(
                    true,
                    "授权成功",
                    "正在保存会话并同步账号信息，返回应用即可。",
                )
            } else {
                html_response(false, "登录处理失败", "凭据解密失败，请返回应用重试。")
            });
            *state.result.lock().unwrap() = Some(outcome);
            return;
        }
    });

    Ok(PendingLogin {
        login_id,
        verification_uri,
        profile_id,
    })
}

/// 回调结果页模板：与应用一致的浅色卡片风格（shadcn light）。
const CALLBACK_PAGE_TEMPLATE: &str = r#"<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>__TITLE__ · Zed 账号管理</title>
<style>
  * { box-sizing: border-box; }
  body {
    margin: 0;
    min-height: 100vh;
    display: grid;
    place-items: center;
    background: #f8fafc;
    color: #0f172a;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
    -webkit-font-smoothing: antialiased;
  }
  .card {
    width: min(360px, calc(100vw - 48px));
    padding: 36px 32px 30px;
    background: #ffffff;
    border: 1px solid #e2e8f0;
    border-radius: 16px;
    box-shadow: 0 12px 40px rgba(15, 23, 42, 0.10);
    text-align: center;
    animation: pop 0.28s cubic-bezier(0.16, 1, 0.3, 1);
  }
  .icon {
    width: 56px;
    height: 56px;
    margin: 0 auto 18px;
    border-radius: 50%;
    display: grid;
    place-items: center;
  }
  .icon.ok { background: #f0fdf4; border: 1px solid #bbf7d0; color: #16a34a; }
  .icon.err { background: #fef2f2; border: 1px solid #fecaca; color: #dc2626; }
  .icon svg { width: 26px; height: 26px; }
  .icon.ok .stroke {
    stroke-dasharray: 48;
    stroke-dashoffset: 48;
    animation: draw 0.5s 0.15s cubic-bezier(0.65, 0, 0.45, 1) forwards;
  }
  h1 { margin: 0 0 8px; font-size: 18px; font-weight: 700; letter-spacing: -0.01em; }
  p { margin: 0; font-size: 13.5px; line-height: 1.65; color: #64748b; }
  .brand {
    margin-top: 24px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11.5px;
    font-weight: 500;
    color: #94a3b8;
  }
  .brand-dot { width: 6px; height: 6px; border-radius: 50%; background: #cbd5e1; }
  @keyframes pop {
    from { opacity: 0; transform: translateY(10px) scale(0.97); }
    to { opacity: 1; transform: translateY(0) scale(1); }
  }
  @keyframes draw { to { stroke-dashoffset: 0; } }
</style>
</head>
<body>
  <div class="card">
    <div class="icon __KIND__">__ICON__</div>
    <h1>__TITLE__</h1>
    <p>__DESC__</p>
    <div class="brand"><span class="brand-dot"></span>Zed 账号管理</div>
  </div>
</body>
</html>"#;

const ICON_OK: &str = r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path class="stroke" d="M4 12.5l5 5L20 6.5"/></svg>"#;
const ICON_ERR: &str = r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M6 6l12 12M18 6L6 18"/></svg>"#;

fn html_response(ok: bool, title: &str, desc: &str) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let html = CALLBACK_PAGE_TEMPLATE
        .replace("__KIND__", if ok { "ok" } else { "err" })
        .replace("__ICON__", if ok { ICON_OK } else { ICON_ERR })
        .replace("__TITLE__", title)
        .replace("__DESC__", desc);
    tiny_http::Response::from_string(html).with_header(
        tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..])
            .unwrap(),
    )
}

pub fn cancel_login(app: &AppHandle) -> Result<(), String> {
    *ACTIVE_LOGIN.lock().unwrap() = None;
    in_app_browser::close_if_open(app);
    Ok(())
}

pub fn poll_login(login_id: &str) -> Result<Option<(String, String)>, String> {
    let guard = ACTIVE_LOGIN.lock().unwrap();
    let Some(state) = guard.as_ref() else {
        return Err("没有进行中的登录".to_string());
    };
    if state.login_id != login_id {
        return Err("login_id 不匹配".to_string());
    }
    let mut result_guard = state.result.lock().unwrap();
    match result_guard.take() {
        Some(Ok(pair)) => Ok(Some(pair)),
        Some(Err(err)) => Err(err),
        None => Ok(None),
    }
}

fn active_profile(login_id: &str) -> Result<String, String> {
    let guard = ACTIVE_LOGIN.lock().unwrap();
    let Some(state) = guard.as_ref() else {
        return Err("没有进行中的登录".to_string());
    };
    if state.login_id != login_id {
        return Err("login_id 不匹配".to_string());
    }
    Ok(state.profile_id.clone())
}

/// 登录成功后：尽量导航 Dashboard 落 cookie → 抓取 → 清状态（不关 UI，由前端关）。
pub async fn finalize_login(
    app: &AppHandle,
    login_id: &str,
    user_id: String,
    access_token: String,
) -> Result<LoginResult, String> {
    let profile_id = active_profile(login_id)?;

    let _ = in_app_browser::navigate_content(app, "https://dashboard.zed.dev/");
    sleep_ms(1200).await;

    let cookies = capture_cookies(app).await.unwrap_or_default();
    let filtered = web_session::filter_zed_cookies(cookies);
    let web_session = Some(WebSession {
        profile_id: profile_id.clone(),
        cookies: filtered,
        captured_at: Some(chrono::Utc::now().timestamp()),
    });

    *ACTIVE_LOGIN.lock().unwrap() = None;

    Ok(LoginResult {
        user_id,
        access_token,
        profile_id,
        web_session,
    })
}

async fn capture_cookies(app: &AppHandle) -> Result<Vec<StoredCookie>, String> {
    let Some(content) = in_app_browser::content_webview(app) else {
        return Ok(Vec::new());
    };

    let cookies = tauri::async_runtime::spawn_blocking(move || content.cookies())
        .await
        .map_err(|e| format!("读取 cookie 任务失败: {e}"))?
        .map_err(|e| format!("读取 WebView cookie 失败: {e}"))?;

    Ok(cookies
        .into_iter()
        .map(|c| StoredCookie {
            name: c.name().to_string(),
            value: c.value().to_string(),
            domain: c.domain().map(|d| d.to_string()),
            path: c.path().map(|p| p.to_string()),
            secure: c.secure().unwrap_or(false),
            http_only: c.http_only().unwrap_or(false),
            expires: c.expires_datetime().map(|dt| dt.unix_timestamp()),
        })
        .collect())
}

/// 从当前内嵌浏览器抓 cookie 写到指定账号（前端需先 browser_open 该 profile）。
pub async fn recapture_account_session(
    app: &AppHandle,
    account_id: &str,
    profile_id: &str,
) -> Result<WebSession, String> {
    let cookies = capture_cookies(app).await.unwrap_or_default();
    let filtered = web_session::filter_zed_cookies(cookies);
    let session = WebSession {
        profile_id: profile_id.to_string(),
        cookies: filtered,
        captured_at: Some(chrono::Utc::now().timestamp()),
    };
    web_session::save_session(account_id, &session)?;
    Ok(session)
}
