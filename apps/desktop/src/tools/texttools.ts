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
