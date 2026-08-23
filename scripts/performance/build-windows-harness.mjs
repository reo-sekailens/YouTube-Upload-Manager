import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const defaultRepositoryRoot = path.resolve(scriptDirectory, "..", "..");

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex").toUpperCase();
}

export function createArtifactReceipt(repositoryRoot, relativePath, kind) {
  const absolutePath = path.join(repositoryRoot, relativePath);
  return createArtifactReceiptAtPath(absolutePath, relativePath, kind);
}

function createArtifactReceiptAtPath(absolutePath, relativePath, kind) {
  if (!existsSync(absolutePath) || !statSync(absolutePath).isFile()) {
    throw new Error(`Required ${kind} artifact is missing: ${relativePath}`);
  }
  const bytes = readFileSync(absolutePath);
  return {
    kind,
    relativePath: relativePath.replaceAll("\\", "/"),
    bytes: bytes.length,
    sha256: sha256(bytes),
  };
}

export function resolveCargoTargetDirectory({
  repositoryRoot = defaultRepositoryRoot,
  workingDirectory = process.cwd(),
  environment = process.env,
} = {}) {
  const configured = environment.CARGO_TARGET_DIR?.trim();
  if (!configured) return path.join(repositoryRoot, "src-tauri", "target");
  return path.isAbsolute(configured)
    ? path.normalize(configured)
    : path.resolve(workingDirectory, configured);
}

function gitValue(repositoryRoot, args) {
  const result = spawnSync("git", ["-C", repositoryRoot, ...args], {
    encoding: "utf8",
    windowsHide: true,
  });
  if (result.status !== 0) return "unavailable";
  return result.stdout.trim();
}

export function writeWindowsHarnessBuildManifest({
  repositoryRoot = defaultRepositoryRoot,
  capturedAtUtc = new Date().toISOString(),
  workingDirectory = process.cwd(),
  environment = process.env,
} = {}) {
  const cargoTargetDirectory = resolveCargoTargetDirectory({
    repositoryRoot,
    workingDirectory,
    environment,
  });
  const required = [
    ["scripts/performance/measure-packaged-windows.ps1", "runner"],
    ["scripts/performance/build-windows-harness.mjs", "builder"],
    ["package.json", "source"],
    ["package-lock.json", "lockfile"],
    ["src-tauri/Cargo.toml", "source"],
    ["src-tauri/Cargo.lock", "lockfile"],
    ["src-tauri/tauri.conf.json", "source"],
    ["src-tauri/binaries/ffprobe-license.txt", "license"],
  ];
  const binariesDirectory = path.join(repositoryRoot, "src-tauri", "binaries");
  for (const name of readdirSync(binariesDirectory).sort()) {
    if (/^ffprobe-.+\.exe$/i.test(name)) {
      required.push([`src-tauri/binaries/${name}`, "ffprobe"]);
    }
  }
  const nsisDirectory = path.join(
    cargoTargetDirectory,
    "release",
    "bundle",
    "nsis",
  );
  const installers = existsSync(nsisDirectory)
    ? readdirSync(nsisDirectory).filter((name) => name.toLowerCase().endsWith(".exe"))
    : [];
  if (installers.length !== 1) {
    throw new Error(`Expected exactly one unsigned NSIS harness installer, found ${installers.length}.`);
  }
  const repositoryArtifacts = required.map(([relativePath, kind]) =>
    createArtifactReceipt(repositoryRoot, relativePath, kind),
  );
  const targetArtifacts = [
    createArtifactReceiptAtPath(
      path.join(cargoTargetDirectory, "release", "youtube-upload-manager.exe"),
      "cargo-target/release/youtube-upload-manager.exe",
      "package-executable",
    ),
    createArtifactReceiptAtPath(
      path.join(nsisDirectory, installers[0]),
      `cargo-target/release/bundle/nsis/${installers[0]}`,
      "package-installer",
    ),
  ];

  const status = gitValue(repositoryRoot, ["status", "--short", "--untracked-files=all"]);
  const manifest = {
    schemaVersion: 2,
    capturedAtUtc,
    localOnly: true,
    containsSensitiveData: false,
    evidenceBoundary: "unsigned-performance-harness-build",
    performanceHarness: true,
    cargoTargetDirectorySource: environment.CARGO_TARGET_DIR?.trim()
      ? "environment"
      : "repository-default",
    signed: false,
    liveProvider: false,
    source: {
      commit: gitValue(repositoryRoot, ["rev-parse", "HEAD"]),
      workingTreeClean: status.length === 0,
      workingTreeStatusSha256: sha256(Buffer.from(status, "utf8")),
    },
    artifacts: [...repositoryArtifacts, ...targetArtifacts],
  };
  const outputDirectory = path.join(repositoryRoot, "output", "performance");
  mkdirSync(outputDirectory, { recursive: true });
  const outputPath = path.join(outputDirectory, "windows-harness-build-manifest.json");
  writeFileSync(outputPath, `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
  return { manifest, outputPath };
}

export function createWindowsHarnessInvocation({
  extraArguments = [],
  environment = process.env,
  execPath = process.execPath,
  resolveTauriCli = () => require.resolve("@tauri-apps/cli/tauri.js"),
} = {}) {
  return {
    command: execPath,
    args: [
      resolveTauriCli(),
      "build",
      "--features",
      "performance-harness",
      "--bundles",
      "nsis",
      "--no-sign",
      ...extraArguments,
    ],
    options: {
      stdio: "inherit",
      env: {
        ...environment,
        TAURI_ENV_PERFORMANCE_HARNESS: "1",
      },
    },
  };
}

export function runWindowsHarnessBuild(extraArguments = process.argv.slice(2)) {
  if (process.platform !== "win32") {
    console.error("The packaged performance harness currently supports Windows only.");
    return 1;
  }
  const invocation = createWindowsHarnessInvocation({ extraArguments });
  const result = spawnSync(invocation.command, invocation.args, invocation.options);
  if (result.error) throw result.error;
  if (result.status === 0) {
    const { outputPath } = writeWindowsHarnessBuildManifest();
    console.log(`Windows harness build manifest recorded: ${outputPath}`);
  }
  return result.status ?? 1;
}

const entryPath = process.argv[1] ? path.resolve(process.argv[1]) : null;
if (entryPath && fileURLToPath(import.meta.url) === entryPath) {
  process.exit(runWindowsHarnessBuild());
}
