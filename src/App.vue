<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { Icon } from "@iconify/vue";
import { api } from "./api";
import type { AccountSummary } from "./types";
import AccountCard from "./components/AccountCard.vue";
import BrowserPanel from "./components/BrowserPanel.vue";
import CredentialsDialog, { type Credentials } from "./components/CredentialsDialog.vue";

const accounts = ref<AccountSummary[]>([]);
const busy = ref(false);
const toast = ref<{ text: string; isError: boolean } | null>(null);

/** 操作加载状态指示 */
const actionTargetId = ref<string | null>(null);
const actionType = ref<"switch" | "refresh" | "browser" | "sync" | "remove" | null>(null);

/** 主窗口内嵌浏览器状态（不新建窗口） */
const browserOpen = ref(false);
const browserTitle = ref("浏览器");
const browserProfileId = ref("");
const browserUrl = ref("");
const browserHint = ref("");
const browserShowSync = ref(false);
const browserAccountId = ref<string | null>(null);
const browserMode = ref<"login" | "session">("login");

/** 环境初始化遮罩：打开内嵌浏览器前的准备阶段提示 */
const envLoading = ref(false);
const envLoadingText = ref("");

/** 添加账号凭据对话框 */
const credDialogOpen = ref(false);
/** 传给内嵌浏览器底部栏的凭据（账号/密码/2FA） */
const browserCredentials = ref<Credentials | null>(null);

let pollTimer: ReturnType<typeof setInterval> | null = null;
let toastTimer: ReturnType<typeof setTimeout> | null = null;
let activeLoginId: string | null = null;

const currentAccount = computed(() => accounts.value.find((a) => a.is_current) ?? null);
const accountCount = computed(() => accounts.value.length);

function showToast(text: string, isError = false) {
  toast.value = { text, isError };
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (toast.value = null), 5600);
}

function dismissToast() {
  if (toastTimer) clearTimeout(toastTimer);
  toast.value = null;
}

async function loadAccounts() {
  try {
    accounts.value = await api.listAccounts();
  } catch (e) {
    showToast(`加载账号列表失败: ${e}`, true);
  }
}

function stopPolling() {
  if (pollTimer !== null) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
  activeLoginId = null;
}

async function closeInAppBrowser() {
  browserOpen.value = false;
  browserAccountId.value = null;
  browserCredentials.value = null;
  try {
    await api.browserClose();
  } catch {
    /* ignore */
  }
}

async function handleSwitch(accountId: string) {
  busy.value = true;
  actionTargetId.value = accountId;
  actionType.value = "switch";
  try {
    const account = await api.switchAccount(accountId);
    showToast(
      `已切换到 ${account.display_name ?? account.github_login ?? account.id}，正在重启 Zed…`,
    );
    await loadAccounts();
  } catch (e) {
    showToast(String(e), true);
  } finally {
    busy.value = false;
    actionTargetId.value = null;
    actionType.value = null;
  }
}

async function handleRemove(accountId: string) {
  if (
    !confirm(
      "确定从本地移除这个账号？\n会删除独立浏览器会话与 cookie，不影响 Zed 当前登录态。",
    )
  ) {
    return;
  }
  busy.value = true;
  actionTargetId.value = accountId;
  actionType.value = "remove";
  try {
    await api.removeAccount(accountId);
    await loadAccounts();
    showToast("已移除账号");
  } catch (e) {
    showToast(String(e), true);
  } finally {
    busy.value = false;
    actionTargetId.value = null;
    actionType.value = null;
  }
}

async function handleRefreshAll() {
  busy.value = true;
  actionTargetId.value = "all";
  actionType.value = "refresh";
  try {
    await api.refreshQuota();
    await loadAccounts();
    showToast("已刷新全部账号额度");
  } catch (e) {
    showToast(String(e), true);
  } finally {
    busy.value = false;
    actionTargetId.value = null;
    actionType.value = null;
  }
}

async function handleRefreshOne(accountId: string) {
  busy.value = true;
  actionTargetId.value = accountId;
  actionType.value = "refresh";
  try {
    await api.refreshQuota(accountId);
    await loadAccounts();
    showToast("已刷新该账号额度");
  } catch (e) {
    showToast(String(e), true);
  } finally {
    busy.value = false;
    actionTargetId.value = null;
    actionType.value = null;
  }
}

/** 网页会话：主窗口内嵌打开该账号独立 profile */
async function handleOpenBrowser(accountId: string) {
  busy.value = true;
  actionTargetId.value = accountId;
  actionType.value = "browser";
  envLoading.value = true;
  envLoadingText.value = "正在初始化独立会话环境…";
  try {
    window.scrollTo(0, 0);
    const prep = await api.prepareAccountBrowser(accountId);
    browserMode.value = "session";
    browserTitle.value = "网页会话（独立隔离）";
    browserProfileId.value = prep.profile_id;
    browserUrl.value = prep.initial_url;
    browserAccountId.value = accountId;
    browserShowSync.value = true;
    browserHint.value =
      "此区域使用该账号独立浏览器存储。登录 Dashboard 后点「同步会话」保存 cookie。";
    envLoading.value = false;
    browserOpen.value = true;
  } catch (e) {
    envLoading.value = false;
    showToast(String(e), true);
  } finally {
    busy.value = false;
    actionTargetId.value = null;
    actionType.value = null;
  }
}

async function handleRecapture(accountId: string) {
  if (browserOpen.value && browserAccountId.value === accountId) {
    await handleBrowserSync();
    return;
  }
  await handleOpenBrowser(accountId);
  showToast("请在页面中登录后点击「同步会话」");
}

async function handleBrowserSync() {
  const accountId = browserAccountId.value;
  if (!accountId) {
    showToast("当前不是账号会话模式", true);
    return;
  }
  busy.value = true;
  actionTargetId.value = accountId;
  actionType.value = "sync";
  try {
    await api.recaptureWebSession(accountId);
    await loadAccounts();
    showToast("已同步 Web 会话并刷新额度");
  } catch (e) {
    showToast(String(e), true);
  } finally {
    busy.value = false;
    actionTargetId.value = null;
    actionType.value = null;
  }
}

/** 点击「添加账号」：先弹凭据对话框 */
function openAddAccountDialog() {
  browserCredentials.value = null;
  credDialogOpen.value = true;
}

function onCredCancel() {
  credDialogOpen.value = false;
}

/** 凭据填写完成：记录凭据并进入登录流程 */
function onCredConfirm(creds: Credentials) {
  credDialogOpen.value = false;
  browserCredentials.value = creds;
  void handleAddAccount();
}

async function handleAddAccount() {
  envLoading.value = true;
  envLoadingText.value = "正在初始化登录环境…";
  try {
    window.scrollTo(0, 0);
    const pending = await api.loginStart();
    activeLoginId = pending.login_id;

    browserMode.value = "login";
    browserTitle.value = "添加 Zed 账号";
    browserProfileId.value = pending.profile_id;
    browserUrl.value = pending.verification_uri;
    browserAccountId.value = null;
    browserShowSync.value = false;
    browserHint.value =
      "在下方完成 GitHub / Zed 授权。每个账号使用独立会话存储，互不串号。";
    envLoading.value = false;
    browserOpen.value = true;

    let elapsed = 0;
    stopPolling();
    activeLoginId = pending.login_id;
    pollTimer = setInterval(async () => {
      elapsed += 1500;
      if (elapsed > 5 * 60 * 1000) {
        stopPolling();
        try {
          await api.loginCancel();
        } catch {
          /* ignore */
        }
        await closeInAppBrowser();
        showToast("登录超时，请重新点击「添加账号」", true);
        return;
      }
      try {
        const result = await api.loginPoll(pending.login_id);
        if (result.status === "success") {
          stopPolling();
          await closeInAppBrowser();
          const name =
            result.account.display_name ??
            result.account.github_login ??
            result.account.id;
          const sessionHint = result.account.has_web_session
            ? "Web 会话已保存"
            : "Web 会话未抓到，可用「网页会话」补登";
          showToast(`登录成功：${name}（${sessionHint}）`);
          await loadAccounts();
        } else if (result.status === "error") {
          stopPolling();
          await closeInAppBrowser();
          showToast(`登录失败: ${result.message}`, true);
        }
      } catch (e) {
        stopPolling();
        await closeInAppBrowser();
        showToast(String(e), true);
      }
    }, 1500);
  } catch (e) {
    envLoading.value = false;
    showToast(String(e), true);
  }
}

async function handleBrowserClose() {
  if (browserMode.value === "login") {
    stopPolling();
    try {
      await api.loginCancel();
    } catch {
      /* ignore */
    }
  }
  await closeInAppBrowser();
}

onMounted(loadAccounts);
onUnmounted(() => {
  stopPolling();
  if (toastTimer) clearTimeout(toastTimer);
  void api.browserClose().catch(() => undefined);
});
</script>

<template>
  <div class="app-shell" :class="{ 'browser-active': browserOpen }">
    <!-- Streamlined Header Navigation -->
    <header class="header">
      <div class="brand">
        <div class="brand-icon-wrapper">
          <Icon icon="lucide:user-check" style="font-size: 22px" />
        </div>
        <div class="brand-title-group">
          <h1>Zed 账号管理</h1>
          <div class="stats-badges">
            <span class="stat-chip">
              <Icon icon="lucide:users" style="font-size: 12px" />
              {{ accountCount }} 个账号
            </span>
          </div>
        </div>
      </div>
      <div class="toolbar">
        <div
          class="active-pill"
          v-if="currentAccount"
          :title="`当前生效账号 · ${currentAccount.plan_raw ?? '未知套餐'}`"
        >
          <span class="pulse-dot" />
          <span class="active-name">
            {{
              currentAccount.display_name ||
              currentAccount.github_login ||
              currentAccount.id
            }}
          </span>
        </div>
        <div class="active-pill offline" v-else title="未写入 Keychain / 未标记当前账号">
          <span class="pulse-dot offline" />
          <span class="active-name">未生效</span>
        </div>

        <button
          class="primary"
          @click="openAddAccountDialog"
          :disabled="busy || browserOpen || envLoading || credDialogOpen"
          title="添加新 Zed 账号"
        >
          <Icon
            :icon="envLoading ? 'lucide:loader-2' : 'lucide:plus'"
            :class="{ 'icon-spin': envLoading }"
            style="font-size: 16px"
          />
          添加账号
        </button>

        <button
          class="outline icon-btn"
          @click="handleRefreshAll"
          :disabled="busy || !accountCount || browserOpen"
          title="刷新所有账号的额度"
        >
          <Icon
            icon="lucide:rotate-cw"
            :class="{ 'icon-spin': busy && actionTargetId === 'all' }"
            style="font-size: 14px"
          />
        </button>
      </div>
    </header>

    <!-- Toast 悬浮通知：固定顶部居中，不占布局 -->
    <Transition name="toast">
      <div class="toast-container" :class="{ error: toast?.isError }" v-if="toast">
        <div class="toast-content">
          <Icon
            class="toast-icon"
            :class="toast.isError ? 'is-error' : 'is-ok'"
            :icon="toast.isError ? 'lucide:alert-triangle' : 'lucide:check-circle-2'"
            style="font-size: 16px; flex-shrink: 0"
          />
          <span>{{ toast.text }}</span>
        </div>
        <button class="toast-close" @click="dismissToast" title="关闭通知">
          <Icon icon="lucide:x" style="font-size: 14px" />
        </button>
      </div>
    </Transition>

    <!-- Account Cards List -->
    <main class="account-list">
      <AccountCard
        v-for="account in accounts"
        :key="account.id"
        :account="account"
        :busy="browserOpen || envLoading || (busy && actionTargetId !== null)"
        :active-action="actionType"
        :is-action-target="actionTargetId === account.id"
        @switch="handleSwitch"
        @remove="handleRemove"
        @refresh="handleRefreshOne"
        @open-browser="handleOpenBrowser"
        @recapture="handleRecapture"
      />

      <div class="empty-state" v-if="accounts.length === 0 && !browserOpen">
        <div class="empty-icon-box">
          <Icon icon="lucide:user-plus" style="font-size: 24px" />
        </div>
        <h2>还没有保存的 Zed 账号</h2>
        <p>
          点击右上角「+ 添加账号」，在本窗口内完成授权。每个账号使用独立会话存储，
          互不串号，可一键切换 Zed 并刷新额度。
        </p>
      </div>
    </main>

    <!-- 主窗口内嵌浏览器：主 WebView 缩为工具栏，内容在下方子 WebView -->
    <div class="browser-host" v-if="browserOpen">
      <BrowserPanel
        :title="browserTitle"
        :profile-id="browserProfileId"
        :initial-url="browserUrl"
        :hint="browserHint"
        :show-sync="browserShowSync"
        :credentials="browserCredentials"
        @close="handleBrowserClose"
        @sync="handleBrowserSync"
      />
    </div>
  </div>

  <!-- 环境初始化遮罩：独立于 app-shell，避免被 browser-active 隐藏规则波及 -->
  <div class="env-overlay" v-if="envLoading">
    <div class="env-box">
      <div class="env-spinner" />
      <div class="env-title">{{ envLoadingText }}</div>
      <div class="env-sub">正在准备独立浏览器会话，马上就好</div>
    </div>
  </div>

  <!-- 添加账号凭据对话框 -->
  <CredentialsDialog
    v-if="credDialogOpen"
    @confirm="onCredConfirm"
    @cancel="onCredCancel"
  />
</template>
