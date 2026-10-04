<!-- Text/data toolbox: JSON, base64, UUID, timestamps, JWT, regex, QR, file hash.
All local-first; QR + file hash go through thin Rust commands. -->
<script setup lang="ts">
import { computed, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { b64decode, b64encode, epochToIso, isoToEpoch, jsonError, jsonMinify, jsonPretty, jwtDecode, newUuid, qrMailto, qrSms, qrUrl, qrVcard, qrWifi, regexTest } from "./texttools";
import type { QrPayloadKind } from "./texttools";
import { api, isCommandError, revealInFolder } from "../api";

const jsonIn = ref('{"hello":"world"}');
const jsonOut = ref("");
const jsonMsg = ref<string | null>(null);
const b64In = ref("hello");
const b64Out = ref("");
const b64Mode = ref<"encode" | "decode">("encode");
const b64Msg = ref<string | null>(null);
const uuidOut = ref("");
const epochIn = ref("0");
const epochOut = ref<string | null>(null);
const isoIn = ref("1970-01-01T00:00:00.000Z");
const isoOut = ref<number | null>(null);
const jwtIn = ref("");
const jwtOut = ref("");
const jwtMsg = ref<string | null>(null);
const rxPattern = ref("\\d+");
const rxFlags = ref("g");
const rxText = ref("a1b22c333");
const qrText = ref("https://example.com");
const qrSize = ref(256);
const qrEc = ref("M");
const qrFormat = ref<"png" | "svg">("png");
const qrQuiet = ref(true);
const qrMsg = ref<string | null>(null);
const qrPath = ref<string | null>(null);
const qrDecoded = ref<string | null>(null);
const qrKind = ref<QrPayloadKind>("raw");
const qrBuildMsg = ref<string | null>(null);
const wifiSsid = ref("");
const wifiPass = ref("");
const wifiSec = ref("WPA");
const wifiHidden = ref(false);
const mailTo = ref("");
const mailSubj = ref("");
const mailBody = ref("");
const smsNum = ref("");
const smsMsg = ref("");
const vcName = ref("");
const vcPhone = ref("");
const vcEmail = ref("");
const hashPath = ref<string | null>(null);
const hashOut = ref<string | null>(null);
const toolError = ref<string | null>(null);
const rxHits = computed(() => regexTest(rxPattern.value, rxFlags.value, rxText.value));

function doPretty(): void {
  jsonMsg.value = null;
  try {
    jsonOut.value = jsonPretty(jsonIn.value);
  } catch (e) {
    jsonMsg.value = e instanceof Error ? e.message : String(e);
  }
}
function doMinify(): void {
  jsonMsg.value = null;
  try {
    jsonOut.value = jsonMinify(jsonIn.value);
  } catch (e) {
    jsonMsg.value = e instanceof Error ? e.message : String(e);
  }
}
function doValidate(): void {
  const err = jsonError(jsonIn.value);
  jsonMsg.value = err ?? "Valid JSON ✓";
}
function doB64(): void {
  b64Msg.value = null;
  try {
    b64Out.value = b64Mode.value === "encode" ? b64encode(b64In.value) : b64decode(b64In.value);
  } catch (e) {
    b64Msg.value = e instanceof Error ? e.message : String(e);
  }
}
function doUuid(): void {
  uuidOut.value = newUuid();
}
function doEpochToIso(): void {
  const iso = epochToIso(Number(epochIn.value));
  epochOut.value = iso ?? "Invalid epoch";
}
function doIsoToEpoch(): void {
  const ms = isoToEpoch(isoIn.value);
  isoOut.value = ms;
}
function doJwt(): void {
  jwtMsg.value = null;
  try {
    const d = jwtDecode(jwtIn.value);
    jwtOut.value = `HEADER\n${d.header}\n\nPAYLOAD\n${d.payload}\n\n(signature NOT verified — decode only)`;
  } catch (e) {
    jwtMsg.value = e instanceof Error ? e.message : String(e);
  }
}
async function doQr(): Promise<void> {
  qrMsg.value = null;
  qrPath.value = null;
  toolError.value = null;
  try {
    const path = await api.qrPng({ text: qrText.value, size: qrSize.value, ec: qrEc.value, format: qrFormat.value, quiet: qrQuiet.value });
    qrPath.value = path;
    qrMsg.value = `Wrote ${path}`;
  } catch (err) {
    toolError.value = isCommandError(err) ? `${err.kind}: ${err.message}` : String(err);
  }
}
async function revealQr(): Promise<void> {
  if (!qrPath.value) return;
  try {
    await revealInFolder(qrPath.value);
  } catch (err) {
    toolError.value = err instanceof Error ? err.message : String(err);
  }
}
function fillWifi(): void {
  try {
    qrText.value = qrWifi(wifiSec.value, wifiSsid.value, wifiPass.value, wifiHidden.value);
  } catch (e) {
    qrBuildMsg.value = e instanceof Error ? e.message : String(e);
  }
}
function fillUrl(): void {
  qrBuildMsg.value = null;
  try {
    qrText.value = qrUrl(qrText.value);
  } catch (e) {
    qrBuildMsg.value = e instanceof Error ? e.message : String(e);
  }
}
function fillMailto(): void {
  qrBuildMsg.value = null;
  try {
    qrText.value = qrMailto(mailTo.value, mailSubj.value, mailBody.value);
  } catch (e) {
    qrBuildMsg.value = e instanceof Error ? e.message : String(e);
  }
}
function fillSms(): void {
  qrBuildMsg.value = null;
  try {
    qrText.value = qrSms(smsNum.value, smsMsg.value);
  } catch (e) {
    qrBuildMsg.value = e instanceof Error ? e.message : String(e);
  }
}
function fillVcard(): void {
  qrBuildMsg.value = null;
  try {
    qrText.value = qrVcard(vcName.value, vcPhone.value, vcEmail.value);
  } catch (e) {
    qrBuildMsg.value = e instanceof Error ? e.message : String(e);
  }
}
async function doQrDecode(): Promise<void> {
  qrDecoded.value = null;
  toolError.value = null;
  try {
    const picked = await open({ multiple: false });
    const path = Array.isArray(picked) ? picked[0] : picked;
    if (!path) return;
    qrDecoded.value = await api.qrDecode(path);
  } catch (err) {
    toolError.value = isCommandError(err) ? `${err.kind}: ${err.message}` : String(err);
  }
}
async function pickHashFile(): Promise<void> {
  const picked = await open({ multiple: false });
  const path = Array.isArray(picked) ? picked[0] : picked;
  if (path) {
    hashPath.value = path;
    hashOut.value = null;
  }
}
async function doHash(): Promise<void> {
  toolError.value = null;
  if (!hashPath.value) return;
  try {
    hashOut.value = await api.hashFile(hashPath.value);
  } catch (err) {
    toolError.value = isCommandError(err) ? `${err.kind}: ${err.message}` : String(err);
  }
}
</script>

<template>
  <section class="tools" aria-label="Text and data toolbox">
    <p v-if="toolError" class="error" role="alert">{{ toolError }}</p>
    <div class="grid">
      <div class="panel card">
        <h3>JSON</h3>
        <textarea v-model="jsonIn" rows="4" spellcheck="false" aria-label="JSON input" />
        <div class="row">
          <button class="btn ghost sm" @click="doPretty">Pretty</button>
          <button class="btn ghost sm" @click="doMinify">Minify</button>
          <button class="btn ghost sm" @click="doValidate">Validate</button>
        </div>
        <p v-if="jsonMsg" class="dim">{{ jsonMsg }}</p>
        <textarea :value="jsonOut" rows="4" readonly spellcheck="false" aria-label="JSON output" />
      </div>
      <div class="panel card">
        <h3>Base64</h3>
        <div class="row">
          <button class="btn ghost sm" :class="{ on: b64Mode === 'encode' }" @click="b64Mode = 'encode'">Encode</button>
          <button class="btn ghost sm" :class="{ on: b64Mode === 'decode' }" @click="b64Mode = 'decode'">Decode</button>
          <button class="btn ghost sm" @click="doB64">Run</button>
        </div>
        <textarea v-model="b64In" rows="3" spellcheck="false" aria-label="Base64 input" />
        <p v-if="b64Msg" class="error">{{ b64Msg }}</p>
        <textarea :value="b64Out" rows="3" readonly spellcheck="false" aria-label="Base64 output" />
      </div>
      <div class="panel card">
        <h3>UUID · Time</h3>
        <div class="row"><button class="btn ghost sm" @click="doUuid">New UUID v4</button><code v-if="uuidOut">{{ uuidOut }}</code></div>
        <label>ms epoch <input v-model="epochIn" type="text" inputmode="numeric" /><button class="btn ghost sm" @click="doEpochToIso">→ ISO</button></label>
        <p v-if="epochOut" class="dim">{{ epochOut }}</p>
        <label>ISO <input v-model="isoIn" type="text" /><button class="btn ghost sm" @click="doIsoToEpoch">→ epoch</button></label>
        <p v-if="isoOut !== null" class="dim">{{ isoOut }}</p>
      </div>
      <div class="panel card">
        <h3>JWT (decode only)</h3>
        <textarea v-model="jwtIn" rows="2" spellcheck="false" placeholder="header.payload.signature" aria-label="JWT input" />
        <div class="row"><button class="btn ghost sm" @click="doJwt">Decode</button></div>
        <p v-if="jwtMsg" class="error">{{ jwtMsg }}</p>
        <pre v-if="jwtOut" class="out">{{ jwtOut }}</pre>
      </div>
      <div class="panel card">
        <h3>Regex</h3>
        <div class="row">
          <input v-model="rxPattern" aria-label="Pattern" spellcheck="false" />
          <input v-model="rxFlags" aria-label="Flags" class="flags" spellcheck="false" />
        </div>
        <textarea v-model="rxText" rows="2" spellcheck="false" aria-label="Test text" />
        <p v-if="rxHits.error" class="error">{{ rxHits.error }}</p>
        <ul v-else class="hits">
          <li v-for="(h, i) in rxHits.hits" :key="i"><code>{{ h.match }}</code> <span class="dim">@{{ h.index }}</span></li>
        </ul>
      </div>
      <div class="panel card">
        <h3>QR → PNG/SVG</h3>
        <div class="row" role="tablist" aria-label="Payload type">
          <button class="btn ghost sm" :class="{ on: qrKind === 'raw' }" @click="qrKind = 'raw'">Text</button>
          <button class="btn ghost sm" :class="{ on: qrKind === 'url' }" @click="qrKind = 'url'">URL</button>
          <button class="btn ghost sm" :class="{ on: qrKind === 'wifi' }" @click="qrKind = 'wifi'">WiFi</button>
          <button class="btn ghost sm" :class="{ on: qrKind === 'mailto' }" @click="qrKind = 'mailto'">Email</button>
          <button class="btn ghost sm" :class="{ on: qrKind === 'sms' }" @click="qrKind = 'sms'">SMS</button>
          <button class="btn ghost sm" :class="{ on: qrKind === 'vcard' }" @click="qrKind = 'vcard'">vCard</button>
        </div>
        <div v-if="qrKind === 'raw' || qrKind === 'url'" class="row">
          <textarea v-model="qrText" rows="2" spellcheck="false" aria-label="QR text" :placeholder="qrKind === 'url' ? 'https://example.com' : 'Any text'" />
          <button v-if="qrKind === 'url'" class="btn ghost sm" @click="fillUrl">Validate</button>
        </div>
        <div v-if="qrKind === 'wifi'" class="row">
          <label>SSID <input v-model="wifiSsid" type="text" spellcheck="false" /></label>
          <label>Password <input v-model="wifiPass" type="text" spellcheck="false" /></label>
          <label>Security
            <select v-model="wifiSec" aria-label="WiFi security">
              <option value="WPA">WPA</option>
              <option value="WEP">WEP</option>
              <option value="nopass">nopass</option>
            </select>
          </label>
          <label><input v-model="wifiHidden" type="checkbox" /> Hidden</label>
          <button class="btn ghost sm" @click="fillWifi">Fill</button>
        </div>
        <div v-if="qrKind === 'mailto'" class="row">
          <label>To <input v-model="mailTo" type="text" spellcheck="false" /></label>
          <label>Subject <input v-model="mailSubj" type="text" /></label>
          <label>Body <input v-model="mailBody" type="text" /></label>
          <button class="btn ghost sm" @click="fillMailto">Fill</button>
        </div>
        <div v-if="qrKind === 'sms'" class="row">
          <label>Number <input v-model="smsNum" type="text" inputmode="tel" /></label>
          <label>Message <input v-model="smsMsg" type="text" /></label>
          <button class="btn ghost sm" @click="fillSms">Fill</button>
        </div>
        <div v-if="qrKind === 'vcard'" class="row">
          <label>Name <input v-model="vcName" type="text" /></label>
          <label>Phone <input v-model="vcPhone" type="text" inputmode="tel" /></label>
          <label>Email <input v-model="vcEmail" type="text" inputmode="email" /></label>
          <button class="btn ghost sm" @click="fillVcard">Fill</button>
        </div>
        <p v-if="qrBuildMsg" class="error">{{ qrBuildMsg }}</p>
        <div class="row">
          <label>Size <input v-model.number="qrSize" type="number" min="128" max="1024" /></label>
          <label>EC
            <select v-model="qrEc" aria-label="Error correction">
              <option value="L">L (7%)</option>
              <option value="M">M (15%)</option>
              <option value="Q">Q (25%)</option>
              <option value="H">H (30%)</option>
            </select>
          </label>
          <button class="btn ghost sm" :class="{ on: qrFormat === 'png' }" @click="qrFormat = 'png'">PNG</button>
          <button class="btn ghost sm" :class="{ on: qrFormat === 'svg' }" @click="qrFormat = 'svg'">SVG</button>
          <label title="White border around the code (scanners expect it)"><input v-model="qrQuiet" type="checkbox" /> Quiet zone</label>
          <button class="btn ghost sm" @click="doQr">Generate</button>
        </div>
        <p v-if="qrMsg" class="dim">{{ qrMsg }}</p>
        <div class="row">
          <button v-if="qrPath" class="btn ghost sm" @click="revealQr">Reveal file</button>
          <button class="btn ghost sm" @click="doQrDecode">Decode image…</button>
        </div>
        <code v-if="qrDecoded">{{ qrDecoded }}</code>
      </div>
      <div class="panel card">
        <h3>SHA-256 file</h3>
        <div class="row">
          <button class="btn ghost sm" @click="pickHashFile">Choose file</button>
          <button class="btn ghost sm" :disabled="!hashPath" @click="doHash">Hash</button>
        </div>
        <p v-if="hashPath" class="dim">{{ hashPath }}</p>
        <code v-if="hashOut">{{ hashOut }}</code>
      </div>
    </div>
  </section>
</template>

<style scoped>
.tools { display: flex; flex-direction: column; gap: 0.7rem; }
.grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(280px, 1fr)); gap: 0.7rem; }
.panel { padding: 0.9rem; }
.panel h3 { margin: 0 0 0.5rem; font-size: 0.95rem; }
.row { display: flex; gap: 0.4rem; align-items: center; margin: 0.4rem 0; flex-wrap: wrap; }
textarea, input { border: 1px solid var(--line); border-radius: 8px; padding: 0.35rem 0.5rem; font: inherit; width: 100%; }
select { border: 1px solid var(--line); border-radius: 8px; padding: 0.35rem 0.5rem; font: inherit; background: white; }
input { width: auto; flex: 1; min-width: 0; }
input.flags { max-width: 4rem; flex: none; }
label { display: flex; gap: 0.4rem; align-items: center; margin: 0.3rem 0; font-size: 0.85rem; }
.btn.ghost.on { background: var(--ink); color: white; }
.hits { list-style: none; padding: 0; margin: 0.3rem 0; max-height: 120px; overflow-y: auto; }
.out { white-space: pre-wrap; font-size: 0.78rem; max-height: 160px; overflow-y: auto; }
code { font-family: ui-monospace, monospace; font-size: 0.78rem; word-break: break-all; }
.error { color: var(--danger); }
.dim { color: var(--dim); font-size: 0.85rem; }
</style>
