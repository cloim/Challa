// After DataGrid selection changes: node scripts/test_grid_selection.mjs
// Finite regression test. Runs an isolated server with synthetic rows only.
import { createServer } from "vite";
import { chromium } from "playwright";
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { spawn } from "node:child_process";
import { homedir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const directory = new URL("../docs/grid-selection/", import.meta.url);
await mkdir(directory, { recursive: true });
const server = await createServer({ server: { host: "127.0.0.1", port: 4184, strictPort: true } });
let browser;
const passed = [], errors = [];
try {
  await server.listen();
  browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1280, height: 820 } });
  page.on("pageerror", error => errors.push(error.message));
  const url = "http://127.0.0.1:4184/scripts/fixtures/grid-selection.html";
  await page.goto(url);
  const selected = page.locator(".ag-row.selected-snippet");
  async function checkSelection(id) {
    await page.waitForFunction(expected => {
      const rows = [...document.querySelectorAll(".ag-row.selected-snippet")];
      return expected === null ? rows.length === 0 : rows.length === 1 && rows[0].getAttribute("row-id") === expected;
    }, id, { timeout: 3000 });
  }
  for (const id of ["0", "1", "2", "3", "0"]) {
    await page.locator(`.ag-row[row-id="${id}"] .ag-cell`).click();
    await checkSelection(id);
  }
  passed.push("sequential pointer selection removes previous row highlight");
  await page.locator('.ag-row[row-id="4"] .ag-cell').focus();
  await page.keyboard.press("Enter");
  await checkSelection("4");
  passed.push("Enter selection removes previous row highlight");
  await page.getByRole("button", { name: "선택 해제" }).click();
  await checkSelection(null);
  passed.push("clearing selectedId removes highlight");
  await page.locator('.ag-row[row-id="1"] .ag-cell').click();
  await checkSelection("1");
  const input = page.getByRole("textbox", { name: "검색" });
  await input.fill("테스트 02");
  await checkSelection(null);
  await page.locator('.ag-row[row-id="2"] .ag-cell').click();
  await checkSelection("2");
  await input.fill("");
  await checkSelection("2");
  passed.push("filter and restore keep only the current selection");
  await page.getByRole("columnheader", { name: "항목" }).click();
  await checkSelection("2");
  const viewport = page.locator(".ag-grid-viewport");
  await viewport.evaluate(element => { element.scrollTop = element.scrollHeight; });
  await page.locator('.ag-row[row-id="59"]').waitFor();
  await viewport.evaluate(element => { element.scrollTop = 0; });
  await checkSelection("2");
  assert.equal(await selected.count(), 1);
  passed.push("sorting and virtual row recycling preserve selection");
  await page.screenshot({ path: fileURLToPath(new URL("selection.png", directory)) });
  assert.deepEqual(errors, []);
  await browser.close(); browser = null;
  const audit = await new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [join(homedir(), ".agents/skills/balance/scripts/ui-audit.mjs"), "--url", url, "--project", process.cwd(), "--json"], { stdio: ["ignore", "pipe", "pipe"] });
    let out = "", err = "";
    child.stdout.on("data", chunk => out += chunk); child.stderr.on("data", chunk => err += chunk);
    child.on("error", reject); child.on("close", code => resolve({ code, out, err }));
  });
  await writeFile(new URL("ui-audit.json", directory), JSON.stringify(audit, null, 2));
  assert.equal(audit.code, 0, audit.err || audit.out);
  passed.push("desktop and mobile balance audit");
  console.log(JSON.stringify({ passed, errors }));
} finally {
  await writeFile(new URL("results.json", directory), JSON.stringify({ passed, errors }, null, 2));
  if (browser) await browser.close();
  await server.close();
}
