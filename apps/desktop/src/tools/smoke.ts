// DOM-free smoke for text/data tools (run with bun).
import { jsonPretty, jsonMinify, jsonError, b64encode, b64decode, b64urlDecode, newUuid, epochToIso, isoToEpoch, jwtDecode, regexTest, qrWifi, qrUrl, qrMailto, qrSms, qrVcard } from "./texttools";

let pass = 0;
let fail = 0;
function check(name: string, cond: boolean): void {
  if (cond) pass += 1;
  else {
    fail += 1;
    console.error(`FAIL: ${name}`);
  }
}

check("pretty roundtrip", jsonMinify(jsonPretty('{"b":2,"a":1}')) === '{"b":2,"a":1}');
check("pretty indents", jsonPretty('{"a":1}').includes("\n"));
check("json error null on valid", jsonError('{"a":1}') === null);
check("json error string on bad", (jsonError("{bad") ?? "").length > 0);
check("b64 roundtrip ascii", b64decode(b64encode("hello")) === "hello");
check("b64 roundtrip unicode", b64decode(b64encode("မင်္ဂလာပါ こんにちは")) === "မင်္ဂလာပါ こんにちは");
check("b64url jwt payload", (() => {
  const payload = b64urlDecode("eyJzdWIiOiIxMjM0In0");
  return payload.includes("1234");
})());
check("uuid shape", /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(newUuid()));
check("epoch 0", epochToIso(0) === "1970-01-01T00:00:00.000Z");
check("epoch bad", epochToIso(NaN) === null);
check("iso roundtrip", isoToEpoch("1970-01-01T00:00:00.000Z") === 0);
check("iso bad", isoToEpoch("not a date") === null);
check("jwt decode", (() => {
  const t = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0In0.sig";
  const d = jwtDecode(t);
  return d.header.includes("HS256") && d.payload.includes("1234");
})());
let threw = false;
try {
  jwtDecode("onlyone");
} catch {
  threw = true;
}
check("jwt bad throws", threw);
check("regex finds", regexTest("\\d+", "g", "a1b22").hits.length === 2);
check("regex bad pattern", regexTest("([", "g", "x").error !== null);
check("regex empty-match guard", regexTest("x*", "g", "ab").hits.length <= 50);
check("wifi basic", qrWifi("WPA", "Cafe", "secret", false) === "WIFI:T:WPA;S:Cafe;P:secret;;");
check("wifi escape", qrWifi("WPA", "a;b:c,d\\e", "p", false) === "WIFI:T:WPA;S:a\\;b\\:c\\,d\\\\e;P:p;;");
check("wifi nopass", qrWifi("nopass", "Open", "", false) === "WIFI:T:nopass;S:Open;;");
check("wifi hidden", qrWifi("WPA", "S", "p", true).includes("H:true;"));
check("url ok", qrUrl("https://example.com/x") === "https://example.com/x");
let urlBad = false;
try { qrUrl("example.com"); } catch { urlBad = true; }
check("url rejects scheme-less", urlBad);
check("mailto full", qrMailto("a@b.co", "Hi", "Yo") === "mailto:a@b.co?subject=Hi&body=Yo");
check("sms plain", qrSms("+123", "") === "sms:+123");
check("sms body", qrSms("123", "hi you") === "sms:123?body=hi%20you");
let smsBad = false;
try { qrSms("abc", ""); } catch { smsBad = true; }
check("sms rejects letters", smsBad);
check("vcard minimal", qrVcard("Ada", "", "").includes("FN:Ada"));
check("vcard fold", qrVcard("A".repeat(100), "", "").includes("\r\n "));

console.log(`\n${pass} passed, ${fail} failed`);
if (fail > 0) throw new Error(`${fail} failed`);
