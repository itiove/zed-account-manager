<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from "vue";
import { Icon } from "@iconify/vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

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
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "sync"): void;
}>();

const urlInput = ref("");
const loading = ref(false);
const syncing = ref(false);

let unlisten: UnlistenFn | null = null;

async function openBrowser() {
  loading.value = true;
  try {
    await invoke("browser_open", {
      url: props.initialUrl,
      profileId: props.profileId,
      chromeHeight: CHROME_HEIGHT,
    });
    urlInput.value = props.initialUrl;
  } catch (e) {
    console.error(e);
  } finally {
    loading.value = false;
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

  await openBrowser();
});

onUnmounted(() => {
  document.body.style.overflow = "";
  window.removeEventListener("keydown", onKeydown);
  if (unlisten) void unlisten();
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
</style>
