import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";

function resolveParams(): { windowLabel: string; initialUrl: string } {
  let windowLabel = "";
  let initialUrl = "";
  try {
    const params = new URLSearchParams(window.location.search);
    windowLabel = params.get("window") || "";
    initialUrl = params.get("url") || "";
  } catch {
    /* ignore */
  }
  if (!windowLabel) {
    try {
      const hash = window.location.hash.replace(/^#/, "");
      const params = new URLSearchParams(hash);
      windowLabel = params.get("window") || "";
      initialUrl = params.get("url") || "";
    } catch {
      /* ignore */
    }
  }
  if (!windowLabel) {
    try {
      windowLabel = getCurrentWindow().label;
    } catch {
      windowLabel = "";
    }
  }
  return { windowLabel, initialUrl };
}

const { windowLabel, initialUrl } = resolveParams();
const urlInput = document.getElementById("url-input") as HTMLInputElement;
const urlForm = document.getElementById("url-form") as HTMLFormElement;
const securityBadge = document.getElementById("security-badge") as HTMLSpanElement;
const btnBack = document.getElementById("btn-back") as HTMLButtonElement;
const btnForward = document.getElementById("btn-forward") as HTMLButtonElement;
const btnReload = document.getElementById("btn-reload") as HTMLButtonElement;
const btnHome = document.getElementById("btn-home") as HTMLButtonElement;
const btnGo = document.getElementById("go-btn") as HTMLButtonElement;

let navigating = false;

function applyUrl(url: string) {
  if (!url) return;
  urlInput.value = url;
  const secure = url.startsWith("https://");
  if (urlForm) urlForm.classList.toggle("secure", secure);
  if (securityBadge) {
    securityBadge.title = secure ? "HTTPS 安全连接" : "HTTP 未加密";
  }
}

// 设定初始 URL
if (initialUrl) {
  applyUrl(initialUrl);
}

async function refreshUrlFromBackend() {
  if (!windowLabel) return;
  try {
    const url = await invoke<string>("browser_current_url", { windowLabel });
    applyUrl(url);
  } catch {
    /* content not ready */
  }
}

async function withBusy(action: () => Promise<void>) {
  if (navigating || !windowLabel) return;
  navigating = true;
  if (btnReload) btnReload.disabled = true;
  try {
    await action();
  } catch (e) {
    console.error(e);
  } finally {
    navigating = false;
    if (btnReload) btnReload.disabled = false;
  }
}

if (btnBack) {
  btnBack.addEventListener("click", () =>
    withBusy(async () => {
      await invoke("browser_back", { windowLabel });
      setTimeout(refreshUrlFromBackend, 280);
    }),
  );
}

if (btnForward) {
  btnForward.addEventListener("click", () =>
    withBusy(async () => {
      await invoke("browser_forward", { windowLabel });
      setTimeout(refreshUrlFromBackend, 280);
    }),
  );
}

if (btnReload) {
  btnReload.addEventListener("click", () =>
    withBusy(async () => {
      await invoke("browser_reload", { windowLabel });
    }),
  );
}

if (btnHome) {
  btnHome.addEventListener("click", () =>
    withBusy(async () => {
      const next = await invoke<string>("browser_goto", {
        windowLabel,
        url: "https://dashboard.zed.dev/",
      });
      applyUrl(next);
    }),
  );
}

async function goToAddress() {
  const raw = urlInput.value.trim();
  if (!raw) return;
  await withBusy(async () => {
    const next = await invoke<string>("browser_goto", {
      windowLabel,
      url: raw,
    });
    applyUrl(next);
  });
}

if (urlForm) {
  urlForm.addEventListener("submit", (e) => {
    e.preventDefault();
    void goToAddress();
  });
}

if (btnGo) {
  btnGo.addEventListener("click", () => {
    void goToAddress();
  });
}

if (urlInput) {
  urlInput.addEventListener("focus", () => {
    requestAnimationFrame(() => urlInput.select());
  });
}

window.addEventListener("keydown", (e) => {
  const meta = e.metaKey || e.ctrlKey;
  if (meta && e.key.toLowerCase() === "r") {
    e.preventDefault();
    if (btnReload) btnReload.click();
  } else if (meta && e.key === "[") {
    e.preventDefault();
    if (btnBack) btnBack.click();
  } else if (meta && e.key === "]") {
    e.preventDefault();
    if (btnForward) btnForward.click();
  } else if (meta && e.key.toLowerCase() === "l") {
    e.preventDefault();
    if (urlInput) {
      urlInput.focus();
      urlInput.select();
    }
  }
});

void listen<{ url: string }>("browser-url-changed", (event) => {
  applyUrl(event.payload.url);
});

// 轮询获取内容区 URL
void (async () => {
  for (let i = 0; i < 20; i++) {
    await new Promise((r) => setTimeout(r, 200));
    try {
      const url = await invoke<string>("browser_current_url", { windowLabel });
      applyUrl(url);
      break;
    } catch {
      /* retry */
    }
  }
})();
