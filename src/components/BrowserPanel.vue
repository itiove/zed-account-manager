<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { Icon } from "@iconify/vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { computeTotp } from "../totp";
import type { Credentials } from "./CredentialsDialog.vue";

/**
 * 工具栏固定高度（逻辑 px），与 Rust 端 CHROME_HEIGHT 常量一致。
 * 单行布局实际内容只占顶部 ~46px，底部留有较大安全余量，
 * 即使原生分割边界有偏差也不会切到控件。
 */
const CHROME_HEIGHT = 80;

const props = defineProps<{
  title: string;
  profileId: string;
  initialUrl: string;
  hint?: string;
  showSync?: boolean;
  credentials?: Credentials | null;
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "sync"): void;
}>();

const urlInput = ref("");
const loading = ref(false);
const syncing = ref(false);

const hasCreds = computed(
  () => !!props.credentials && !!props.credentials.username,
);
const hasSecret = computed(() => !!props.credentials?.secret);

let unlisten: UnlistenFn | null = null;

async function openBrowser() {
  loading.value = true;
  try {
    await invoke("browser_open", {
      url: props.initialUrl,
      profileId: props.profileId,
      chromeHeight: CHROME_HEIGHT,
      bottomBar: hasCreds.value,
    });
    urlInput.value = props.initialUrl;
  } catch (e) {
    console.error(e);
  } finally {
    loading.value = false;
  }
}

/* ── 底部凭据栏：复制 + 实时 2FA ─────────── */
const totpCode = ref("------");
const totpRemaining = ref(30);
const totpPeriod = ref(30);
const totpError = ref(false);
const copied = ref<"username" | "password" | "totp" | null>(null);
let totpTimer: ReturnType<typeof setInterval> | null = null;
let copiedTimer: ReturnType<typeof setTimeout> | null = null;

const totpDisplay = computed(() => {
  if (totpError.value) return "无效密钥";
  const c = totpCode.value;
  return c.length === 6 ? `${c.slice(0, 3)} ${c.slice(3)}` : c;
});
const ringDash = computed(() => {
  const frac = totpRemaining.value / totpPeriod.value;
  const circ = 2 * Math.PI * 9;
  return `${circ * frac} ${circ}`;
});

async function refreshTotp() {
  const secret = props.credentials?.secret;
  if (!secret) return;
  try {
    const r = await computeTotp(secret);
    totpCode.value = r.code;
    totpRemaining.value = r.secondsRemaining;
    totpPeriod.value = r.period;
    totpError.value = false;
  } catch {
    totpError.value = true;
    totpCode.value = "------";
  }
}

async function copyField(field: "username" | "password" | "totp") {
  const map = {
    username: props.credentials?.username ?? "",
    password: props.credentials?.password ?? "",
    totp: totpCode.value,
  };
  const text = map[field];
  if (!text) return;
  try {
    await navigator.clipboard.writeText(text);
    copied.value = field;
    if (copiedTimer) clearTimeout(copiedTimer);
    copiedTimer = setTimeout(() => (copied.value = null), 1400);
  } catch {
    /* ignore */
  }
}

async function closeBrowser() {
  try {
    await invoke("browser_close");
  } catch {
    /* ignore */
  }
}

async function navBack() {
  try {
    await invoke("browser_back");
  } catch {
    /* ignore */
  }
}
async function navForward() {
  try {
    await invoke("browser_forward");
  } catch {
    /* ignore */
  }
}
async function navReload() {
  try {
    await invoke("browser_reload");
  } catch {
    /* ignore */
  }
}
async function navHome() {
  try {
    const next = await invoke<string>("browser_goto", {
      url: "https://dashboard.zed.dev/",
    });
    urlInput.value = next;
  } catch {
    /* ignore */
  }
}
async function navGoto() {
  const raw = urlInput.value.trim();
  if (!raw) return;
  try {
    const next = await invoke<string>("browser_goto", { url: raw });
    urlInput.value = next;
  } catch {
    /* ignore */
  }
}

function onClose() {
  void closeBrowser().then(() => emit("close"));
}

async function onSync() {
  if (syncing.value) return;
  syncing.value = true;
  try {
    emit("sync");
  } finally {
    // 视觉反馈用的短暂 loading，实际同步进度由父组件 toast 提示
    window.setTimeout(() => (syncing.value = false), 1200);
  }
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === "Escape") {
    onClose();
    return;
  }
  const meta = e.metaKey || e.ctrlKey;
  if (meta && e.key.toLowerCase() === "r") {
    e.preventDefault();
    void navReload();
  } else if (meta && e.key === "[") {
    e.preventDefault();
    void navBack();
  } else if (meta && e.key === "]") {
    e.preventDefault();
    void navForward();
  } else if (meta && e.key.toLowerCase() === "l") {
    e.preventDefault();
    const input = document.getElementById("in-app-url") as HTMLInputElement | null;
    input?.focus();
    input?.select();
  }
}

onMounted(async () => {
  window.scrollTo(0, 0);
  document.body.style.overflow = "hidden";
  window.addEventListener("keydown", onKeydown);

  unlisten = await listen<{ url: string }>("browser-url-changed", (ev) => {
    urlInput.value = ev.payload.url;
  });

  if (hasSecret.value) {
    await refreshTotp();
    totpTimer = setInterval(refreshTotp, 1000);
  }

  await openBrowser();
});

onUnmounted(() => {
  document.body.style.overflow = "";
  window.removeEventListener("keydown", onKeydown);
  if (unlisten) void unlisten();
  if (totpTimer) clearInterval(totpTimer);
  if (copiedTimer) clearTimeout(copiedTimer);
  void closeBrowser();
});

watch(
  () => [props.profileId, props.initialUrl],
  async () => {
    await openBrowser();
  },
);
</script>

<template>
  <!--
    主 WebView 只显示这条固定高度的单行工具栏，
    下方登录页由原生内容 WebView 渲染，两块区域按同一常量分割，互不重叠。
  -->
  <header class="browser-chrome">
    <div class="chrome-row">
      <div class="btn-group">
        <button class="nav-btn" type="button" title="后退 (⌘[)" @click="navBack">
          <Icon icon="lucide:arrow-left" style="font-size: 14px" />
        </button>
        <button class="nav-btn" type="button" title="前进 (⌘])" @click="navForward">
          <Icon icon="lucide:arrow-right" style="font-size: 14px" />
        </button>
        <button class="nav-btn" type="button" title="刷新 (⌘R)" @click="navReload">
          <Icon icon="lucide:rotate-cw" style="font-size: 13px" />
        </button>
        <button class="nav-btn" type="button" title="回到 Zed Dashboard" @click="navHome">
          <Icon icon="lucide:home" style="font-size: 14px" />
        </button>
      </div>

      <form class="url-form" @submit.prevent="navGoto">
        <span class="lock" :class="{ secure: urlInput.startsWith('https://') }">
          <Icon
            :icon="urlInput.startsWith('https://') ? 'lucide:lock' : 'lucide:globe'"
            style="font-size: 13px"
          />
        </span>
        <input
          id="in-app-url"
          v-model="urlInput"
          type="text"
          spellcheck="false"
          placeholder="输入网址，回车访问（⌘L 聚焦）"
          @focus="($event.target as HTMLInputElement).select()"
        />
        <button class="go-btn" type="submit" title="访问该网址">前往</button>
      </form>

      <button
        v-if="showSync"
        class="chip primary"
        type="button"
        :disabled="syncing"
        title="抓取 cookie 并刷新额度"
        @click="onSync"
      >
        <Icon
          icon="lucide:refresh-cw"
          :class="{ 'icon-spin': syncing }"
          style="font-size: 12px"
        />
        同步会话
      </button>
      <button class="chip close-btn" type="button" title="关闭浏览器 (Esc)" @click="onClose">
        <Icon icon="lucide:x" style="font-size: 13px" />
        关闭
      </button>
    </div>

    <!-- 加载进度条：绝对定位覆盖在底边，不占布局高度 -->
    <div v-if="loading" class="loading-bar" />
  </header>

  <!-- 底部凭据栏：登录页在中间原生 WebView，这里悬浮在窗口最底部 -->
  <footer v-if="hasCreds" class="creds-bar">
    <button
      class="cred-cell"
      type="button"
      :class="{ copied: copied === 'username' }"
      title="点击复制账号"
      @click="copyField('username')"
    >
      <span class="cred-cell-label">
        <Icon icon="lucide:user" style="font-size: 12px" />
        账号
      </span>
      <span class="cred-cell-value">{{ credentials?.username }}</span>
      <Icon
        :icon="copied === 'username' ? 'lucide:check' : 'lucide:copy'"
        class="cred-cell-copy"
        style="font-size: 13px"
      />
    </button>

    <button
      class="cred-cell"
      type="button"
      :class="{ copied: copied === 'password' }"
      title="点击复制密码"
      @click="copyField('password')"
    >
      <span class="cred-cell-label">
        <Icon icon="lucide:lock" style="font-size: 12px" />
        密码
      </span>
      <span class="cred-cell-value">••••••••</span>
      <Icon
        :icon="copied === 'password' ? 'lucide:check' : 'lucide:copy'"
        class="cred-cell-copy"
        style="font-size: 13px"
      />
    </button>

    <button
      v-if="hasSecret"
      class="cred-cell totp"
      type="button"
      :class="{ copied: copied === 'totp', error: totpError }"
      title="点击复制动态验证码"
      @click="copyField('totp')"
    >
      <span class="cred-cell-label">
        <Icon icon="lucide:shield-check" style="font-size: 12px" />
        2FA 验证码
      </span>
      <span class="cred-cell-value totp-code">{{ totpDisplay }}</span>
      <svg v-if="!totpError" class="totp-ring" width="22" height="22" viewBox="0 0 22 22">
        <circle class="totp-ring-bg" cx="11" cy="11" r="9" />
        <circle
          class="totp-ring-fg"
          cx="11"
          cy="11"
          r="9"
          :stroke-dasharray="ringDash"
        />
        <text x="11" y="11" class="totp-ring-num">{{ totpRemaining }}</text>
      </svg>
      <Icon
        v-else
        icon="lucide:alert-triangle"
        class="cred-cell-copy"
        style="font-size: 13px"
      />
    </button>
  </footer>
</template>

<style scoped>
.browser-chrome {
  /* 固定高度 + 固定定位 + 裁剪溢出：保证渲染高度永远等于 CHROME_HEIGHT。
     内容单行只占顶部 ~44px，底部余量作为安全区，防止边界偏差切到控件。 */
  --chrome-height: 80px;
  position: fixed;
  top: 0;
  left: 0;
  right: 0;
  height: var(--chrome-height);
  box-sizing: border-box;
  overflow: hidden;
  z-index: 10;

  padding: 8px 12px 0;
  background: #ffffff;
  border-bottom: 1px solid #ececf0;
}

/* ── 单行工具栏 ─────────────────────── */
.chrome-row {
  height: 38px;
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}

.btn-group {
  display: flex;
  align-items: center;
  gap: 2px;
  flex-shrink: 0;
}

.nav-btn {
  width: 32px;
  height: 32px;
  border: none;
  background: transparent;
  border-radius: 8px;
  color: #52525b;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  padding: 0;
  transition: background 0.15s ease, color 0.15s ease;
}

.nav-btn:hover {
  background: #f4f4f5;
  color: #18181b;
}

.nav-btn:active {
  transform: scale(0.92);
}

.url-form {
  flex: 1;
  min-width: 0;
  height: 36px;
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 0 4px 0 12px;
  border-radius: 999px;
  border: 1px solid transparent;
  background: #f4f4f5;
  transition: background 0.15s ease, border-color 0.15s ease, box-shadow 0.15s ease;
}

.url-form:focus-within {
  background: #ffffff;
  border-color: #d4d4d8;
  box-shadow: 0 0 0 3px rgba(24, 24, 27, 0.06);
}

.lock {
  display: flex;
  align-items: center;
  color: #a1a1aa;
  flex-shrink: 0;
}

.lock.secure {
  color: #16a34a;
}

.url-form input {
  flex: 1;
  min-width: 0;
  border: none;
  outline: none;
  background: transparent;
  color: #18181b;
  font-size: 12.5px;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
}

.go-btn {
  height: 28px;
  padding: 0 14px;
  border: none;
  border-radius: 999px;
  background: #18181b;
  color: #ffffff;
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  flex-shrink: 0;
  transition: background 0.15s ease;
}

.go-btn:hover {
  background: #3f3f46;
}

.chip {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 32px;
  padding: 0 12px;
  border: none;
  border-radius: 999px;
  background: transparent;
  color: #52525b;
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  white-space: nowrap;
  flex-shrink: 0;
  transition: background 0.15s ease, color 0.15s ease;
}

.chip:disabled {
  opacity: 0.6;
  cursor: default;
}

.chip.primary {
  background: #18181b;
  color: #ffffff;
}

.chip.primary:hover:not(:disabled) {
  background: #3f3f46;
}

.chip.close-btn:hover {
  background: #fef2f2;
  color: #dc2626;
}

/* ── 加载进度条 ─────────────────────── */
.loading-bar {
  position: absolute;
  left: 0;
  right: 0;
  bottom: 0;
  height: 2px;
  background: linear-gradient(90deg, transparent, #18181b 40%, #18181b 60%, transparent);
  background-size: 50% 100%;
  background-repeat: no-repeat;
  animation: loading-slide 1.1s ease-in-out infinite;
}

@keyframes loading-slide {
  0% {
    background-position: -60% 0;
  }
  100% {
    background-position: 160% 0;
  }
}

.icon-spin {
  animation: chrome-spin 1s linear infinite;
}

@keyframes chrome-spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

/* ── 底部凭据栏 ─────────────────────── */
.creds-bar {
  --creds-bar-height: 64px;
  position: fixed;
  bottom: 0;
  left: 0;
  right: 0;
  height: var(--creds-bar-height);
  box-sizing: border-box;
  overflow: hidden;
  z-index: 10;

  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 12px;
  background: #fafafa;
  border-top: 1px solid #ececf0;
}

.cred-cell {
  flex: 1;
  min-width: 0;
  height: 44px;
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 1px;
  position: relative;
  padding: 0 34px 0 12px;
  border-radius: 10px;
  border: 1px solid #e4e4e7;
  background: #ffffff;
  cursor: pointer;
  text-align: left;
  transition: border-color 0.15s ease, background 0.15s ease, box-shadow 0.15s ease;
}

.cred-cell:hover {
  border-color: #d4d4d8;
  box-shadow: 0 1px 3px rgba(15, 23, 42, 0.06);
}

.cred-cell:active {
  transform: scale(0.99);
}

.cred-cell.copied {
  border-color: #16a34a;
  background: #f0fdf4;
}

.cred-cell.totp.error {
  border-color: #fca5a5;
  background: #fef2f2;
}

.cred-cell-label {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 10.5px;
  font-weight: 600;
  color: #a1a1aa;
  white-space: nowrap;
}

.cred-cell-value {
  font-size: 13px;
  font-weight: 600;
  color: #18181b;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.cred-cell-value.totp-code {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  letter-spacing: 1px;
  font-size: 15px;
}

.cred-cell-copy {
  position: absolute;
  right: 11px;
  top: 50%;
  transform: translateY(-50%);
  color: #a1a1aa;
}

.cred-cell.copied .cred-cell-copy {
  color: #16a34a;
}

/* TOTP 倒计时环 */
.totp-ring {
  position: absolute;
  right: 8px;
  top: 50%;
  transform: translateY(-50%);
}

.totp-ring-bg {
  fill: none;
  stroke: #e4e4e7;
  stroke-width: 2.5;
}

.totp-ring-fg {
  fill: none;
  stroke: #18181b;
  stroke-width: 2.5;
  stroke-linecap: round;
  transform: rotate(-90deg);
  transform-origin: 11px 11px;
  transition: stroke-dasharray 0.95s linear;
}

.totp-ring-num {
  font-size: 8px;
  font-weight: 700;
  fill: #52525b;
  text-anchor: middle;
  dominant-baseline: central;
}
</style>
