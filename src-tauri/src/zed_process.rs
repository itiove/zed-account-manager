//! 负责控制本机 Zed 进程本身：查找安装路径、判断是否在运行、退出、
//! 重新启动、聚焦窗口。
//!
//! 设计参考 cockpit-tools 对 Zed 的真实实现（`zed_instance.rs`）：
//! 账号切换靠“写 Keychain + 退出重启 Zed”而不是模拟 SignOut/SignIn 快捷键。
//! 原因是快捷键模拟依赖 macOS「辅助功能」权限，这个权限在未签名的开发调试
//! 二进制上非常不稳定（每次重新编译都可能被系统悄悄收回，导致
//! `System Events`报 "不允许发送按键" 的 1002 错误），而“退出 App + `open`
//! 重新启动”只需要一次性的「自动化」授权（针对 Zed 这个目标 App，
//! friction 小得多），且不依赖任何 UI 脚本时序，更稳定可靠。
//!
//! 代价：账号切换会真正重启 Zed 进程，依赖 Zed 自身的会话恢复
//! （默认会记住上次打开的工作区）来找回窗口状态，而不是完全无重启。

use std::path::PathBuf;
use std::time::{Duration, Instant};

#[cfg(target_os = "macos")]
const PROCESS_NAME: &str = "zed";

#[cfg(target_os = "macos")]
fn run(program: &str, args: &[&str]) -> std::io::Result<std::process::Output> {
    std::process::Command::new(program).args(args).output()
}

/// 查找本机 Zed.app 的安装路径，用于 `open` 命令启动。
#[cfg(target_os = "macos")]
pub fn find_app_path() -> Result<PathBuf, String> {
    let candidates = [
        "/Applications/Zed.app",
        "/Applications/Zed Preview.app",
    ];
    for candidate in candidates {
        let path = PathBuf::from(candidate);
        if path.exists() {
            return Ok(path);
        }
    }
    if let Some(home) = dirs::home_dir() {
        let path = home.join("Applications").join("Zed.app");
        if path.exists() {
            return Ok(path);
        }
    }
    // 通过 Spotlight 索引兜底查找（用户可能装在自定义位置）。
    if let Ok(output) = run(
        "mdfind",
        &["kMDItemCFBundleIdentifier == 'dev.zed.Zed'"],
    ) {
        if let Some(first_line) = String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .map(|s| s.trim().to_string())
        {
            if !first_line.is_empty() {
                return Ok(PathBuf::from(first_line));
            }
        }
    }
    Err("未找到 Zed.app，请确认已安装 Zed（默认路径 /Applications/Zed.app）".to_string())
}

#[cfg(target_os = "macos")]
pub fn is_running() -> bool {
    run("pgrep", &["-x", PROCESS_NAME])
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// 优雅退出：给 Zed 发送 AppleEvent quit（相当于 Cmd+Q），
/// 只需要用户对本 App 授予一次「自动化 -> 允许控制 Zed」权限，
/// 不涉及「辅助功能」。
#[cfg(target_os = "macos")]
fn graceful_quit() {
    let _ = run("osascript", &["-e", "tell application \"Zed\" to quit"]);
}

/// 兜底强杀，用于优雅退出超时后仍未退出的情况。
#[cfg(target_os = "macos")]
fn force_kill() {
    let _ = run("pkill", &["-x", PROCESS_NAME]);
}

#[cfg(target_os = "macos")]
fn wait_until_stopped(timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !is_running() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    !is_running()
}

#[cfg(target_os = "macos")]
pub fn focus() -> Result<(), String> {
    let output = run("osascript", &["-e", "tell application \"Zed\" to activate"])
        .map_err(|e| format!("执行 Zed activate 失败: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "激活 Zed 失败: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn launch(app_path: &PathBuf) -> Result<(), String> {
    let output = run("open", &["-a", &app_path.to_string_lossy()])
        .map_err(|e| format!("启动 Zed 失败: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "启动 Zed 失败: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

/// 完整的“让新 Keychain 凭据生效”动作：退出 Zed（如果在运行）-> 重新启动 -> 聚焦。
/// 调用前必须已经把新账号的凭据写入 Keychain。
#[cfg(target_os = "macos")]
pub fn restart_to_apply_credentials() -> Result<(), String> {
    let app_path = find_app_path()?;
    let was_running = is_running();

    if was_running {
        graceful_quit();
        if !wait_until_stopped(Duration::from_secs(5)) {
            force_kill();
            wait_until_stopped(Duration::from_secs(3));
        }
        // 给系统一点喘息时间再拉起，避免和刚退出的进程资源抢占冲突。
        std::thread::sleep(Duration::from_millis(400));
    }

    launch(&app_path)?;
    // 启动是异步的，稍等一下再尝试聚焦，避免窗口还没创建出来。
    std::thread::sleep(Duration::from_millis(900));
    let _ = focus();
    Ok(())
}

/// 仅用于登出场景：同样需要重启 Zed，否则内存里已经登录的状态不会消失。
#[cfg(target_os = "macos")]
pub fn restart_to_clear_credentials() -> Result<(), String> {
    restart_to_apply_credentials()
}

#[cfg(not(target_os = "macos"))]
pub fn restart_to_apply_credentials() -> Result<(), String> {
    Err("目前仅支持 macOS".to_string())
}

#[cfg(not(target_os = "macos"))]
pub fn restart_to_clear_credentials() -> Result<(), String> {
    Err("目前仅支持 macOS".to_string())
}
