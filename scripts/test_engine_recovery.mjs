// After engine recovery or vault lifecycle UI changes, build and serve the UI:
// pnpm build, then pnpm preview --host 127.0.0.1 --port 4188
// node scripts/test_engine_recovery.mjs http://127.0.0.1:4188
// Isolated mock IPC only. Exit 0 confirms recovery feedback and vault refresh.
// Results: docs/engine-recovery-ui/results.json. Fix failures before packaging.
import { chromium } from "playwright";
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";

const directory = new URL("../docs/engine-recovery-ui/", import.meta.url);
await mkdir(directory, { recursive: true });
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
const errors = [];
page.on("pageerror", error => errors.push(error.message));
await page.addInitScript(() => {
  const fixture = window.recoveryFixture = {
    engine: { active: true, paused: false, vault: "Locked", error: null },
    vault: { status: "Locked", cli_available: true, cached_fields: 0 },
    calls: [], recoveries: 0, finish: null, fail: null,
  };
  let id = 0;
  window.isTauri = true;
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
  window.__TAURI_INTERNALS__ = {
    transformCallback() { return ++id; },
    unregisterCallback() {},
    async invoke(command) {
      fixture.calls.push(command);
      switch (command) {
        case "plugin:event|listen": return ++id;
        case "plugin:event|unlisten": return;
        case "list_snippets": return [];
        case "get_engine_status": return structuredClone(fixture.engine);
        case "get_productivity_state": return { settings: { active_profile: "work", rules: [] }, effective_profile: "work", application: "", readiness: {} };
        case "get_vault_status":
        case "inspect_vault": return structuredClone(fixture.vault);
        case "list_vault_items": return [{ id: "fixture-item", name: "복원 확인 항목", username: "fixture", fields: ["Password"] }];
        case "recover_engine":
          fixture.recoveries++;
          return new Promise((resolve, reject) => {
            fixture.finish = () => { fixture.engine = { active: true, paused: false, vault: "Locked", error: null }; resolve(structuredClone(fixture.engine)); };
            fixture.fail = () => { fixture.engine.active = false; fixture.engine.error = "복구 실패 시험"; reject("복구 실패 시험"); };
          });
        default: throw Error(`Unsupported mock command: ${command}`);
      }
    },
  };
});
try {
  await page.goto(process.argv[2] ?? "http://127.0.0.1:4188");
  const recover = page.getByRole("button", { name: "엔진 복구", exact: true });
  await recover.waitFor();
  await recover.click();
  assert.equal(await recover.isDisabled(), true);
  assert.equal(await recover.getAttribute("aria-busy"), "true");
  assert.equal(await recover.locator(".animate-spin").count(), 1);
  assert.equal(await page.getByRole("button", { name: "엔진 일시 정지", exact: true }).isDisabled(), true);
  await page.evaluate(() => window.recoveryFixture.finish());
  await page.getByRole("status").filter({ hasText: "입력 엔진을 복구했어요" }).waitFor();
  assert.equal(await recover.isEnabled(), true);
  await recover.click();
  await page.evaluate(() => window.recoveryFixture.fail());
  await page.getByRole("status").filter({ hasText: "복구 실패 시험" }).waitFor();
  assert.equal(await recover.isEnabled(), true);
  await recover.click();
  await page.evaluate(() => window.recoveryFixture.finish());
  await page.getByRole("status").filter({ hasText: "입력 엔진을 복구했어요" }).waitFor();
  assert.equal(await page.evaluate(() => window.recoveryFixture.recoveries), 3);
  const measurements = [];
  for (const width of [1440, 720, 390, 325]) {
    await page.setViewportSize({ width, height: 900 });
    const bounds = await recover.boundingBox();
    assert.ok(bounds && bounds.x >= 0 && bounds.x + bounds.width <= width);
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
    measurements.push({ width, recover: bounds });
    await page.screenshot({ path: new URL(`footer-${width}.png`, directory).pathname.replace(/^\/(\w:)/, "$1") });
  }
  await page.getByRole("button", { name: "보관함 열기", exact: true }).click();
  await page.getByRole("button", { name: "잠금 해제", exact: true }).waitFor();
  await page.evaluate(() => { window.recoveryFixture.vault.status = "Unlocked"; window.recoveryFixture.vault.cached_fields = 1; });
  await page.getByText("복원 확인 항목 (fixture)", { exact: true }).waitFor({ timeout: 7000 });
  assert.ok(await page.evaluate(() => window.recoveryFixture.calls.includes("list_vault_items")));
  assert.deepEqual(errors, []);
  const result = { passed: true, scenarios: ["recovery_loading_and_duplicate_prevention", "failure_and_retry_when_inactive", "footer_1440_720_390_325", "vault_items_refresh_after_automatic_resume"], measurements, errors };
  await writeFile(new URL("results.json", directory), JSON.stringify(result, null, 2));
  console.log(JSON.stringify(result));
} finally {
  await browser.close();
}
