// Run after UI changes with a local Vite server: node scripts/test_ui.mjs [url]
// Uses an isolated browser context and preview data, never desktop snippets.
import { chromium } from "playwright";
import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
const directory = new URL("../docs/ui-tests/", import.meta.url);
await mkdir(directory, { recursive: true });
const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1280, height: 820 } });
const errors = [];
page.on("pageerror", error => errors.push(error.message));
page.on("console", message => { if (message.type() === "error") errors.push(message.text()); });
const results = [];
const url = process.argv[2] || "http://127.0.0.1:4179";
try {
  await page.goto(url);
  await page.getByRole("gridcell").filter({ hasText: "서명 템플릿" }).click();
  await page.getByRole("heading", { name: "스니펫 편집", exact: true }).waitFor();
  await page.screenshot({ path: new URL("desktop.png", directory).pathname.replace(/^\/(\w:)/, "$1") });
  await page.getByRole("button", { name: "편집 취소", exact: true }).click();
  await page.getByRole("button", { name: "새 스니펫", exact: true }).click();
  await page.getByLabel("이름", { exact: true }).fill("회귀 테스트");
  await page.getByLabel("단축어", { exact: true }).fill(":qa");
  await page.getByLabel("치환할 내용", { exact: false }).fill("검증 문구\n둘째 줄");
  await page.getByRole("button", { name: "저장", exact: true }).click();
  await page.getByRole("status").filter({ hasText: "저장했어요" }).waitFor();
  assert.equal(await page.locator(".ag-row").count(), 3);
  results.push("create");
  await page.getByRole("switch", { name: "스니펫 사용" }).click();
  await page.getByRole("button", { name: "저장", exact: true }).click();
  await page.waitForFunction(() => document.querySelector('button[aria-label="사용 중지"]')?.textContent.includes("1"));
  await page.getByRole("button", { name: "편집 취소", exact: true }).click();
  await page.getByRole("button", { name: "사용 중지", exact: true }).click();
  await page.waitForFunction(() => document.querySelectorAll('.ag-row').length === 1);
  assert.equal(await page.locator(".ag-row").count(), 1);
  results.push("disable_and_filter");
  await page.reload();
  await page.getByRole("gridcell").filter({ hasText: "회귀 테스트" }).click();
  assert.equal(await page.getByRole("switch").getAttribute("aria-checked"), "false");
  results.push("reload_persistence");
  await page.getByLabel("단축어", { exact: true }).fill(":challa");
  await page.getByRole("button", { name: "저장", exact: true }).click();
  await page.getByRole("alert").waitFor();
  assert.match(await page.getByRole("alert").textContent(), /이미 등록/);
  results.push("duplicate_error");
  await page.getByLabel("단축어", { exact: true }).fill(":qa");
  for (const width of [720, 390, 325]) {
    await page.setViewportSize({ width, height: 844 });
    const geometry = await page.evaluate(() => ({ width: document.documentElement.scrollWidth, viewport: innerWidth, save: document.querySelector('button[type="submit"]')?.getBoundingClientRect().bottom }));
    assert.ok(geometry.width <= geometry.viewport, JSON.stringify(geometry));
    assert.ok(geometry.save <= 844, JSON.stringify(geometry));
    if (width === 325) {
      const splits = await page.evaluate(() => {
        const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
        let node; const result = [];
        while ((node = walker.nextNode())) {
          if (!node.parentElement?.getClientRects().length || node.parentElement.closest('script, style, textarea, .ag-root')) continue;
          for (const word of node.textContent.matchAll(/[가-힣]{2,}/gu)) {
            const range = document.createRange(); range.setStart(node, word.index); range.setEnd(node, word.index + word[0].length);
            if (new Set([...range.getClientRects()].filter(rect => rect.width > 0).map(rect => Math.round(rect.top))).size > 1) result.push(word[0]);
          }
        }
        return result;
      });
      assert.deepEqual(splits, [], `Korean word fragments: ${splits.join(', ')}`);
    }
    await page.screenshot({ path: new URL(`editor-${width}.png`, directory).pathname.replace(/^\/(\w:)/, "$1") });
  }
  results.push("editor_720_390_325");
  await page.setViewportSize({ width: 1280, height: 820 });
  await page.getByRole("button", { name: "스니펫 삭제", exact: true }).click();
  await page.getByRole("button", { name: "삭제 확인", exact: true }).click();
  await page.getByRole("status").filter({ hasText: "삭제했어요" }).waitFor();
  assert.equal(await page.locator(".ag-row").count(), 2);
  results.push("confirmed_delete");
  await page.getByRole("textbox", { name: "스니펫 검색", exact: true }).fill("없는 검색어");
  await page.getByText("검색 조건에 맞는 스니펫이 없어요").waitFor();
  await page.getByRole("button", { name: "검색 지우기" }).click();
  await page.waitForFunction(() => document.querySelectorAll('.ag-row').length === 2);
  assert.equal(await page.locator(".ag-row").count(), 2);
  results.push("search_empty_restore");
  const broken = await browser.newPage();
  await broken.addInitScript(() => { window.isTauri = true; window.__TAURI_INTERNALS__ = { invoke: () => Promise.reject(new Error("테스트 연결 오류")) }; });
  await broken.goto(url);
  await broken.getByRole("alert").waitFor();
  assert.equal(await broken.locator(".ag-row").count(), 0);
  assert.match(await broken.getByRole("alert").textContent(), /테스트 연결 오류/);
  results.push("native_error_not_masked_by_preview");
  assert.deepEqual(errors, []);
  console.log(JSON.stringify({ passed: true, scenarios: results, errors }, null, 2));
} finally {
  await writeFile(new URL("results.json", directory), JSON.stringify({ scenarios: results, errors }, null, 2));
  await browser.close();
}
