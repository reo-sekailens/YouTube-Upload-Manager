import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import {
  collectFrontendBaseline,
  formatFrontendBaselineMarkdown,
} from "../scripts/performance/frontend-baseline.mjs";
import {
  createArtifactReceipt,
  createWindowsHarnessInvocation,
  resolveCargoTargetDirectory,
} from "../scripts/performance/build-windows-harness.mjs";

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

describe("packaged Windows harness launcher", () => {
  it("invokes the installed Tauri JavaScript CLI through Node without a command shell", () => {
    const invocation = createWindowsHarnessInvocation({
      execPath: "C:\\Program Files\\nodejs\\node.exe",
      resolveTauriCli: () => "C:\\repo\\node_modules\\@tauri-apps\\cli\\tauri.js",
      environment: { EXISTING_VALUE: "preserved" },
      extraArguments: ["--config", "fixture.json"],
    });

    expect(invocation).toEqual({
      command: "C:\\Program Files\\nodejs\\node.exe",
      args: [
        "C:\\repo\\node_modules\\@tauri-apps\\cli\\tauri.js",
        "build",
        "--features",
        "performance-harness",
        "--bundles",
        "nsis",
        "--no-sign",
        "--config",
        "fixture.json",
      ],
      options: {
        stdio: "inherit",
        env: {
          EXISTING_VALUE: "preserved",
          TAURI_ENV_PERFORMANCE_HARNESS: "1",
        },
      },
    });
    expect(invocation.command).not.toMatch(/npm(?:\.cmd)?$/i);
    expect(invocation.options).not.toHaveProperty("shell");
  });

  it("keeps cold empty profiles fresh and clones a closed initialized warm template", async () => {
    const runner = await readFile(
      new URL(
        "../scripts/performance/measure-packaged-windows.ps1",
        import.meta.url,
      ),
      "utf8",
    );

    expect(runner).toContain(
      "$coldTemplate = New-RunProfile $resolvedProfileRoot 'cold-template'",
    );
    expect(runner).toContain(
      "Invoke-StartupMeasurement $resolvedExecutable $warmTemplate 'warmup'",
    );
    expect(runner).toContain(
      "$profile = Copy-FixtureTemplate $resolvedProfileRoot $template $profileName",
    );
    expect(runner).toContain("'closed-initialized-after-warmups'");
    expect(runner).toContain("$idleMinimumMilliseconds = 1900");
    expect(runner).toContain("$idleMaximumMilliseconds = 2200");
    expect(runner).toContain("[ValidateRange(40, 200)]");
    expect(runner).toContain("[ValidateRange(5, 20)]");
    expect(runner).toContain("'block-1-cold-first'");
    expect(runner).toContain("'block-2-warm-first'");
    expect(runner).toContain("$webView2ProfileEnvironmentName = 'WEBVIEW2_USER_DATA_FOLDER'");
    expect(runner).toContain("Promote-WarmedWebViewData");
  });

  it("requires idle zeros, clone integrity, provenance, and truthful response receipts", async () => {
    const runner = await readFile(
      new URL(
        "../scripts/performance/measure-packaged-windows.ps1",
        import.meta.url,
      ),
      "utf8",
    );

    expect(runner).toContain(
      "The settled-idle $field delta must be zero in every measured run.",
    );
    expect(runner).toContain("PRAGMA quick_check");
    expect(runner).toContain(
      "A measured profile clone does not match its closed template.",
    );
    expect(runner).toContain(
      "minimumFreeSpaceRule = 'max(20 GiB, 10% of volume capacity), before and after'",
    );
    expect(runner).toContain("firstInteractionResponseMs");
    expect(runner).toContain("firstInteractionLatencyMs");
    expect(runner).toContain("percentileMethod = 'nearest-rank'");
    expect(runner).toContain("outliersRemoved = 0");
    expect(runner).toContain("Native ready p50 | p90 | p95 | max");
    expect(runner).toContain("signedProduction = 'not-exercised'");
    expect(runner).toContain("liveGoogleYouTube = 'not-exercised'");
  });

  it("hashes build artifacts without exposing an absolute source path", async () => {
    const root = await mkdtemp(path.join(tmpdir(), "yum-harness-provenance-"));
    temporaryDirectories.push(root);
    await mkdir(path.join(root, "scripts"), { recursive: true });
    await writeFile(path.join(root, "scripts", "runner.ps1"), "abc", "utf8");

    expect(createArtifactReceipt(root, "scripts/runner.ps1", "runner")).toEqual({
      kind: "runner",
      relativePath: "scripts/runner.ps1",
      bytes: 3,
      sha256: "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD",
    });
  });

  it("resolves Cargo target directories with Cargo environment semantics", () => {
    expect(
      resolveCargoTargetDirectory({
        repositoryRoot: "C:\\repo",
        workingDirectory: "C:\\repo",
        environment: {},
      }),
    ).toBe(path.join("C:\\repo", "src-tauri", "target"));
    expect(
      resolveCargoTargetDirectory({
        repositoryRoot: "C:\\repo",
        workingDirectory: "D:\\build-work",
        environment: { CARGO_TARGET_DIR: "cargo-target" },
      }),
    ).toBe(path.resolve("D:\\build-work", "cargo-target"));
    expect(
      resolveCargoTargetDirectory({
        repositoryRoot: "C:\\repo",
        workingDirectory: "C:\\repo",
        environment: { CARGO_TARGET_DIR: "F:\\yum-target" },
      }),
    ).toBe(path.normalize("F:\\yum-target"));
  });

  it("renders before importing instrumentation and reports unavailable commit counts truthfully", async () => {
    const [entry, harness, app] = await Promise.all([
      readFile(new URL("../src/main.tsx", import.meta.url), "utf8"),
      readFile(
        new URL("../src/performance-harness.ts", import.meta.url),
        "utf8",
      ),
      readFile(new URL("../src/App.tsx", import.meta.url), "utf8"),
    ]);

    const prime = entry.indexOf("void primeStartupBootstrap();");
    const render = entry.indexOf("createRoot(document.getElementById");
    const instrumentation = entry.indexOf('import("./performance-harness")');
    expect(prime).toBeGreaterThan(-1);
    expect(render).toBeGreaterThan(prime);
    expect(instrumentation).toBeGreaterThan(render);
    expect(harness).toContain("reactCommits: null");
    expect(harness).toContain('sendMilestone("safe_shell_paint"');
    expect(harness).toContain("safeShellPaintMs:");
    expect(harness).toContain("await receipt();");
    expect(harness).toContain("firstInteractionResponseMs:");
    expect(harness).toContain("firstInteractionLatencyMs:");
    expect(harness).toContain('"batch_search_10k"');
    expect(harness).toContain("pendingSearchKey.startedAtMs");
    expect(harness).toContain(
      'document.addEventListener("keydown", beginBatchSearchKey, true)',
    );
    expect(harness).toContain(
      'const batchContentSelector = \'[data-performance-batch-content="ready"]\';',
    );
    expect(app).toMatch(
      /requestAnimationFrame\(\(\) => \{[\s\S]*prefetchBatchWorkspace\(\)[\s\S]*requestAnimationFrame\(\(\) => \{[\s\S]*completeStartupAfterSafeShell\(\)/,
    );
    expect(app).toMatch(
      /performance\.mark\("\[data-performance-shell\]"\)[\s\S]*completeStartupAfterSafeShellPaint\([\s\S]*completeStartupAfterSafeShell/,
    );
  });
});

function rootSensitivePath(distDir: string) {
  return path.dirname(distDir).replaceAll("\\", "/");
}
