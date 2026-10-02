// Text/data toolbox: pure functions, no Vue/DOM/Tauri.
// JSON pretty/minify, base64, UUID, timestamps, JWT decode (no verify),
// regex tester. QR + file-hash live in Rust (binary access). Tested by bun.

/** Pretty-print JSON (2-space) or throw with position hint. */
export function jsonPretty(raw: string): string {
  const v: unknown = JSON.parse(raw);
  return JSON.stringify(v, null, 2);
}

/** Minify JSON (single line). */
export function jsonMinify(raw: string): string {
  const v: unknown = JSON.parse(raw);
  return JSON.stringify(v);
}

/** Validate JSON, return error string or null. */
export function jsonError(raw: string): string | null {
  try {
    JSON.parse(raw);
    return null;
  } catch (e) {
    return e instanceof Error ? e.message : String(e);
  }
}

/** UTF-8 string → base64. */
export function b64encode(text: string): string {
  const bytes = new TextEncoder().encode(text);
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin);
}

/** Base64 → UTF-8 string (throws on bad input). */
export function b64decode(b64: string): string {
  const bin = atob(b64.trim());
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return new TextDecoder().decode(bytes);
}

/** Base64URL (JWT-style) → UTF-8 string. */
export function b64urlDecode(b64url: string): string {
  const padded = b64url.replace(/-/g, "+").replace(/_/g, "/");
  const pad = (4 - (padded.length % 4)) % 4;
  return b64decode(padded + "=".repeat(pad));
}

/** UUID v4 (crypto.randomUUID with Math fallback). */
export function newUuid(): string {
  try {
    if (typeof crypto !== "undefined" && "randomUUID" in crypto) return crypto.randomUUID();
  } catch {
    /* fall through */
  }
  return "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".replace(/[xy]/g, (c) => {
    const r = Math.floor(Math.random() * 16);
    return (c === "x" ? r : (r & 0x3) | 0x8).toString(16);
  });
}

/** ms epoch → ISO string (null when invalid). */
export function epochToIso(ms: number): string | null {
  if (!Number.isFinite(ms)) return null;
  const d = new Date(ms);
  return Number.isNaN(d.getTime()) ? null : d.toISOString();
}

/** ISO/date string → ms epoch (null when unparsable). */
export function isoToEpoch(raw: string): number | null {
  const t = Date.parse(raw.trim());
  return Number.isNaN(t) ? null : t;
}

/** Decode JWT parts (header/payload pretty JSON). No signature verify — labelled. */
export function jwtDecode(token: string): { header: string; payload: string } {
  const parts = token.trim().split(".");
  if (parts.length < 2) throw new Error("JWT needs header.payload[.signature]");
  const fmt = (p: string): string => {
    const json = b64urlDecode(p);
    return JSON.stringify(JSON.parse(json), null, 2);
  };
  return { header: fmt(parts[0]), payload: fmt(parts[1]) };
}

export interface RegexHit {
  index: number;
  match: string;
  groups: string[];
}

/** Test a regex (pattern + flags) against text, up to 50 hits. */
export function regexTest(pattern: string, flags: string, text: string): { hits: RegexHit[]; error: string | null } {
  let re: RegExp;
  try {
    re = new RegExp(pattern, flags.includes("g") ? flags : flags + "g");
  } catch (e) {
    return { hits: [], error: e instanceof Error ? e.message : String(e) };
  }
  const hits: RegexHit[] = [];
  let m: RegExpExecArray | null;
  let guard = 0;
  while ((m = re.exec(text)) !== null && guard < 50) {
    guard += 1;
    hits.push({ index: m.index, match: m[0], groups: m.slice(1) });
    if (m[0].length === 0) re.lastIndex += 1;
  }
  return { hits, error: null };
}

/** QR payload kinds (builders fill the QR text box; encode path unchanged). */
export type QrPayloadKind = "raw" | "url" | "wifi" | "mailto" | "sms" | "vcard";

/** Escape `; , : \` for WiFi payload fields (spec escaping). */
function wifiEscape(raw: string): string {
  return raw.replace(/\\/g, "\\\\").replace(/;/g, "\\;").replace(/,/g, "\\,").replace(/:/g, "\\:");
}

/** Fold a vCard line at 75 octets (CRLF + space continuation). ASCII-only inputs here. */
function foldVcard(line: string): string {
  if (line.length <= 75) return line;
  let out = line.slice(0, 75);
  let rest = line.slice(75);
  while (rest.length > 0) {
    out += "\r\n " + rest.slice(0, 74);
    rest = rest.slice(74);
  }
  return out;
}

/** Build a WiFi QR payload (`WIFI:T:..;S:..;P:..;H:..;;`). Throws on bad input. */
export function qrWifi(security: string, ssid: string, password: string, hidden: boolean): string {
  const sec = security.trim().toUpperCase();
  if (sec !== "WPA" && sec !== "WEP" && sec !== "NOPASS") throw new Error("WiFi security must be WPA|WEP|nopass");
  if (!ssid) throw new Error("WiFi SSID required");
  if (sec === "NOPASS") return `WIFI:T:nopass;S:${wifiEscape(ssid)};${hidden ? "H:true;" : ""};`;
  if (!password) throw new Error("WiFi password required for WPA/WEP");
  return `WIFI:T:${sec};S:${wifiEscape(ssid)};P:${wifiEscape(password)};${hidden ? "H:true;" : ""};`;
}

/** Build a URL QR payload (requires http/https scheme). */
export function qrUrl(raw: string): string {
  const url = raw.trim();
  if (!/^https?:\/\//i.test(url)) throw new Error("URL must start with http:// or https://");
  return url;
}

/** Build a `mailto:` QR payload (minimal address check). */
export function qrMailto(to: string, subject: string, body: string): string {
  const addr = to.trim();
  if (!/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(addr)) throw new Error("bad email address");
  const params = new URLSearchParams();
  if (subject) params.set("subject", subject);
  if (body) params.set("body", body);
  const q = params.toString();
  return `mailto:${addr}${q ? "?" + q : ""}`;
}

/** Build an `sms:` QR payload (digits/`+` only, message optional). */
export function qrSms(number: string, message: string): string {
  const num = number.trim();
  if (!/^\+?\d+$/.test(num)) throw new Error("SMS number must be digits with optional leading +");
  return message ? `sms:${num}?body=${encodeURIComponent(message)}` : `sms:${num}`;
}

/** Build a minimal vCard 3.0 QR payload (FN required; CRLF folded). */
export function qrVcard(name: string, phone: string, email: string): string {
  if (!name.trim()) throw new Error("vCard name required");
  const lines = ["BEGIN:VCARD", "VERSION:3.0", foldVcard(`FN:${name.trim()}`)];
  if (phone.trim()) lines.push(foldVcard(`TEL:${phone.trim()}`));
  if (email.trim()) {
    if (!/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(email.trim())) throw new Error("bad vCard email");
    lines.push(foldVcard(`EMAIL:${email.trim()}`));
  }
  lines.push("END:VCARD");
  return lines.join("\r\n");
}
