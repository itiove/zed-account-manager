# Zed Account Manager（Tauri 2 + Vue 3）

独立的桌面小应用：管理多个 Zed 账号（OAuth 登录、Keychain 存储、额度查看），
点击即可在**不重启 Zed** 的情况下切换账号。不依赖 cockpit-tools，也不是 Zed
扩展（Zed 官方扩展系统目前没有开放自定义 UI 面板的接口，详见下方「设计背景」）。

## 架构

```
Vue 3 UI (src/)                 Tauri command (src-tauri/src/commands.rs)
  AccountCard.vue  ──invoke──►     list_accounts / login_start / login_poll
  App.vue                          switch_account / refresh_quota
                                    open_account_browser / recapture_web_session
                                    logout_current / remove_account
                                          │
                                          ▼
        ┌──────────────────────────────────────────────────────────────┐
        │ oauth.rs        内置隔离 WebView + native_app_signin 回调     │
        │ web_session.rs  每账号 cookie 快照 + billing/usage 额度接口   │
        │ keychain.rs     读写 macOS Keychain（与 Zed 同一条记录）       │
        │ accounts.rs     本地账号池 + /client/users/me 合并刷新         │
        │ zed_process.rs  退出 + 重启 Zed 进程，让新凭据生效             │
        └──────────────────────────────────────────────────────────────┘
```

## 主窗口内嵌浏览器 + 每账号会话隔离

**不再新建浏览器窗口**。登录 / 网页会话都在主窗口覆盖层完成：

```
主窗口 (main)
├── Vue UI：账号列表 / 工具栏(地址栏、前进后退刷新)
└── 子 WebView `in-app-browser`（叠在内容槽位上）
      └── 独立 profile_id → data_store_identifier + browser_profiles/<id>/
```

流程：

1. `login_start` 只起 OAuth 回调，返回 `verification_uri` + `profile_id`
2. 前端打开 `BrowserPanel`，调用 `browser_open(url, profile_id, bounds)`
3. Rust 在主窗口 `add_child` 挂载隔离内容 WebView
4. 登录成功后抓 `zed.session`，写入 `web_sessions/<account_id>.json`
5. `browser_close` 销毁子 WebView，回到账号列表

刷新额度：`/client/users/me` + 可选 `/frontend/billing/usage`（有 cookie 时）。

## "无感换号"的原理：参考 cockpit-tools 对 Zed 的真实实现

最初设计想用 Zed 内置的 `client::SignOut` / `client::SignIn` action 加快捷键模拟来
实现真正的"进程不重启"。但实际验证后发现这条路很脆弱：模拟按键需要 macOS
“辅助功能”权限，而这个权限在未签名的开发调试二进制上非常不稳定（每次重新
编译都可能被系统悔回权限，导致 `System Events` 报 "不允许发送按键"）。

因此改为参考 cockpit-tools 对 Zed 的真实实现（`src-tauri/src/modules/zed_instance.rs`
的 `restart_default_session`）：**写 Keychain 后退出 Zed 进程并重新启动**，而不是模拟
快捷键。好处：

- 退出用 `osascript -e 'tell application "Zed" to quit'`，发送的是普通 Apple Event，
  只需要一次性的“自动化 -> 允许控制 Zed”授权，不涉及辅助功能，权限
  提示简单且稳定；
- 重新启动用 `open -a Zed`，不需要任何特殊权限；
- 彻底绕开了“模拟按键时序”这类不稳定因素。

代价：确实会重启 Zed 进程（不是严格意义上的“进程不重启”），依靠 Zed 自带的
会话恢复（默认会记住上次打开的工作区）找回窗口状态。这个取舍是有意为之——
稳定可靠 > 形式上完美的“无重启”，与 cockpit-tools 对其他 IDE（Cursor/Windsurf 等）
的处理思路一致。

点击"切换账号"时，内部按顺序执行（`zed_process.rs::restart_to_apply_credentials`）：

1. 把目标账号凭据写入系统 Keychain（`service = https://zed.dev`，和 Zed
   官方客户端读取的是同一条记录）；
2. 如果 Zed 正在运行，先尝试优雅退出，5 秒内没退干净就 `pkill` 强制结束；
3. `open -a Zed` 重新拉起，稍等窗口创建后自动 `activate` 拉到前台。

用户全程只需要点一次"切换账号"按钮，不需要知道背后的实现细节。

## 开发调试

```bash
npm install
npm run tauri dev
```

首次点击"切换账号"/"退出登录"时，macOS 会弹一次"自动化"权限请求
（“'Zed Account Manager' 想控制 'Zed'”），点允许即可，之后不会再弹。这个权限
比之前需要的“辅助功能”稳定得多，不会因为重新编译而失效。

## 打包

```bash
npm run tauri build
```

打包前建议用一张正式 Logo 替换占位图标：

```bash
npm run tauri icon path/to/logo.png
```

## 已知限制

- 目前只实现了 macOS（Keychain 用 `security` 命令行工具，进程控制用 `osascript` +
  `open`/`pgrep`/`pkill`）。Windows/Linux 需要分别实现凭据存储（Windows 凭据管理器 /
  libsecret）和进程控制（直接用 `taskkill`/`Start-Process` 或 `pkill`/`xdg-open` 类似物）。
- 切换账号会真实重启 Zed 进程，会短暂关闭并重新打开窗口，工作区恢复依赖 Zed
  自带的会话恢复能力。
- 长期更优雅的方案是给 Zed 提 PR，开放一个真正的进程内 `client::SignIn` 编程
  接口（比如 CLI 子命令或 IPC），那样才能真正做到不重启进程就刷新登录态。

## 设计背景：为什么不做成 Zed 扩展

Zed 的 WASM 扩展 API（`crates/extension_api/wit/`）目前没有任何自定义面板/
按钮/点击 UI 的 host 接口，唯一能被用户主动触发的方式是 Agent 面板对话里
调用 MCP Context Server 工具，这是"聊天式"交互，不是"点击式"。本应用的
需求是清晰的点击式操作体验，所以选择做成一个独立的原生桌面应用，而不是
受限于当前 Zed 扩展系统能力的扩展包。
