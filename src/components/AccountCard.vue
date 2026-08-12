<script setup lang="ts">
import { computed } from "vue";
import { Icon } from "@iconify/vue";
import type { AccountSummary } from "../types";

const props = defineProps<{
  account: AccountSummary;
  busy: boolean;
  activeAction?: "switch" | "refresh" | "browser" | "sync" | "remove" | null;
  isActionTarget?: boolean;
}>();

const emit = defineEmits<{
  (e: "switch", id: string): void;
  (e: "remove", id: string): void;
  (e: "refresh", id: string): void;
  (e: "open-browser", id: string): void;
  (e: "recapture", id: string): void;
}>();

const label = computed(
  () =>
    props.account.display_name ||
    props.account.github_login ||
    props.account.id,
);

const subLabel = computed(() => {
  if (props.account.github_login && props.account.display_name) {
    return `@${props.account.github_login}`;
  }
  return props.account.id;
});

const initial = computed(() => label.value.charAt(0).toUpperCase());

const planBadge = computed(() => {
  const raw = props.account.plan_raw?.trim();
  if (!raw) return "未知套餐";
  return raw.replace(/^zed_/i, "").toUpperCase();
});

const quotaPercent = computed(() => {
  const used = props.account.token_spend_used_cents;
  const limit = props.account.token_spend_limit_cents;
  if (used == null || limit == null || limit <= 0) return null;
  return Math.min(100, Math.round((used / limit) * 100));
});

const quotaTone = computed(() => {
  const p = quotaPercent.value;
  if (p == null) return "mid";
  if (p >= 90) return "low";
  if (p >= 70) return "warn";
  return "ok";
});

function formatCents(value: number | null | undefined): string {
  if (value == null || !Number.isFinite(value)) return "—";
  return `$${(value / 100).toFixed(2)}`;
}

const spendLabel = computed(() => {
  const used = props.account.token_spend_used_cents;
  const limit = props.account.token_spend_limit_cents;
  if (used == null && limit == null) return null;
  return `${formatCents(used)} / ${formatCents(limit)}`;
});

function parseLimitText(raw: string | null | undefined): string {
  if (!raw) return "—";
  const trimmed = raw.trim();
  if (trimmed.toLowerCase().includes("unlimited")) return "无限";
  try {
    const parsed = JSON.parse(trimmed);
    if (typeof parsed === "number") return parsed.toLocaleString();
    if (typeof parsed === "object" && parsed !== null) {
      if ("limited" in parsed) return Number(parsed.limited).toLocaleString();
      if ("value" in parsed) return Number(parsed.value).toLocaleString();
    }
  } catch {
    /* not JSON */
  }
  const numMatch = trimmed.match(/\d+/);
  if (numMatch) return Number(numMatch[0]).toLocaleString();
  return trimmed.replace(/["{}]/g, "");
}

const editLabel = computed(() => {
  const used = props.account.edit_predictions_used;
  const limit = props.account.edit_predictions_limit_raw;
  if (used == null && !limit) return null;
  return `${used ?? 0} / ${parseLimitText(limit)}`;
});

const refreshedLabel = computed(() => {
  const ts = props.account.last_refreshed_at;
  if (!ts) return null;
  const d = new Date(ts * 1000);
  return d.toLocaleString(undefined, {
    month: "numeric",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
});
</script>

<template>
  <article class="account-card" :class="{ 'is-current': account.is_current }">
    <!-- Top Account Profile Header -->
    <div class="card-top">
      <div class="user-identity">
        <div class="avatar" :class="{ current: account.is_current }">
          {{ initial }}
        </div>
        <div class="account-info">
          <div class="name-row">
            <span class="name">{{ label }}</span>
            <span class="badge-current" v-if="account.is_current">
              <Icon icon="lucide:check-circle-2" style="font-size: 12px" />
              当前生效
            </span>
          </div>
          <div class="sub">{{ subLabel }}</div>
        </div>
      </div>

      <div class="meta-badges">
        <span class="plan-badge">
          <Icon icon="lucide:zap" style="font-size: 11px; color: var(--amber)" />
          {{ planBadge }}
        </span>
        <span
          class="session-pill"
          :class="account.has_web_session ? 'ok' : 'warn'"
          :title="
            account.has_web_session
              ? '已保存 Web 会话，可用 Dashboard 额度接口'
              : '尚无 zed.session，可点「网页会话」补登'
          "
        >
          <Icon
            :icon="account.has_web_session ? 'lucide:globe' : 'lucide:shield-alert'"
            style="font-size: 11px"
          />
          {{ account.has_web_session ? "Web 会话就绪" : "缺 Web 会话" }}
        </span>
      </div>
    </div>

    <!-- Compact Quotas Display Bar -->
    <div class="quota-block" v-if="spendLabel || editLabel">
      <div class="quota-grid">
        <div class="quota-item" v-if="spendLabel">
          <span class="q-label">
            <Icon icon="lucide:coins" style="font-size: 13px" />
            Token 花费
          </span>
          <span class="q-value">{{ spendLabel }}</span>
        </div>
        <div class="quota-item" v-if="editLabel">
          <span class="q-label">
            <Icon icon="lucide:wand-2" style="font-size: 13px" />
            Edit Predictions
          </span>
          <span class="q-value">{{ editLabel }}</span>
        </div>
      </div>

      <div class="quota-bar" v-if="quotaPercent !== null" :class="quotaTone">
        <div class="quota-bar-fill" :style="{ width: quotaPercent + '%' }" />
      </div>

      <div class="quota-hint" v-if="quotaPercent !== null || refreshedLabel">
        <span>{{ quotaPercent !== null ? `已用额度 ${quotaPercent}%` : "" }}</span>
        <span v-if="refreshedLabel" class="q-label">
          <Icon icon="lucide:clock" style="font-size: 11px" />
          刷新于 {{ refreshedLabel }}
        </span>
      </div>
    </div>

    <div class="quota-empty" v-else>
      <Icon icon="lucide:info" style="font-size: 13px; margin-right: 4px" />
      暂无额度数据
      <span v-if="account.last_quota_error" class="err-hint">
        · {{ account.last_quota_error }}
      </span>
    </div>

    <!-- Intuitive Action Buttons Toolbar -->
    <div class="actions">
      <button
        class="primary action-switch"
        v-if="!account.is_current"
        :disabled="busy"
        @click="emit('switch', account.id)"
      >
        <Icon
          :icon="isActionTarget && activeAction === 'switch' ? 'lucide:loader-2' : 'lucide:arrow-right-left'"
          :class="{ 'icon-spin': isActionTarget && activeAction === 'switch' }"
          style="font-size: 14px"
        />
        {{ isActionTarget && activeAction === 'switch' ? "正在切换..." : "切换到此账号" }}
      </button>

      <button
        class="outline"
        :disabled="busy"
        @click="emit('refresh', account.id)"
        title="刷新额度数据"
      >
        <Icon
          :icon="isActionTarget && activeAction === 'refresh' ? 'lucide:loader-2' : 'lucide:rotate-cw'"
          :class="{ 'icon-spin': isActionTarget && activeAction === 'refresh' }"
          style="font-size: 13px"
        />
        {{ isActionTarget && activeAction === 'refresh' ? "刷新中" : "刷新" }}
      </button>

      <button
        class="outline"
        :disabled="busy"
        :title="account.has_web_session ? '打开该账号独立浏览器' : '打开独立浏览器补登 Dashboard'"
        @click="emit('open-browser', account.id)"
      >
        <Icon
          :icon="isActionTarget && activeAction === 'browser' ? 'lucide:loader-2' : 'lucide:globe'"
          :class="{ 'icon-spin': isActionTarget && activeAction === 'browser' }"
          style="font-size: 13px"
        />
        网页会话
      </button>

      <button
        class="ghost"
        :disabled="busy"
        title="从该账号浏览器抓取 cookie 并刷新额度"
        @click="emit('recapture', account.id)"
      >
        <Icon
          :icon="isActionTarget && activeAction === 'sync' ? 'lucide:loader-2' : 'lucide:refresh-cw'"
          :class="{ 'icon-spin': isActionTarget && activeAction === 'sync' }"
          style="font-size: 13px"
        />
        同步
      </button>

      <button
        class="danger"
        :disabled="busy"
        title="从本地移除（不影响 Zed 当前登录态）"
        @click="emit('remove', account.id)"
      >
        <Icon
          :icon="isActionTarget && activeAction === 'remove' ? 'lucide:loader-2' : 'lucide:trash-2'"
          :class="{ 'icon-spin': isActionTarget && activeAction === 'remove' }"
          style="font-size: 13px"
        />
        移除
      </button>
    </div>
  </article>
</template>
