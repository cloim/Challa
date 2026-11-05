// After vault selector changes: node --test scripts/test_vault_search.mjs
import { test } from "node:test";
import assert from "node:assert/strict";
import { searchVaultItems, vaultItemLabel } from "../src/vaultSearch.ts";
const items = [
  { id: "1", name: "네이버", username: "work@example.com", fields: [] },
  { id: "2", name: "네이버", username: "personal@example.com", fields: [] },
  { id: "3", name: "GitHub", username: null, fields: [] },
];
test("duplicate names display their account IDs", () => {
  assert.equal(vaultItemLabel(items[0]), "네이버 (work@example.com)");
  assert.equal(vaultItemLabel(items[2]), "GitHub (아이디 없음)");
});
test("search matches names, IDs, multiple terms and Korean initials", () => {
  assert.deepEqual(searchVaultItems(items, "네이버 WORK").map(i => i.id), ["1"]);
  assert.deepEqual(searchVaultItems(items, "ㄴㅇㅂ personal").map(i => i.id), ["2"]);
  assert.deepEqual(searchVaultItems(items, "github").map(i => i.id), ["3"]);
});
test("clearing search restores the original list without modifying it", () => {
  assert.equal(searchVaultItems(items, "not-found").length, 0);
  assert.deepEqual(searchVaultItems(items, " "), items);
  assert.equal(items.length, 3);
});
