import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const root = process.cwd();
const read = (path) => readFileSync(resolve(root, path), "utf8");
const failures = [];
const styles = read("src/styles.css");

if (styles.includes('source(".")')) {
  failures.push("src/styles.css must use explicit Tailwind source entries, not source(\".\").");
}
if (/\.(?:[a-z][\w-]*)\s*[,{]/.test(styles)) {
  failures.push("src/styles.css may contain only document-level base rules; component selectors belong in utilities.");
}
if (/@apply\b/.test(styles)) {
  failures.push("Tailwind component migration must not introduce @apply.");
}

for (const component of [
  "DeletionReview",
  "DuplicateReview",
  "FolderMonitorPanel",
  "PlaylistManager",
  "PreIngestDuplicatePanel",
  "VideoTitleRename",
]) {
  const css = read(`src/components/${component}.lazy.css`);
  if (!css.includes(`@source "./${component}.tsx"`)) {
    failures.push(`${component}.lazy.css must emit its own statically detected utility CSS.`);
  }
  if (!read(`src/components/${component}.tsx`).includes(`./${component}.lazy.css`)) {
    failures.push(`${component}.tsx must import its lazy Tailwind CSS entry.`);
  }
}

if (failures.length) {
  console.error("Tailwind UI guard failed:\n- " + failures.join("\n- "));
  process.exit(1);
}

console.log("Tailwind UI guard passed.");
