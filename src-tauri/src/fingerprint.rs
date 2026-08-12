//! Per-profile 稳定浏览器指纹。
//!
//! 用 `profile_id` 做种子确定性地派生一套 UA + navigator/screen 属性：
//! 同一账号每次打开都一致，不同账号之间互不相同，模拟「多个真实用户」的画像。
//! 注意：这里刻意做「稳定」而非「随机」——正常用户的硬件指纹不会每次访问都变，
//! 每次随机反而更可疑。

/// 由 profile_id 派生的一套稳定指纹。
pub struct Fingerprint {
    pub user_agent: String,
    hardware_concurrency: u32,
    device_memory: u32,
    screen_w: u32,
    screen_h: u32,
    languages_js: String,
}

/// 简单确定性哈希（FNV-1a 64bit），无需引入额外依赖。
fn seed_hash(profile_id: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in profile_id.as_bytes() {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// 从种子里按位取一个候选值。
fn pick<'a, T>(list: &'a [T], seed: u64, shift: u32) -> &'a T {
    let idx = ((seed >> shift) as usize) % list.len();
    &list[idx]
}

impl Fingerprint {
    pub fn derive(profile_id: &str) -> Self {
        let seed = seed_hash(profile_id);

        // macOS 版本 + Safari 版本组合（保持与 WKWebView 真实内核相符，避免 UA 与内核矛盾）。
        let macos_versions = ["10_15_7", "13_6", "14_5", "14_6", "15_1"];
        let safari_versions = ["16.6", "17.4.1", "17.5", "17.6", "18.1"];
        let webkit_versions = ["605.1.15", "618.3.11", "619.1.26"];

        let macos = pick(&macos_versions, seed, 0);
        let safari = pick(&safari_versions, seed, 8);
        let webkit = pick(&webkit_versions, seed, 16);

        let user_agent = format!(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X {macos}) AppleWebKit/{webkit} (KHTML, like Gecko) Version/{safari} Safari/{webkit}"
        );

        // 常见 Mac 的逻辑核心数 / 内存档位。
        let cores = [8u32, 10, 12, 14, 16];
        let mems = [8u32, 16, 32];
        let hardware_concurrency = *pick(&cores, seed, 24);
        let device_memory = *pick(&mems, seed, 32);

        // 常见 Mac 显示分辨率（逻辑像素）。
        let screens = [
            (1440u32, 900u32),
            (1512, 982),
            (1680, 1050),
            (1728, 1117),
            (1920, 1080),
            (2560, 1440),
        ];
        let (screen_w, screen_h) = *pick(&screens, seed, 40);

        // 语言组合。
        let langs = [
            r#"["zh-CN","zh","en-US","en"]"#,
            r#"["zh-CN","zh"]"#,
            r#"["en-US","en","zh-CN"]"#,
        ];
        let languages_js = pick(&langs, seed, 48).to_string();

        Self {
            user_agent,
            hardware_concurrency,
            device_memory,
            screen_w,
            screen_h,
            languages_js,
        }
    }

    /// 生成页面加载前注入的初始化脚本，覆盖易被指纹采集的 navigator/screen 属性。
    pub fn init_script(&self) -> String {
        format!(
            r#"(function() {{
  try {{
    const def = (obj, prop, val) => {{
      try {{ Object.defineProperty(obj, prop, {{ get: () => val, configurable: true }}); }} catch (e) {{}}
    }};
    def(navigator, 'hardwareConcurrency', {cores});
    def(navigator, 'deviceMemory', {mem});
    def(navigator, 'languages', Object.freeze({langs}));
    def(navigator, 'language', {langs}[0]);
    def(screen, 'width', {sw});
    def(screen, 'height', {sh});
    def(screen, 'availWidth', {sw});
    def(screen, 'availHeight', {sh} - 25);
  }} catch (e) {{}}
}})();"#,
            cores = self.hardware_concurrency,
            mem = self.device_memory,
            langs = self.languages_js,
            sw = self.screen_w,
            sh = self.screen_h,
        )
    }
}
