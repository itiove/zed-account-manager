// 前端 TOTP（RFC 6238）实现，基于 Web Crypto 的 HMAC-SHA1。
// 用于在内嵌浏览器底部凭据栏实时计算两步验证（2FA）动态验证码。

/** 解码 Base32（RFC 4648，忽略空格/连字符与大小写，容忍缺省 padding） */
function base32Decode(input: string): Uint8Array {
  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
  const clean = input
    .toUpperCase()
    .replace(/[\s-]/g, "")
    .replace(/=+$/, "");
  let bits = 0;
  let value = 0;
  const out: number[] = [];
  for (const ch of clean) {
    const idx = alphabet.indexOf(ch);
    if (idx === -1) continue;
    value = (value << 5) | idx;
    bits += 5;
    if (bits >= 8) {
      bits -= 8;
      out.push((value >>> bits) & 0xff);
    }
  }
  return new Uint8Array(out);
}

/** 判断字符串是否可能是有效的 Base32 TOTP 密钥 */
export function looksLikeTotpSecret(input: string): boolean {
  const clean = input.toUpperCase().replace(/[\s-]/g, "").replace(/=+$/, "");
  return clean.length >= 8 && /^[A-Z2-7]+$/.test(clean);
}

export interface TotpResult {
  code: string;
  /** 当前 30s 周期内剩余秒数 */
  secondsRemaining: number;
  period: number;
}

/**
 * 计算当前 TOTP 验证码。
 * @param secret Base32 密钥
 * @param digits 位数，默认 6
 * @param period 周期秒数，默认 30
 */
export async function computeTotp(
  secret: string,
  digits = 6,
  period = 30,
): Promise<TotpResult> {
  const key = base32Decode(secret);
  if (key.length === 0) {
    throw new Error("空的 2FA 密钥");
  }

  const nowSec = Math.floor(Date.now() / 1000);
  const counter = Math.floor(nowSec / period);
  const secondsRemaining = period - (nowSec % period);

  // 8 字节大端计数器（显式 ArrayBuffer，满足 Web Crypto 的 BufferSource 类型）
  const counterBuffer = new ArrayBuffer(8);
  const counterView = new DataView(counterBuffer);
  let c = counter;
  for (let i = 7; i >= 0; i--) {
    counterView.setUint8(i, c & 0xff);
    c = Math.floor(c / 256);
  }

  const keyBuffer = new ArrayBuffer(key.length);
  new Uint8Array(keyBuffer).set(key);

  const cryptoKey = await crypto.subtle.importKey(
    "raw",
    keyBuffer,
    { name: "HMAC", hash: "SHA-1" },
    false,
    ["sign"],
  );
  const hmac = new Uint8Array(
    await crypto.subtle.sign("HMAC", cryptoKey, counterBuffer),
  );

  // 动态截断（RFC 4226）
  const offset = hmac[hmac.length - 1] & 0x0f;
  const binary =
    ((hmac[offset] & 0x7f) << 24) |
    ((hmac[offset + 1] & 0xff) << 16) |
    ((hmac[offset + 2] & 0xff) << 8) |
    (hmac[offset + 3] & 0xff);

  const code = (binary % 10 ** digits).toString().padStart(digits, "0");
  return { code, secondsRemaining, period };
}
