//! 直接操作 macOS 系统 Keychain 里 Zed 官方客户端使用的那条 Internet Password 记录。
//!
//! 这和 Zed 自己的 `zed_credentials_provider::KeychainCredentialsProvider` 走的是
//! 同一个底层存储位置：service = ClientSettings.server_url（默认
//! "https://zed.dev"），account = user_id，password = access_token。
//! 这里不经过 GPUI 的 `cx.write_credentials`，而是直接调用 macOS `security`
//! 命令行工具，因为我们是一个独立于 Zed 主进程之外运行的原生进程。

#[cfg(target_os = "macos")]
use std::process::Command;

#[cfg(target_os = "macos")]
fn run_security(args: &[&str]) -> Result<std::process::Output, String> {
    Command::new("security")
        .args(args)
        .output()
        .map_err(|e| format!("执行 security 命令失败: {e}"))
}

#[cfg(target_os = "macos")]
pub fn delete_credentials(service: &str) -> Result<(), String> {
    loop {
        let output = run_security(&["delete-internet-password", "-s", service])?;
        if output.status.success() {
            continue;
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("could not be found") {
            return Ok(());
        }
        return Err(format!(
            "删除 Keychain 凭据失败: status={}, stderr={}",
            output.status,
            stderr.trim()
        ));
    }
}

#[cfg(target_os = "macos")]
pub fn write_credentials(service: &str, user_id: &str, access_token: &str) -> Result<(), String> {
    delete_credentials(service)?;
    let output = run_security(&[
        "add-internet-password",
        "-U",
        "-a",
        user_id,
        "-s",
        service,
        "-w",
        access_token,
    ])?;
    if !output.status.success() {
        return Err(format!(
            "写入 Keychain 凭据失败: status={}, stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
#[allow(dead_code)]
fn parse_account_from_output(text: &str) -> Option<String> {
    for line in text.lines() {
        if let Some(rest) = line.split("\"acct\"<blob>=\"").nth(1) {
            if let Some(value) = rest.split('"').next() {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }
    None
}

#[cfg(target_os = "macos")]
#[allow(dead_code)]
pub fn read_credentials(service: &str) -> Result<Option<(String, String)>, String> {
    let meta = run_security(&["find-internet-password", "-s", service])?;
    if !meta.status.success() {
        let stderr = String::from_utf8_lossy(&meta.stderr);
        if stderr.contains("could not be found") {
            return Ok(None);
        }
        return Err(format!(
            "读取 Keychain 元数据失败: status={}, stderr={}",
            meta.status,
            stderr.trim()
        ));
    }
    let password_output = run_security(&["find-internet-password", "-s", service, "-w"])?;
    if !password_output.status.success() {
        return Err("读取 Keychain 密码失败".to_string());
    }

    let meta_text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&meta.stdout),
        String::from_utf8_lossy(&meta.stderr)
    );
    let user_id = parse_account_from_output(&meta_text)
        .ok_or_else(|| "解析 Keychain 账号失败".to_string())?;
    let access_token = String::from_utf8_lossy(&password_output.stdout)
        .trim()
        .to_string();
    if access_token.is_empty() {
        return Ok(None);
    }
    Ok(Some((user_id, access_token)))
}

#[cfg(not(target_os = "macos"))]
pub fn delete_credentials(_service: &str) -> Result<(), String> {
    Err("账号切换目前仅支持 macOS".to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn write_credentials(
    _service: &str,
    _user_id: &str,
    _access_token: &str,
) -> Result<(), String> {
    Err("账号切换目前仅支持 macOS".to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn read_credentials(_service: &str) -> Result<Option<(String, String)>, String> {
    Err("账号切换目前仅支持 macOS".to_string())
}
