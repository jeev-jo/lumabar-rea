// Writes latest.json for the updater from the signed MSI that `tauri build` made.
// Usage: node scripts/latest-json.mjs "Release notes"
// Upload the .msi and latest.json to the GitHub release for the same version.
import { readFileSync, writeFileSync } from "node:fs";

const conf = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
const version = conf.version;
const msi = `${conf.productName}_${version}_x64_en-US.msi`;
const dir = "src-tauri/target/release/bundle/msi";
const signature = readFileSync(`${dir}/${msi}.sig`, "utf8").trim();

const latest = {
  version,
  notes: process.argv[2] ?? `luma-ai-bar ${version}`,
  pub_date: new Date().toISOString(),
  platforms: {
    "windows-x86_64": {
      signature,
      url: `https://github.com/jeev-jo/lumabar-rea/releases/download/v${version}/${msi}`,
    },
  },
};
writeFileSync(`${dir}/latest.json`, JSON.stringify(latest, null, 2) + "\n");
console.log(`${dir}/latest.json for v${version}`);
