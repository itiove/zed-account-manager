mod accounts;
mod commands;
mod in_app_browser;
mod keychain;
mod oauth;
mod web_session;
mod zed_process;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::list_accounts,
            commands::login_start,
            commands::login_cancel,
            commands::login_poll,
            commands::switch_account,
            commands::refresh_quota,
            commands::prepare_account_browser,
            commands::recapture_web_session,
            commands::logout_current,
            commands::remove_account,
            in_app_browser::browser_open,
            in_app_browser::browser_set_bounds,
            in_app_browser::browser_set_chrome_height,
            in_app_browser::browser_close,
            in_app_browser::browser_back,
            in_app_browser::browser_forward,
            in_app_browser::browser_reload,
            in_app_browser::browser_goto,
            in_app_browser::browser_current_url,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
