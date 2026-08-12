<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { Icon } from "@iconify/vue";
import { looksLikeTotpSecret } from "../totp";

export interface Credentials {
  username: string;
  password: string;
  secret: string;
}

const emit = defineEmits<{
  (e: "confirm", creds: Credentials): void;
  (e: "cancel"): void;
}>();

const username = ref("");
const password = ref("");
const secret = ref("");
const bulk = ref("");
const showPassword = ref(false);
const bulkRef = ref<HTMLTextAreaElement | null>(null);

/** 把一段文本按「账号 / 密码 / 2FA 密钥」智能拆分并回填三个输入框 */
function parseBulk(text: string) {
  const normalized = text
    .replace(/[|,;\t]/g, "\n")
    .replace(/-{3,}/g, "\n")
    .replace(/\u2014{1,}/g, "\n");
  let tokens = normalized
    .split(/\n+/)
    .map((s) => s.trim())
    .filter(Boolean);
  // 单行且用空格分隔的情况
  if (tokens.length === 1) {
    tokens = tokens[0].split(/\s+/).filter(Boolean);
  }
  if (tokens.length === 0) return;

  username.value = tokens[0] ?? "";
  password.value = tokens[1] ?? "";
  // 第三段及之后都并入 2FA 密钥（密钥可能带空格被拆开）
  secret.value = tokens.slice(2).join("").replace(/\s+/g, "");
}

watch(bulk, (v) => {
  if (v.trim()) parseBulk(v);
});

const secretValid = computed(
  () => secret.value.trim() === "" || looksLikeTotpSecret(secret.value),
);
const canConfirm = computed(
  () => username.value.trim() !== "" && password.value.trim() !== "",
);

function onConfirm() {
  if (!canConfirm.value) return;
  emit("confirm", {
    username: username.value.trim(),
    password: password.value.trim(),
    secret: secret.value.replace(/\s+/g, ""),
  });
}

function onCancel() {
  emit("cancel");
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === "Escape") onCancel();
  if ((e.metaKey || e.ctrlKey) && e.key === "Enter") onConfirm();
}

onMounted(async () => {
  await nextTick();
  bulkRef.value?.focus();
});
</script>

<template>
  <div class="cred-overlay" @keydown="onKeydown">
    <div class="cred-dialog" role="dialog" aria-modal="true">
      <div class="cred-head">
        <div class="cred-head-icon">
          <Icon icon="lucide:key-round" style="font-size: 18px" />
        </div>
        <div class="cred-head-text">
          <h2>添加 Zed 账号</h2>
          <p>填写登录凭据，稍后会在浏览器底部随时复制</p>
        </div>
        <button class="cred-x" type="button" title="取消 (Esc)" @click="onCancel">
          <Icon icon="lucide:x" style="font-size: 16px" />
        </button>
      </div>

      <div class="cred-body">
        <label class="cred-field bulk">
          <span class="cred-label">
            <Icon icon="lucide:clipboard-paste" style="font-size: 13px" />
            快速粘贴（账号 · 密码 · 2FA 密钥）
          </span>
          <textarea
            ref="bulkRef"
            v-model="bulk"
            rows="3"
            spellcheck="false"
            placeholder="可直接粘贴一段文本，自动拆分到下面三项。&#10;例如：&#10;user@example.com&#10;MyPassw0rd&#10;JBSWY3DPEHPK3PXP"
          />
        </label>

        <div class="cred-divider"><span>或分别填写</span></div>

        <label class="cred-field">
          <span class="cred-label">
            <Icon icon="lucide:user" style="font-size: 13px" />
            账号 / 邮箱
          </span>
          <input
            v-model="username"
            type="text"
            spellcheck="false"
            autocomplete="off"
            placeholder="用户名或邮箱"
          />
        </label>

        <label class="cred-field">
          <span class="cred-label">
            <Icon icon="lucide:lock" style="font-size: 13px" />
            密码
          </span>
          <div class="cred-input-wrap">
            <input
              v-model="password"
              :type="showPassword ? 'text' : 'password'"
              spellcheck="false"
              autocomplete="off"
              placeholder="登录密码"
            />
            <button
              class="cred-eye"
              type="button"
              :title="showPassword ? '隐藏' : '显示'"
              @click="showPassword = !showPassword"
            >
              <Icon
                :icon="showPassword ? 'lucide:eye-off' : 'lucide:eye'"
                style="font-size: 14px"
              />
            </button>
          </div>
        </label>

        <label class="cred-field">
          <span class="cred-label">
            <Icon icon="lucide:shield-check" style="font-size: 13px" />
            2FA 密钥（可选）
          </span>
          <input
            v-model="secret"
            type="text"
            spellcheck="false"
            autocomplete="off"
            :class="{ invalid: !secretValid }"
            placeholder="两步验证 Base32 密钥，用于动态计算验证码"
          />
          <span v-if="!secretValid" class="cred-warn">
            这看起来不是有效的 Base32 密钥，请检查
          </span>
        </label>
      </div>

      <div class="cred-foot">
        <button class="cred-btn ghost" type="button" @click="onCancel">取消</button>
        <button
          class="cred-btn primary"
          type="button"
          :disabled="!canConfirm"
          @click="onConfirm"
        >
          确定并打开登录页
          <span class="cred-kbd">⌘↵</span>
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.cred-overlay {
  position: fixed;
  inset: 0;
  z-index: 110;
  display: grid;
  place-items: center;
  padding: 20px;
  background: rgba(15, 23, 42, 0.28);
  backdrop-filter: blur(4px);
  -webkit-backdrop-filter: blur(4px);
  animation: cred-fade 0.16s ease-out;
}

.cred-dialog {
  width: 100%;
  max-width: 440px;
  background: #ffffff;
  border-radius: 16px;
  border: 1px solid #e4e4e7;
  box-shadow: 0 20px 60px rgba(15, 23, 42, 0.24);
  overflow: hidden;
  animation: cred-pop 0.18s cubic-bezier(0.16, 1, 0.3, 1);
}

.cred-head {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 16px 16px 14px;
  border-bottom: 1px solid #f1f1f4;
}

.cred-head-icon {
  width: 38px;
  height: 38px;
  border-radius: 10px;
  background: #18181b;
  color: #ffffff;
  display: grid;
  place-items: center;
  flex-shrink: 0;
}

.cred-head-text {
  flex: 1;
  min-width: 0;
}

.cred-head-text h2 {
  margin: 0;
  font-size: 15px;
  font-weight: 700;
  color: #18181b;
}

.cred-head-text p {
  margin: 2px 0 0;
  font-size: 12px;
  color: #71717a;
}

.cred-x {
  width: 30px;
  height: 30px;
  border: none;
  background: transparent;
  border-radius: 8px;
  color: #a1a1aa;
  cursor: pointer;
  display: grid;
  place-items: center;
  flex-shrink: 0;
}

.cred-x:hover {
  background: #f4f4f5;
  color: #18181b;
}

.cred-body {
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  max-height: 60vh;
  overflow-y: auto;
}

.cred-field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.cred-label {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  font-weight: 600;
  color: #3f3f46;
}

.cred-field input,
.cred-field textarea {
  width: 100%;
  box-sizing: border-box;
  border: 1px solid #e4e4e7;
  border-radius: 9px;
  padding: 9px 11px;
  font-size: 13px;
  color: #18181b;
  background: #fafafa;
  outline: none;
  transition: border-color 0.15s ease, box-shadow 0.15s ease, background 0.15s ease;
  font-family: inherit;
}

.cred-field textarea {
  resize: none;
  line-height: 1.5;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 12px;
}

.cred-field input:focus,
.cred-field textarea:focus {
  border-color: #18181b;
  background: #ffffff;
  box-shadow: 0 0 0 3px rgba(24, 24, 27, 0.07);
}

.cred-field input.invalid {
  border-color: #fca5a5;
}

.cred-input-wrap {
  position: relative;
  display: flex;
  align-items: center;
}

.cred-input-wrap input {
  padding-right: 38px;
}

.cred-eye {
  position: absolute;
  right: 6px;
  width: 26px;
  height: 26px;
  border: none;
  background: transparent;
  border-radius: 6px;
  color: #a1a1aa;
  cursor: pointer;
  display: grid;
  place-items: center;
}

.cred-eye:hover {
  background: #f4f4f5;
  color: #18181b;
}

.cred-warn {
  font-size: 11px;
  color: #dc2626;
}

.cred-divider {
  display: flex;
  align-items: center;
  gap: 10px;
  color: #a1a1aa;
  font-size: 11px;
}

.cred-divider::before,
.cred-divider::after {
  content: "";
  flex: 1;
  height: 1px;
  background: #f1f1f4;
}

.cred-foot {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 12px 16px;
  border-top: 1px solid #f1f1f4;
  background: #fafafa;
}

.cred-btn {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  height: 36px;
  padding: 0 15px;
  border-radius: 9px;
  border: 1px solid transparent;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: background 0.15s ease, border-color 0.15s ease;
}

.cred-btn.ghost {
  background: #ffffff;
  border-color: #e4e4e7;
  color: #3f3f46;
}

.cred-btn.ghost:hover {
  background: #f4f4f5;
}

.cred-btn.primary {
  background: #18181b;
  color: #ffffff;
}

.cred-btn.primary:hover:not(:disabled) {
  background: #3f3f46;
}

.cred-btn.primary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.cred-kbd {
  font-size: 11px;
  opacity: 0.7;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
}

@keyframes cred-fade {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}

@keyframes cred-pop {
  from {
    opacity: 0;
    transform: translateY(8px) scale(0.98);
  }
  to {
    opacity: 1;
    transform: translateY(0) scale(1);
  }
}
</style>
