// 防止 Windows release 构建下额外弹出一个控制台窗口。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    zed_account_manager_lib::run();
}
