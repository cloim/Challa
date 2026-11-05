// After desktop UI changes, with pnpm preview on an isolated local port:
// node scripts/test_desktop_ui.mjs http://127.0.0.1:4179
// Headless, preview data only. Does not connect to the desktop app or Windows settings.
import { chromium } from "playwright";
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
const directory = new URL("../docs/desktop-ui/", import.meta.url);
await mkdir(directory, { recursive: true });
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
const errors = [], passed = [];
page.on("pageerror", error => errors.push(error.message));
const url = process.argv[2] ?? "http://127.0.0.1:4179";
const screenshot = name => page.screenshot({ path: new URL(`${name}.png`, directory).pathname.replace(/^\/(\w:)/, "$1") });
try {
  await page.goto(url);
  await page.getByRole("gridcell").filter({ hasText: "서명 템플릿" }).waitFor();
  assert.equal(await page.getByRole("button", { name: "사용 안내", exact: true }).count(), 0);
  assert.equal(await page.getByText("내 컴퓨터에서, 안전하게", { exact: true }).count(), 0);
  assert.equal(await page.getByLabel("스니펫 편집기", { exact: true }).count(), 0);
  passed.push("functional_chrome_without_decorative_panel");
  await screenshot("library");
  await page.getByRole("button", { name: "앱 설정", exact: true }).click();
  const close = page.getByRole("switch", { name: "닫기 버튼으로 트레이에 숨기기", exact: true });
  await close.waitFor();
  assert.equal(await close.getAttribute("aria-checked"), "true");
  await close.click();
  await page.getByRole("status").filter({ hasText: "설정을 저장했어요" }).waitFor();
  assert.equal(await close.getAttribute("aria-checked"), "false");
  await page.reload();
  assert.equal(await close.getAttribute("aria-checked"), "false");
  assert.equal(await page.getByRole("switch", { name: "Windows 시작 시 자동 실행", exact: true }).isDisabled(), true);
  passed.push("preferences_save_reload_and_preview_isolation");
  await screenshot("settings");
  const measurements = [];
  for (const width of [1440, 720, 390, 325]) {
    await page.setViewportSize({ width, height: 900 });
    const bounds = await page.evaluate(() => ({ width: innerWidth, overflowX: document.documentElement.scrollWidth > innerWidth, overflowY: document.documentElement.scrollHeight > innerHeight }));
    assert.equal(bounds.overflowX, false, JSON.stringify(bounds));
    assert.equal(bounds.overflowY, false, JSON.stringify(bounds));
    assert.ok(await close.isVisible()); measurements.push(bounds);
    await screenshot(`settings-${width}`);
  }
  passed.push("settings_1440_720_390_325");
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.getByRole("button", { name: "모든 스니펫", exact: true }).click();
  await page.getByRole("gridcell").filter({ hasText: "서명 템플릿" }).click();
  await page.getByRole("heading", { name: "스니펫 편집", exact: true }).waitFor();
  await screenshot("editor");
  const contrast = await page.evaluate(() => {
    const channel = v => { const s = v / 255; return s <= .04045 ? s / 12.92 : ((s + .055) / 1.055) ** 2.4; };
    const luminance = color => { const [r,g,b] = color.match(/[\d.]+/g).slice(0,3).map(Number).map(channel); return .2126*r + .7152*g + .0722*b; };
    const ratio = (a,b) => (Math.max(luminance(a),luminance(b))+.05)/(Math.min(luminance(a),luminance(b))+.05);
    const root = document.documentElement, style = getComputedStyle(root);
    const resolve = key => { const test = document.createElement("span"); test.style.color = style.getPropertyValue(key); document.body.append(test); const c = getComputedStyle(test).color; test.remove(); return c; };
    return { body: ratio(resolve("--color-ink"),resolve("--color-canvas")), secondary: ratio(resolve("--color-muted"),resolve("--color-panel")), inputBoundary: ratio(resolve("--color-input-border"),resolve("--color-input")) };
  });
  assert.ok(contrast.body >= 7); assert.ok(contrast.secondary >= 7); assert.ok(contrast.inputBoundary >= 3);
  passed.push("text_and_control_contrast");
  assert.deepEqual(errors, []);
  const result = { passed: true, scenarios: passed, contrast, measurements, errors };
  await writeFile(new URL("results.json", directory), JSON.stringify(result, null, 2));
  console.log(JSON.stringify(result));
} finally { await browser.close(); }
