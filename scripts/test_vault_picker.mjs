// After vault picker changes: node scripts/test_vault_picker.mjs
// Finite headless test with a disposable context and synthetic data only.
import { createServer } from "vite";
import { chromium } from "playwright";
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { spawn } from "node:child_process";
import { homedir } from "node:os";
import { join } from "node:path";
const port = 4183;
const directory = new URL("../docs/vault-picker-ui/", import.meta.url);
await mkdir(directory, { recursive: true });
const server = await createServer({ server: { host: "127.0.0.1", port, strictPort: true } });
let browser;
const passed = [], errors = [];
try {
  await server.listen();
  browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1280, height: 820 } });
  page.on("pageerror", error => errors.push(error.message));
  const url = `http://127.0.0.1:${port}/scripts/fixtures/vault-picker.html`;
  await page.goto(url);
  const input = page.getByRole("combobox");
  await input.click();
  await input.fill("138");
  await page.getByRole("option", { name: "테스트 계정 138 (user138@example.com)", exact: true }).click();
  await page.locator("[data-selected]").filter({ hasText: "138" }).waitFor();
  assert.equal(await input.inputValue(), "테스트 계정 138 (user138@example.com)");
  passed.push("name and username labels, search and pointer selection");
  await input.click();
  await input.fill("ㄴㅇㅂ personal");
  assert.equal(await page.getByRole("option").count(), 1);
  await input.press("Enter");
  assert.equal(await page.locator("[data-selected]").textContent(), "personal");
  assert.equal(await page.locator("[data-submits]").textContent(), "0");
  passed.push("Korean initials and multi-term search, Enter selects without submitting");
  await input.click();
  await input.fill("not-found");
  await page.getByText("검색 결과가 없어요").waitFor();
  await input.press("Escape");
  assert.equal(await input.inputValue(), "네이버 (personal@example.com)");
  await input.click(); await input.fill("");
  assert.equal(await page.getByRole("option").count(), 142);
  await input.press("Escape");
  passed.push("empty results, Escape and clearing search preserve selection and restore rows");
  await page.getByRole("button", { name: "잠금 전환" }).click();
  assert.equal(await input.isDisabled(), true);
  await page.getByRole("button", { name: "잠금 전환" }).click();
  await page.getByRole("button", { name: "로딩 전환" }).click();
  assert.equal(await input.isDisabled(), true);
  await page.getByRole("button", { name: "로딩 전환" }).click();
  passed.push("lock and loading disable selection");
  for (const width of [1280, 390, 325]) {
    await page.setViewportSize({ width, height: 820 });
    await input.click(); await input.fill("ㄴㅇㅂ");
    const bounds = await page.locator("[data-ui-audit-overlay]").boundingBox();
    const anchor = await input.locator("..").boundingBox();
    assert(bounds && bounds.x >= 0 && bounds.x + bounds.width <= width + 1 && bounds.y >= 0 && bounds.y + bounds.height <= 821);
    assert(anchor && (Math.abs(bounds.y + bounds.height - anchor.y) <= 8 || Math.abs(bounds.y - anchor.y - anchor.height) <= 8));
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    await page.screenshot({ path: new URL(`selector-${width}.png`, directory).pathname.replace(/^\/([A-Z]:)/, "$1") });
    await input.press("Escape");
  }
  passed.push("popup fits desktop, 390px and 325px viewports");
  assert.deepEqual(errors, []);
  await writeFile(new URL("results.json", directory), JSON.stringify({ passed, errors }, null, 2));
  console.log(JSON.stringify({ passed }));
  await browser.close(); browser = null;
  const audit = await new Promise(resolve => {
    const child = spawn(process.execPath, [join(homedir(), ".agents/skills/balance/scripts/ui-audit.mjs"), "--url", url, "--project", process.cwd(), "--json"], { stdio: ["ignore", "pipe", "pipe"] });
    let out = "", err = "";
    child.stdout.on("data", chunk => out += chunk); child.stderr.on("data", chunk => err += chunk);
    child.on("error", () => resolve({ code: 1, out: "", err: "audit launch failed" }));
    child.on("close", code => resolve({ code, out, err }));
  });
  await writeFile(new URL("ui-audit.json", directory), JSON.stringify(audit, null, 2));
  assert.equal(audit.code, 0, audit.err || audit.out);
  console.log("balance audit passed");
} finally {
  if (browser) await browser.close();
  await server.close();
}
