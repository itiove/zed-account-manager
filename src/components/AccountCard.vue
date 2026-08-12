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
}>();

/** 主名称：邮箱优先（信息量最大），完整展示不截断 */
const label = computed(
  () =>
    props.account.email ||
    props.account.display_name ||
    props.account.github_login ||
    props.account.id,
);

const subLabel = computed(() => {
  const parts: string[] = [];
  if (props.account.github_login) parts.push(`@${props.account.github_login}`);
  if (
    props.account.display_name &&
    props.account.display_name !== label.value
  ) {
    parts.push(props.account.display_name);
  }
  return parts.join(" · ") || props.account.id;
});

const initial = computed(() => label.value.charAt(0).toUpperCase());

const planBadge = computed(() => {
  const raw = props.account.plan_raw?.trim();
  if (!raw) return "未知套餐";
  return raw
    .replace(/^token_based_/i, "")
    .replace(/^zed_/i, "")
    .replace(/_/g, " ")
    .toUpperCase();
});

/** 套餐分级：pro 系（含 trial）用高级配色，free/未知用灰 */
const planTier = computed(() => {
  const raw = (props.account.plan_raw ?? "").toLowerCase();
  if (raw.includes("pro")) return "pro";
  return "free";
});

/* ── 用量进度（cents → $，除以 100） ─────────── */
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

function formatDollar(value: number | null | undefined): string {
  if (value == null || !Number.isFinite(value)) return "—";
  return `$${(value / 100).toFixed(2)}`;
}

const spendUsed = computed(() =>
  formatDollar(props.account.token_spend_used_cents),
);
const spendLimit = computed(() =>
  formatDollar(props.account.token_spend_limit_cents),
);
const hasSpend = computed(
  () =>
    props.account.token_spend_used_cents != null ||
    props.account.token_spend_limit_cents != null,
);

function parseLimitText(raw: string | null | undefined): string {
  if (!raw) return "∞";
  const trimmed = raw.trim();
  if (trimmed.toLowerCase().includes("unlimited") || trimmed === "null")
    return "∞";
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
    <!-- 第一行：身份 + 操作 -->
    <div class="card-row">
      <div class="avatar" :class="{ current: account.is_current }">
        <img
          v-if="account.avatar_url"
          :src="account.avatar_url"
          :alt="label"
          loading="lazy"
        />
        <template v-else>{{ initial }}</template>
      </div>

      <div class="account-info">
        <div class="name-row">
          <span class="name" :title="label">{{ label }}</span>
          <span class="badge-current" v-if="account.is_current">
            <Icon icon="lucide:check" style="font-size: 11px" />
            当前
          </span>
          <span
            class="session-dot"
            :class="account.has_web_session ? 'ok' : 'warn'"
            :title="
              account.has_web_session
                ? '已保存 Web 会话，可直接刷新额度'
                : '尚无 Web 会话，点「网页会话」补登'
            "
          />
        </div>
        <div class="sub">{{ subLabel }}</div>
      </div>

      <div class="card-side">
        <span class="plan-badge" :class="planTier">
          <Icon
            v-if="planTier === 'pro'"
            icon="lucide:crown"
            style="font-size: 10px"
          />
          {{ planBadge }}
        </span>
        <div class="actions">
        <button
          class="icon-action"
          :disabled="busy"
          title="刷新额度数据"
          @click="emit('refresh', account.id)"
        >
          <Icon
            :icon="isActionTarget && activeAction === 'refresh' ? 'lucide:loader-2' : 'lucide:rotate-cw'"
            :class="{ 'icon-spin': isActionTarget && activeAction === 'refresh' }"
            style="font-size: 14px"
          />
        </button>
        <button
          class="icon-action"
          :disabled="busy"
          :title="account.has_web_session ? '打开该账号独立浏览器' : '打开独立浏览器补登 Dashboard'"
          @click="emit('open-browser', account.id)"
        >
          <Icon
            :icon="isActionTarget && activeAction === 'browser' ? 'lucide:loader-2' : 'lucide:globe'"
            :class="{ 'icon-spin': isActionTarget && activeAction === 'browser' }"
            style="font-size: 14px"
          />
        </button>
        <button
          class="icon-action danger"
          :disabled="busy"
          title="从本地移除（不影响 Zed 当前登录态）"
          @click="emit('remove', account.id)"
        >
          <Icon
            :icon="isActionTarget && activeAction === 'remove' ? 'lucide:loader-2' : 'lucide:trash-2'"
            :class="{ 'icon-spin': isActionTarget && activeAction === 'remove' }"
            style="font-size: 14px"
          />
        </button>

        <button
          class="primary switch-btn"
          v-if="!account.is_current"
          :disabled="busy"
          @click="emit('switch', account.id)"
        >
          <Icon
            :icon="isActionTarget && activeAction === 'switch' ? 'lucide:loader-2' : 'lucide:arrow-right-left'"
            :class="{ 'icon-spin': isActionTarget && activeAction === 'switch' }"
            style="font-size: 13px"
          />
          {{ isActionTarget && activeAction === 'switch' ? "切换中" : "切换" }}
        </button>
        </div>
      </div>
    </div>

    <!-- 第二行：额度进度条 -->
    <div class="quota-row" v-if="hasSpend">
      <span class="quota-money">
        <span class="used">{{ spendUsed }}</span>
        <span class="sep">/</span>
        <span class="limit">{{ spendLimit }}</span>
      </span>
      <div class="quota-bar" :class="quotaTone">
        <div
          class="quota-bar-fill"
          :style="{ width: (quotaPercent ?? 0) + '%' }"
        />
      </div>
      <span class="quota-pct" :class="quotaTone" v-if="quotaPercent !== null">
        {{ quotaPercent }}%
      </span>
      <span class="quota-meta" v-if="editLabel" title="Edit Predictions 用量">
        <Icon icon="lucide:wand-2" style="font-size: 11px" />
        {{ editLabel }}
      </span>
      <span class="quota-meta" v-if="refreshedLabel" :title="`额度刷新于 ${refreshedLabel}`">
        <Icon icon="lucide:clock" style="font-size: 11px" />
        {{ refreshedLabel }}
      </span>
    </div>

    <div class="quota-row empty" v-else>
      <Icon icon="lucide:info" style="font-size: 12px" />
      暂无额度数据，点刷新或「同步」获取
      <span v-if="account.last_quota_error" class="err-hint">
        · {{ account.last_quota_error }}
      </span>
    </div>
  </article>
</template>
