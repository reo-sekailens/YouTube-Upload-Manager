import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import {
  collectFrontendBaseline,
  formatFrontendBaselineMarkdown,
} from "../scripts/performance/frontend-baseline.mjs";

const temporaryDirectories: string[] = [];

async function createFixture() {
  const root = await mkdtemp(path.join(tmpdir(), "youtube-upload-manager-performance-"));
  temporaryDirectories.push(root);
  const distDir = path.join(root, "dist");
  await mkdir(path.join(distDir, ".vite"), { recursive: true });
  await mkdir(path.join(distDir, "assets"), { recursive: true });
  await writeFile(
    path.join(distDir, ".vite", "manifest.json"),
    JSON.stringify({
      "src/main.tsx": {
        file: "assets/entry.js",
        isEntry: true,
        imports: ["_shared.js"],
        dynamicImports: ["src/optional.ts"],
        css: ["assets/entry.css"],
      },
      "_shared.js": { file: "assets/shared.js" },
      "src/optional.ts": { file: "assets/optional.js", isDynamicEntry: true },
    }),
  );
  await writeFile(path.join(distDir, "assets", "entry.js"), "import './shared.js';\nconsole.log('entry');\n");
  await writeFile(path.join(distDir, "assets", "shared.js"), "export const shared = true;\n");
  await writeFile(path.join(distDir, "assets", "optional.js"), "console.log('optional');\n");
  await writeFile(path.join(distDir, "assets", "entry.css"), "body { color: #123456; }\n");
  const packageJsonPath = path.join(root, "package.json");
  await writeFile(packageJsonPath, JSON.stringify({ name: "fixture-app", version: "1.2.3" }));
  return { distDir, packageJsonPath };
}

afterEach(async () => {
  await Promise.all(temporaryDirectories.splice(0).map((directory) => rm(directory, { recursive: true, force: true })));
});

describe("frontend bundle baseline", () => {
  it("follows static manifest imports while excluding lazy chunks from the initial payload", async () => {
    const fixture = await createFixture();
    const result = await collectFrontendBaseline(fixture);

    expect(result.app).toEqual({ name: "fixture-app", version: "1.2.3" });
    expect(result.bundles.initial.javascript.files.map((file) => file.path)).toEqual([
      "assets/entry.js",
      "assets/shared.js",
    ]);
    expect(result.bundles.all.javascript.files.map((file) => file.path)).toEqual([
      "assets/entry.js",
      "assets/optional.js",
      "assets/shared.js",
    ]);
    expect(result.bundles.initial.css.files.map((file) => file.path)).toEqual(["assets/entry.css"]);
  });

  it("emits deterministic redacted data and evaluates explicit budgets", async () => {
    const fixture = await createFixture();
    const budgets = {
      initialJavaScriptRawBytes: 1,
      initialJavaScriptGzipBytes: 1,
      initialCssRawBytes: 1,
    };
    const first = await collectFrontendBaseline({ ...fixture, budgets });
    const second = await collectFrontendBaseline({ ...fixture, budgets });

    expect(first).toEqual(second);
    expect(first.budgets.passed).toBe(false);
    expect(first.budgets.checks.every((check) => !check.passed)).toBe(true);
    expect(JSON.stringify(first)).not.toContain(rootSensitivePath(fixture.distDir));
    expect(formatFrontendBaselineMarkdown(first)).toContain("Budget status: over budget");
  });
});

function rootSensitivePath(distDir: string) {
  return path.dirname(distDir).replaceAll("\\", "/");
}
