import { brotliCompressSync, constants, gzipSync } from "node:zlib";
import { mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";

export const FRONTEND_BUNDLE_BUDGETS = Object.freeze({
  initialJavaScriptRawBytes: 235 * 1024,
  initialJavaScriptGzipBytes: 70 * 1024,
  initialCssRawBytes: 40 * 1024,
});

const MANIFEST_PATH = path.join(".vite", "manifest.json");

function toPosix(value) {
  return value.split(path.sep).join("/");
}

function assetKind(assetPath) {
  if (assetPath.endsWith(".js") || assetPath.endsWith(".mjs")) return "javascript";
  if (assetPath.endsWith(".css")) return "css";
  return "other";
}

async function fileMeasurement(distDir, assetPath) {
  const normalized = assetPath.replace(/^[/\\]+/, "");
  const contents = await readFile(path.join(distDir, normalized));
  return {
    path: toPosix(normalized),
    rawBytes: contents.byteLength,
    gzipBytes: gzipSync(contents, { level: 9 }).byteLength,
    brotliBytes: brotliCompressSync(contents, {
      params: {
        [constants.BROTLI_PARAM_QUALITY]: 11,
      },
    }).byteLength,
  };
}

function summarize(files) {
  return {
    files,
    rawBytes: files.reduce((total, file) => total + file.rawBytes, 0),
    gzipBytes: files.reduce((total, file) => total + file.gzipBytes, 0),
    brotliBytes: files.reduce((total, file) => total + file.brotliBytes, 0),
  };
}

async function listAssets(directory, root = directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const assets = [];

  for (const entry of entries.sort((left, right) => left.name.localeCompare(right.name))) {
    const absolutePath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      assets.push(...(await listAssets(absolutePath, root)));
    } else if (entry.isFile()) {
      const relativePath = toPosix(path.relative(root, absolutePath));
      if (assetKind(relativePath) !== "other") assets.push(relativePath);
    }
  }

  return assets;
}

function initialAssetPaths(manifest) {
  const assets = new Set();
  const visited = new Set();

  function visit(key) {
    if (visited.has(key)) return;
    visited.add(key);

    const chunk = manifest[key];
    if (!chunk) throw new Error(`Vite manifest references missing chunk ${JSON.stringify(key)}.`);
    if (chunk.file && assetKind(chunk.file) !== "other") assets.add(chunk.file);
    for (const cssPath of chunk.css ?? []) assets.add(cssPath);
    for (const importedKey of chunk.imports ?? []) visit(importedKey);
  }

  const entryKeys = Object.keys(manifest)
    .filter((key) => manifest[key]?.isEntry)
    .sort();
  if (entryKeys.length === 0) throw new Error("Vite manifest contains no entry chunk.");
  for (const key of entryKeys) visit(key);

  return [...assets].sort();
}

async function measureGroups(distDir, assetPaths) {
  const measured = await Promise.all(assetPaths.map((assetPath) => fileMeasurement(distDir, assetPath)));
  const javascript = measured.filter((file) => assetKind(file.path) === "javascript");
  const css = measured.filter((file) => assetKind(file.path) === "css");
  return {
    javascript: summarize(javascript),
    css: summarize(css),
  };
}

function evaluateBudgets(initial, budgets) {
  const checks = [
    {
      metric: "initialJavaScriptRawBytes",
      actualBytes: initial.javascript.rawBytes,
      maximumBytes: budgets.initialJavaScriptRawBytes,
    },
    {
      metric: "initialJavaScriptGzipBytes",
      actualBytes: initial.javascript.gzipBytes,
      maximumBytes: budgets.initialJavaScriptGzipBytes,
    },
    {
      metric: "initialCssRawBytes",
      actualBytes: initial.css.rawBytes,
      maximumBytes: budgets.initialCssRawBytes,
    },
  ].map((check) => ({ ...check, passed: check.actualBytes <= check.maximumBytes }));

  return {
    passed: checks.every((check) => check.passed),
    checks,
  };
}

export async function collectFrontendBaseline({
  distDir,
  packageJsonPath,
  budgets = FRONTEND_BUNDLE_BUDGETS,
}) {
  const [manifestSource, packageSource, allAssetPaths] = await Promise.all([
    readFile(path.join(distDir, MANIFEST_PATH), "utf8"),
    readFile(packageJsonPath, "utf8"),
    listAssets(distDir),
  ]);
  const manifest = JSON.parse(manifestSource);
  const packageJson = JSON.parse(packageSource);
  const [initial, all] = await Promise.all([
    measureGroups(distDir, initialAssetPaths(manifest)),
    measureGroups(distDir, allAssetPaths),
  ]);

  return {
    schemaVersion: 1,
    app: {
      name: packageJson.name,
      version: packageJson.version,
    },
    bundles: { initial, all },
    budgets: evaluateBudgets(initial, budgets),
  };
}

function kib(bytes) {
  return `${(bytes / 1024).toFixed(2)} KiB`;
}

export function formatFrontendBaselineMarkdown(result) {
  const rows = result.budgets.checks.map((check) =>
    `| ${check.metric} | ${kib(check.actualBytes)} | ${kib(check.maximumBytes)} | ${check.passed ? "pass" : "over"} |`,
  );

  return [
    "# Frontend bundle baseline",
    "",
    `- Application: ${result.app.name} ${result.app.version}`,
    `- Budget status: ${result.budgets.passed ? "pass" : "over budget"}`,
    "",
    "## Initial render assets",
    "",
    "| Asset type | Files | Raw | Gzip | Brotli |",
    "| --- | ---: | ---: | ---: | ---: |",
    `| JavaScript | ${result.bundles.initial.javascript.files.length} | ${kib(result.bundles.initial.javascript.rawBytes)} | ${kib(result.bundles.initial.javascript.gzipBytes)} | ${kib(result.bundles.initial.javascript.brotliBytes)} |`,
    `| CSS | ${result.bundles.initial.css.files.length} | ${kib(result.bundles.initial.css.rawBytes)} | ${kib(result.bundles.initial.css.gzipBytes)} | ${kib(result.bundles.initial.css.brotliBytes)} |`,
    "",
    "## Budgets",
    "",
    "| Metric | Actual | Maximum | Result |",
    "| --- | ---: | ---: | --- |",
    ...rows,
    "",
    "The report contains bundle measurements only; it contains no runtime, account, media, or provider data.",
    "",
  ].join("\n");
}

function parseArguments(argv) {
  const options = {
    distDir: path.resolve("dist"),
    outputPrefix: path.resolve("output", "performance", "frontend-bundle-baseline"),
    check: false,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--check") {
      options.check = true;
    } else if (argument === "--dist" || argument === "--output") {
      const value = argv[index + 1];
      if (!value) throw new Error(`${argument} requires a path.`);
      if (argument === "--dist") options.distDir = path.resolve(value);
      else options.outputPrefix = path.resolve(value);
      index += 1;
    } else {
      throw new Error(`Unknown argument ${JSON.stringify(argument)}.`);
    }
  }

  return options;
}

export async function runFrontendBaselineCli(argv = process.argv.slice(2)) {
  const options = parseArguments(argv);
  const result = await collectFrontendBaseline({
    distDir: options.distDir,
    packageJsonPath: path.resolve("package.json"),
  });
  await mkdir(path.dirname(options.outputPrefix), { recursive: true });
  await Promise.all([
    writeFile(`${options.outputPrefix}.json`, `${JSON.stringify(result, null, 2)}\n`, "utf8"),
    writeFile(`${options.outputPrefix}.md`, formatFrontendBaselineMarkdown(result), "utf8"),
  ]);

  const relativePrefix = path.relative(process.cwd(), options.outputPrefix) || options.outputPrefix;
  console.log(
    `Frontend baseline recorded at ${relativePrefix}.{json,md}: ` +
      `${kib(result.bundles.initial.javascript.rawBytes)} JS raw, ` +
      `${kib(result.bundles.initial.javascript.gzipBytes)} JS gzip, ` +
      `${kib(result.bundles.initial.css.rawBytes)} CSS raw; ` +
      `budget ${result.budgets.passed ? "pass" : "over"}.`,
  );

  return options.check && !result.budgets.passed ? 1 : 0;
}

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : "";
if (invokedPath === import.meta.url) {
  runFrontendBaselineCli()
    .then((exitCode) => {
      process.exitCode = exitCode;
    })
    .catch((error) => {
      console.error(error instanceof Error ? error.message : error);
      process.exitCode = 1;
    });
}
