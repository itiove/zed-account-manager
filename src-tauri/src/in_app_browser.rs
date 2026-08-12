//! 主窗口内嵌隔离浏览器（不新建窗口）。
//!
//! 布局策略（解决工具栏被盖住）：
//! - 打开浏览器时：把主 WebView（Vue）**缩小到仅工具栏高度**
//! - 内容子 WebView 占窗口剩余下半部分
//! - 两块区域不重叠，从根上避免 z-order 遮挡
//! - 关闭时恢复主 WebView 为全窗口

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::webview::{PageLoadEvent, WebviewBuilder};
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl, WindowEvent,
};
use url::Url;

use crate::web_session;

pub const CONTENT_LABEL: &str = "in-app-browser";
const MAIN_WINDOW_LABEL: &str = "main";
/// 工具栏固定高度（逻辑像素）。
/// 与前端 BrowserPanel.vue 的 CHROME_HEIGHT / --chrome-height 保持一致。
/// 后端强制使用该常量分割主 WebView 与内容 WebView，
/// 前端传入的任何高度都会被忽略，避免旧代码/测量值造成遮挡。
const CHROME_HEIGHT: f64 = 80.0;
/// 底部凭据栏固定高度（逻辑像素），与前端 BrowserPanel.vue 的 CREDS_BAR_HEIGHT 保持一致。
/// 仅当 browser_open 传入 bottom_bar=true 时预留。
const CREDS_BAR_HEIGHT: f64 = 54.0;

/// 兼容旧前端仍传 bounds 的情况（内容一律忽略）
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserBounds {
    #[allow(dead_code)]
    pub x: f64,
    #[allow(dead_code)]
    pub y: f64,
    #[allow(dead_code)]
    pub width: f64,
    #[allow(dead_code)]
    pub height: f64,
}

#[derive(Debug, Clone)]
struct ActiveBrowser {
    profile_id: String,
    /// 是否在窗口底部预留凭据栏区域
    bottom_bar: bool,
}

static ACTIVE: std::sync::LazyLock<Mutex<Option<ActiveBrowser>>> =
    std::sync::LazyLock::new(|| Mutex::new(None));

static RESIZE_HOOKED: std::sync::LazyLock<Mutex<bool>> =
    std::sync::LazyLock::new(|| Mutex::new(false));

/// resize 防抖序号：只有最后一次 Resized 事件对应的延迟任务才执行重排
static RESIZE_SEQ: AtomicU64 = AtomicU64::new(0);

/// 按当前状态重排：浏览器打开时分割布局，否则恢复主 WebView 全屏
fn apply_layout(app: &AppHandle) {
    let active = ACTIVE.lock().ok().map(|g| g.is_some()).unwrap_or(false);
    if active {
        let _ = layout_split(app);
    } else {
        let _ = restore_main_webview(app);
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UrlChangedPayload {
    url: String,
}

fn get_main_window(app: &AppHandle) -> Result<tauri::Window, String> {
    if let Some(w) = app.get_window(MAIN_WINDOW_LABEL) {
        return Ok(w);
    }
    if let Some(wv) = app.get_webview(MAIN_WINDOW_LABEL) {
        return Ok(wv.window());
    }
    Err("找不到主窗口 main".into())
}

fn window_logical_size(app: &AppHandle) -> Result<(f64, f64), String> {
    let window = get_main_window(app)?;
    let scale = window.scale_factor().unwrap_or(1.0).max(0.5);
    let physical = window
        .inner_size()
        .map_err(|e| format!("读取窗口尺寸失败: {e}"))?;
    Ok((
        f64::from(physical.width) / scale,
        f64::from(physical.height) / scale,
    ))
}

/// 当前应预留的底部凭据栏高度
fn bottom_bar_height() -> f64 {
    let with_bar = ACTIVE
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|a| a.bottom_bar))
        .unwrap_or(false);
    if with_bar {
        CREDS_BAR_HEIGHT
    } else {
        0.0
    }
}

/// 布局策略：主 WebView 铺满整个窗口（顶部渲染工具栏、底部渲染凭据栏），
/// 内容 WebView 作为子视图叠在中间区域（子视图 z-order 恒在主 WebView 之上）。
fn layout_split(app: &AppHandle) -> Result<(), String> {
    let (win_w, win_h) = window_logical_size(app)?;
    let bar_h = bottom_bar_height();
    let content_h = (win_h - CHROME_HEIGHT - bar_h).max(120.0);

    if let Some(main) = app.get_webview(MAIN_WINDOW_LABEL) {
        main.set_position(LogicalPosition::new(0.0, 0.0))
            .map_err(|e| format!("主 WebView 定位失败: {e}"))?;
        main.set_size(LogicalSize::new(win_w, win_h))
            .map_err(|e| format!("主 WebView 尺寸失败: {e}"))?;
    }

    if let Some(content) = app.get_webview(CONTENT_LABEL) {
        content
            .set_position(LogicalPosition::new(0.0, CHROME_HEIGHT))
            .map_err(|e| format!("内容 WebView 定位失败: {e}"))?;
        content
            .set_size(LogicalSize::new(win_w, content_h))
            .map_err(|e| format!("内容 WebView 尺寸失败: {e}"))?;
    }

    Ok(())
}

/// 恢复主 WebView 铺满窗口。
fn restore_main_webview(app: &AppHandle) -> Result<(), String> {
    let (win_w, win_h) = window_logical_size(app)?;
    if let Some(main) = app.get_webview(MAIN_WINDOW_LABEL) {
        main.set_position(LogicalPosition::new(0.0, 0.0))
            .map_err(|e| format!("恢复主 WebView 位置失败: {e}"))?;
        main.set_size(LogicalSize::new(win_w, win_h))
            .map_err(|e| format!("恢复主 WebView 尺寸失败: {e}"))?;
    }
    Ok(())
}

fn ensure_resize_hook(app: &AppHandle) {
    let mut hooked = RESIZE_HOOKED.lock().unwrap();
    if *hooked {
        return;
    }
    let Ok(window) = get_main_window(app) else {
        return;
    };
    let app_handle = app.clone();
    window.on_window_event(move |event| {
        if !matches!(event, WindowEvent::Resized(_)) {
            return;
        }
        // 立即重排一次，随后做防抖兜底：拖拽调整大小期间，系统 autoresizing、
        // Tauri 内部调整与本钩子会竞争子 WebView 的位置，最终顺序无保证。
        // 延迟后仅让"最后一次"事件再执行一遍 layout，确保我们的布局最终落地。
        apply_layout(&app_handle);
        let seq = RESIZE_SEQ.fetch_add(1, Ordering::SeqCst) + 1;
        let handle = app_handle.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(140));
            if RESIZE_SEQ.load(Ordering::SeqCst) == seq {
                apply_layout(&handle);
            }
        });
    });
    *hooked = true;
}

pub fn normalize_url(input: &str) -> Result<Url, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("地址不能为空".into());
    }
    if let Ok(url) = Url::parse(trimmed) {
        if url.scheme() == "http" || url.scheme() == "https" {
            return Ok(url);
        }
    }
    Url::parse(&format!("https://{trimmed}")).map_err(|e| format!("无效地址: {e}"))
}

/// 打开内嵌浏览器：主 WebView 铺满窗口（顶部工具栏 + 可选底部凭据栏），
/// 内容 WebView 作为子视图叠在中间区域。
/// `chrome_height` / `bounds` 参数仅为兼容旧前端保留，实际一律使用固定 CHROME_HEIGHT。
#[tauri::command]
pub fn browser_open(
    app: AppHandle,
    url: String,
    profile_id: String,
    #[allow(unused_variables)] chrome_height: Option<f64>,
    #[allow(unused_variables)] bounds: Option<BrowserBounds>,
    bottom_bar: Option<bool>,
) -> Result<(), String> {
    let initial = normalize_url(&url)?;
    let profile_id = profile_id.trim().to_string();
    if profile_id.is_empty() {
        return Err("profile_id 不能为空".into());
    }
    let bottom_bar = bottom_bar.unwrap_or(false);

    ensure_resize_hook(&app);

    if let Some(existing) = app.get_webview(CONTENT_LABEL) {
        let same_profile = ACTIVE
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|a| a.profile_id == profile_id))
            .unwrap_or(false);
        if same_profile {
            existing
                .navigate(initial)
                .map_err(|e| format!("导航失败: {e}"))?;
            *ACTIVE.lock().unwrap() = Some(ActiveBrowser {
                profile_id,
                bottom_bar,
            });
            layout_split(&app)?;
            let _ = existing.show();
            return Ok(());
        }
        let _ = existing.close();
        std::thread::sleep(std::time::Duration::from_millis(80));
    }

    *ACTIVE.lock().unwrap() = Some(ActiveBrowser {
        profile_id: profile_id.clone(),
        bottom_bar,
    });
    layout_split(&app)?;

    let window = get_main_window(&app)?;
    let (win_w, win_h) = window_logical_size(&app)?;
    let content_h = (win_h - CHROME_HEIGHT - bottom_bar_height()).max(120.0);

    let store_id = web_session::profile_id_to_store_bytes(&profile_id);
    let profile_dir: PathBuf = web_session::profile_data_dir(&profile_id)?;

    let builder = WebviewBuilder::new(CONTENT_LABEL, WebviewUrl::External(initial))
        .data_store_identifier(store_id)
        .data_directory(profile_dir)
        .on_page_load(move |webview, payload| {
            if !matches!(payload.event(), PageLoadEvent::Finished) {
                return;
            }
            let page_url = payload.url().to_string();
            let app = webview.app_handle().clone();
            let _ = app.emit_to(
                MAIN_WINDOW_LABEL,
                "browser-url-changed",
                UrlChangedPayload { url: page_url },
            );
        });

    window
        .add_child(
            builder,
            LogicalPosition::new(0.0, CHROME_HEIGHT),
            LogicalSize::new(win_w, content_h),
        )
        .map_err(|e| format!("挂载内容 WebView 失败: {e}"))?;

    // 再 layout 一次确保对齐
    layout_split(&app)?;
    Ok(())
}

/// 兼容旧前端：高度固定，收到请求只做一次重新分区。
#[tauri::command]
pub fn browser_set_chrome_height(
    app: AppHandle,
    #[allow(unused_variables)] chrome_height: f64,
) -> Result<(), String> {
    if ACTIVE.lock().ok().and_then(|g| g.clone()).is_none() {
        return Ok(());
    }
    layout_split(&app)
}

/// 兼容旧前端 `browser_set_bounds`：高度固定，只做一次重新分区。
#[tauri::command]
pub fn browser_set_bounds(
    app: AppHandle,
    #[allow(unused_variables)] bounds: BrowserBounds,
) -> Result<(), String> {
    browser_set_chrome_height(app, CHROME_HEIGHT)
}

#[tauri::command]
pub fn browser_close(app: AppHandle) -> Result<(), String> {
    if let Some(wv) = app.get_webview(CONTENT_LABEL) {
        let _ = wv.close();
    }
    *ACTIVE.lock().unwrap() = None;
    restore_main_webview(&app)?;
    Ok(())
}

fn get_content(app: &AppHandle) -> Result<tauri::Webview, String> {
    app.get_webview(CONTENT_LABEL)
        .ok_or_else(|| "内嵌浏览器未打开".to_string())
}

#[tauri::command]
pub fn browser_back(app: AppHandle) -> Result<(), String> {
    get_content(&app)?
        .eval("window.history.back()")
        .map_err(|e| format!("后退失败: {e}"))
}

#[tauri::command]
pub fn browser_forward(app: AppHandle) -> Result<(), String> {
    get_content(&app)?
        .eval("window.history.forward()")
        .map_err(|e| format!("前进失败: {e}"))
}

#[tauri::command]
pub fn browser_reload(app: AppHandle) -> Result<(), String> {
    get_content(&app)?
        .reload()
        .map_err(|e| format!("刷新失败: {e}"))
}

#[tauri::command]
pub fn browser_goto(app: AppHandle, url: String) -> Result<String, String> {
    let content = get_content(&app)?;
    let parsed = normalize_url(&url)?;
    let display = parsed.to_string();
    content
        .navigate(parsed)
        .map_err(|e| format!("导航失败: {e}"))?;
    Ok(display)
}

#[tauri::command]
pub fn browser_current_url(app: AppHandle) -> Result<String, String> {
    get_content(&app)?
        .url()
        .map(|u| u.to_string())
        .map_err(|e| format!("读取地址失败: {e}"))
}

pub fn navigate_content(app: &AppHandle, url: &str) -> Result<(), String> {
    let content = get_content(app)?;
    let parsed = normalize_url(url)?;
    content
        .navigate(parsed)
        .map_err(|e| format!("导航失败: {e}"))
}

pub fn content_webview(app: &AppHandle) -> Option<tauri::Webview> {
    app.get_webview(CONTENT_LABEL)
}

pub fn close_if_open(app: &AppHandle) {
    let _ = browser_close(app.clone());
}
