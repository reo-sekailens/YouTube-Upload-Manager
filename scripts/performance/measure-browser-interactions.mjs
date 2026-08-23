import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:net";
import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import process from "node:process";

const repositoryRoot = path.resolve(import.meta.dirname, "..", "..");
const sourceFiles = [
  "performance-interactions.html",
  "src/performance-interactions.tsx",
  "src/components/QueueTable.tsx",
  "src/lib/list-windowing.ts",
  "src/lib/performance-interaction-report.ts",
  "src/lib/retained-workspace-state.ts",
];

function argument(name) {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : undefined;
}

const usage = `Usage:
  node scripts/performance/measure-browser-interactions.mjs --output <absolute-json-path>

Runs five untimed warm-up pairs followed by 40 real 10k-title searches and 40
real Batch clear interactions. The output is local-only redacted JSON; a PNG
with the same basename is saved beside it.`;

if (process.argv.includes("--help") || process.argv.includes("-h")) {
  process.stdout.write(`${usage}\n`);
  process.exit(0);
}

const outputValue = argument("--output");
if (!outputValue || !path.isAbsolute(outputValue))
  throw new Error(`--output must be an explicit absolute JSON path.\n\n${usage}`);
const outputPath = path.resolve(outputValue);
if (path.extname(outputPath).toLocaleLowerCase() !== ".json")
  throw new Error("--output must end in .json.");
const screenshotPath = outputPath.replace(/\.json$/i, ".png");

function chromiumExecutable() {
  const candidates = [
    "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
    "C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe",
    "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
    "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
    process.env.LOCALAPPDATA &&
      path.join(process.env.LOCALAPPDATA, "Microsoft", "Edge", "Application", "msedge.exe"),
  ].filter(Boolean);
  const executable = candidates.find(existsSync);
  if (!executable)
    throw new Error("Microsoft Edge or Google Chrome was not found on this Windows host.");
  return executable;
}

async function reservePort() {
  const server = createServer();
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  const port = typeof address === "object" && address ? address.port : undefined;
  await new Promise((resolve) => server.close(resolve));
  if (!port) throw new Error("A loopback port could not be reserved.");
  return port;
}

async function waitForHttp(url, timeoutMs = 30_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url, { cache: "no-store" });
      if (response.ok) return response;
    } catch {
      // Local server startup is expected to race the first request.
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`The local endpoint did not become ready: ${url}`);
}

async function json(url, timeoutMs = 30_000) {
  return (await waitForHttp(url, timeoutMs)).json();
}

class CdpClient {
  constructor(url) {
    this.nextId = 1;
    this.pending = new Map();
    this.socket = new WebSocket(url);
  }

  async connect() {
    await new Promise((resolve, reject) => {
      this.socket.addEventListener("open", resolve, { once: true });
      this.socket.addEventListener("error", reject, { once: true });
    });
    this.socket.addEventListener("message", ({ data }) => {
      const message = JSON.parse(String(data));
      if (!message.id) return;
      const pending = this.pending.get(message.id);
      if (!pending) return;
      this.pending.delete(message.id);
      if (message.error) pending.reject(new Error(message.error.message));
      else pending.resolve(message.result);
    });
  }

  send(method, params = {}) {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.socket.send(JSON.stringify({ id, method, params }));
    });
  }

  close() {
    this.socket.close();
  }
}

function validate(report) {
  const failures = [];
  if (
    report?.schemaVersion !== 1 ||
    report?.evidenceBoundary !== "local-browser-performance-harness" ||
    report?.localOnly !== true ||
    report?.containsSensitiveData !== false
  )
    failures.push("invalid local-only envelope");
  if (report?.fixture?.uploadItems !== 10_000 || report?.fixture?.measuredPairs !== 40)
    failures.push("invalid deterministic fixture");
  if (report?.search10k?.count !== 40) failures.push("missing 40 search samples");
  if (report?.batchClear?.count !== 40) failures.push("missing 40 Batch samples");
  if (report?.samples?.length !== 80) failures.push("missing raw chronological population");
  if (report?.browser?.longTaskObserverSupported !== true) failures.push("Long Tasks API unavailable");
  if (report?.search10k?.p95 >= 100) failures.push("search p95 is not below 100 ms");
  if (report?.batchClear?.p95 >= 100) failures.push("Batch p95 is not below 100 ms");
  if (report?.maximumInteractionLongTaskMs > 50) failures.push("interaction long task exceeds 50 ms");
  if (report?.samples?.some(({ visibleRecords }) => visibleRecords > 32))
    failures.push("bounded queue mounted more than 32 records");
  if (report?.runtimeErrors !== 0) failures.push("page runtime error");
  if (report?.gates?.passed !== true) failures.push("independent in-page gate failed");
  return failures;
}

async function fingerprint() {
  const hash = createHash("sha256");
  for (const relative of sourceFiles) {
    hash.update(relative);
    hash.update("\0");
    hash.update(await readFile(path.join(repositoryRoot, relative)));
    hash.update("\0");
  }
  return hash.digest("hex");
}

async function stop(child) {
  if (!child || child.exitCode !== null) return;
  child.kill();
  await Promise.race([
    new Promise((resolve) => child.once("exit", resolve)),
    new Promise((resolve) => setTimeout(resolve, 5_000)),
  ]);
}

const vitePort = await reservePort();
const debugPort = await reservePort();
const tempBase = path.resolve(os.tmpdir());
const profile = await mkdtemp(path.join(tempBase, "yum-interaction-cert-"));
if (path.dirname(profile) !== tempBase)
  throw new Error("The browser profile escaped the system temporary directory.");
let vite;
let chromium;
let page;
let browser;

try {
  const viteEntry = path.join(repositoryRoot, "node_modules", "vite", "bin", "vite.js");
  if (!existsSync(viteEntry)) throw new Error("The installed Vite entry point is missing.");
  vite = spawn(
    process.execPath,
    [viteEntry, "--host", "127.0.0.1", "--port", String(vitePort), "--strictPort"],
    {
      cwd: repositoryRoot,
      env: { ...process.env, TAURI_ENV_PERFORMANCE_HARNESS: "1" },
      stdio: ["ignore", "pipe", "pipe"],
      windowsHide: true,
    },
  );
  const fixtureUrl = `http://127.0.0.1:${vitePort}/performance-interactions.html`;
  await waitForHttp(fixtureUrl);
  chromium = spawn(
    chromiumExecutable(),
    [
      "--headless=new",
      "--no-first-run",
      "--disable-default-apps",
      "--disable-background-networking",
      "--remote-debugging-address=127.0.0.1",
      `--remote-debugging-port=${debugPort}`,
      `--user-data-dir=${profile}`,
      "--window-size=1440,1200",
      fixtureUrl,
    ],
    { stdio: ["ignore", "pipe", "pipe"], windowsHide: true },
  );

  let targets;
  const targetDeadline = Date.now() + 30_000;
  while (Date.now() < targetDeadline) {
    targets = await json(`http://127.0.0.1:${debugPort}/json/list`);
    if (targets.some((target) => target.type === "page" && target.url === fixtureUrl)) break;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  const target = targets?.find((candidate) => candidate.type === "page" && candidate.url === fixtureUrl);
  if (!target?.webSocketDebuggerUrl)
    throw new Error("Chromium did not expose the local fixture page.");
  page = new CdpClient(target.webSocketDebuggerUrl);
  await page.connect();
  await page.send("Runtime.enable");
  await page.send("Page.enable");

  let report;
  const reportDeadline = Date.now() + 120_000;
  while (Date.now() < reportDeadline) {
    const evaluation = await page.send("Runtime.evaluate", {
      expression: "globalThis.__YUM_PERFORMANCE_INTERACTION_REPORT__ ?? null",
      returnByValue: true,
    });
    report = evaluation?.result?.value;
    if (report) break;
    const state = await page.send("Runtime.evaluate", {
      expression: "document.documentElement.dataset.certification ?? null",
      returnByValue: true,
    });
    if (state?.result?.value === "failed") {
      const failure = await page.send("Runtime.evaluate", {
        expression: "document.documentElement.dataset.certificationFailure ?? 'unknown failure'",
        returnByValue: true,
      });
      throw new Error(`The in-page interaction driver failed: ${failure?.result?.value}.`);
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  if (!report) throw new Error("The 40-pair interaction run timed out.");
  const failures = validate(report);

  const version = await json(`http://127.0.0.1:${debugPort}/json/version`);
  const screenshot = await page.send("Page.captureScreenshot", {
    format: "png",
    fromSurface: true,
    captureBeyondViewport: false,
  });
  const png = Buffer.from(screenshot.data, "base64");
  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(screenshotPath, png);
  const evidence = {
    ...report,
    generatedAt: new Date().toISOString(),
    browser: { ...report.browser, product: version.Browser },
    sourceFingerprintSha256: await fingerprint(),
    screenshotSha256: createHash("sha256").update(png).digest("hex"),
  };
  await writeFile(outputPath, `${JSON.stringify(evidence, null, 2)}\n`, "utf8");
  process.stdout.write(`${JSON.stringify({
    passed: failures.length === 0,
    search10k: evidence.search10k,
    batchClear: evidence.batchClear,
    maximumInteractionLongTaskMs: evidence.maximumInteractionLongTaskMs,
    sampleCount: evidence.samples.length,
    outputPath,
    screenshotPath,
  })}\n`);

  if (failures.length)
    throw new Error(`Interaction certification failed: ${failures.join("; ")}.`);

  if (version.webSocketDebuggerUrl) {
    browser = new CdpClient(version.webSocketDebuggerUrl);
    await browser.connect();
    await browser.send("Browser.close");
  }
} finally {
  page?.close();
  browser?.close();
  await stop(chromium);
  await stop(vite);
  const resolved = path.resolve(profile);
  if (path.dirname(resolved) !== tempBase || !path.basename(resolved).startsWith("yum-interaction-cert-"))
    throw new Error("Refusing to remove an unexpected browser profile path.");
  await rm(resolved, { recursive: true, force: true });
}
