// Run `pnpm icons` after changing src/assets/challa-mark.svg, before the app build.
// Generates Windows resources from the same SVG used by the sidebar and favicon.
import { createRequire } from "node:module";
import { readFileSync, mkdirSync, copyFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const require = createRequire(import.meta.url);
const root = dirname(dirname(fileURLToPath(import.meta.url)));
const cliPackage = require.resolve("@tauri-apps/cli/package.json");
const cli = JSON.parse(readFileSync(cliPackage, "utf8")).bin.tauri;
if (typeof cli !== "string") throw new Error("Tauri CLI bin.tauri is required");
const output = join(root, "docs", "icon-build");
mkdirSync(output, { recursive: true });
const result = spawnSync(process.execPath, [join(dirname(cliPackage), cli), "icon", join(root, "src/assets/challa-mark.svg"), "--output", output], { cwd: root, stdio: "inherit" });
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);
for (const name of ["icon.png", "icon.ico"]) copyFileSync(join(output, name), join(root, "src-tauri/icons", name));
copyFileSync(join(dirname(require.resolve("lucide-react/package.json")), "LICENSE"), join(root, "src/assets/lucide-LICENSE.txt"));
console.log("Challa sidebar SVG -> Windows PNG and ICO");
