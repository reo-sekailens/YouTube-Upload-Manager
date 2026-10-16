import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { validateTauriVersions } from "./check-tauri-versions.mjs";

function fixture({ js = "2.12.1", rust = "2.12.2", jsPin = "~2.12.0", rustPin = "~2.12.0" } = {}) {
  return {
    packageJson: { dependencies: { "@tauri-apps/api": jsPin, "@tauri-apps/plugin-dialog": "^2.7.0" }, devDependencies: { "@tauri-apps/cli": "^2.10.0" } },
    packageLock: { lockfileVersion: 3, packages: {
      "": { dependencies: { "@tauri-apps/api": jsPin } },
      "node_modules/@tauri-apps/api": { version: js },
      "node_modules/@tauri-apps/cli": { version: "2.10.0" },
      "node_modules/@tauri-apps/plugin-dialog": { version: "2.7.0" },
    } },
    cargoToml: `[package]\nname = "example"\n[dependencies]\ntauri = { version = "${rustPin}", features = ["tray-icon"] }\ntauri-plugin-dialog = "2.7"\n`,
    cargoLock: `version = 4\n[[package]]\nname = "tauri"\nversion = "${rust}"\nsource = "registry+https://example.test/index#hash"\n[[package]]\nname = "tauri-build"\nversion = "2.5.0"\n`,
  };
}

function fails(input, pattern) {
  const result = validateTauriVersions(input);
  assert.equal(result.ok, false);
  assert.match(result.errors.join("\n"), pattern);
  return result;
}

test("the reported JavaScript 2.11.1 / Rust 2.12.2 failure identifies both core packages", () => {
  fails(fixture({ js: "2.11.1", jsPin: "~2.11.0" }), /@tauri-apps\/api v2\.11\.1.*tauri v2\.12\.2.*major\/minor/);
});

test("same minor accepts different resolved patches and independently versioned CLI/plugins", () => {
  const result = validateTauriVersions(fixture());
  assert.equal(result.ok, true, result.errors.join("\n"));
  assert.equal(result.versions.npmResolved, "2.12.1");
  assert.equal(result.versions.rustResolved, "2.12.2");
});

test("exact npm and Cargo pins can differ in patch versions", () => {
  const result = validateTauriVersions(fixture({ jsPin: "2.12.1", rustPin: "=2.12.2" }));
  assert.equal(result.ok, true, result.errors.join("\n"));
});

test("tilde pins permit newer patches but never an older patch or different minor", () => {
  fails(fixture({ jsPin: "~2.12.2" }), /@tauri-apps\/api lock drift: declared ~2\.12\.2, resolved 2\.12\.1/);
  fails(fixture({ rust: "2.13.0" }), /tauri lock drift: declared ~2\.12\.0, resolved 2\.13\.0/);
});

test("exact lock drift is rejected independently for both ecosystems", () => {
  fails(fixture({ jsPin: "2.12.0" }), /@tauri-apps\/api lock drift/);
  fails(fixture({ rustPin: "=2.12.1" }), /tauri lock drift/);
});

test("major releases must also match", () => {
  fails(fixture({ js: "3.12.1", jsPin: "~3.12.0" }), /Tauri core release mismatch/);
});

test("package-lock root declaration must match package.json even when both ranges resolve", () => {
  const input = fixture();
  input.packageLock.packages[""].dependencies["@tauri-apps/api"] = "~2.12.1";
  fails(input, /root @tauri-apps\/api declaration drift.*~2\.12\.0.*~2\.12\.1/);
});

test("broad npm ranges and broad Rust caret requirements cannot silently drift minors", () => {
  for (const jsPin of ["^2.12.0", "2", "2.12", "*", ">=2.12.0", "~2.12.0 || ~2.13.0"]) {
    fails(fixture({ jsPin }), /package\.json @tauri-apps\/api must pin a minor release/);
  }
  for (const rustPin of ["2", "2.12.2", "^2.12.0", "*"]) {
    fails(fixture({ rustPin }), /Cargo\.toml tauri must pin a minor release/);
  }
});

test("missing declarations and resolved records fail with their source names", () => {
  const noPackage = fixture(); delete noPackage.packageJson.dependencies["@tauri-apps/api"];
  fails(noPackage, /package\.json must declare @tauri-apps\/api exactly once; found 0/);
  const noLockRoot = fixture(); delete noLockRoot.packageLock.packages[""];
  fails(noLockRoot, /package-lock\.json root package must declare/);
  const noNpmVersion = fixture(); delete noNpmVersion.packageLock.packages["node_modules/@tauri-apps/api"];
  fails(noNpmVersion, /package-lock\.json node_modules\/@tauri-apps\/api.*missing/);
  fails({ ...fixture(), cargoToml: "[dependencies]\nother = \"2\"\n" }, /Cargo\.toml.*tauri exactly once; found 0/);
  fails({ ...fixture(), cargoLock: '[[package]]\nname = "tauri-build"\nversion = "2.12.2"\n' }, /Cargo\.lock.*tauri package record; found 0/);
});

test("ambiguous API declarations, Rust declarations and locked packages fail closed", () => {
  const duplicate = fixture(); duplicate.packageJson.devDependencies["@tauri-apps/api"] = "~2.12.0";
  fails(duplicate, /package\.json.*exactly once; found 2/);
  const duplicateLockRoot = fixture(); duplicateLockRoot.packageLock.packages[""].devDependencies = { "@tauri-apps/api": "~2.12.0" };
  fails(duplicateLockRoot, /package-lock\.json root package.*exactly once; found 2/);
  const duplicateRust = fixture(); duplicateRust.cargoToml += 'tauri = { version = "~2.12.0" }\n';
  fails(duplicateRust, /Cargo\.toml.*exactly once; found 2/);
  const duplicateLock = fixture(); duplicateLock.cargoLock += '[[package]]\nname = "tauri"\nversion = "2.12.1"\n';
  fails(duplicateLock, /Cargo\.lock.*tauri package record; found 2/);
});

test("malformed and prerelease versions do not masquerade as compatible releases", () => {
  for (const js of [undefined, null, 2.12, "2.12", "v2.12.1", "2.12.1-beta.1", "02.12.1", "2.12.NaN", "2.12.9007199254740992"]) {
    const input = fixture(); input.packageLock.packages["node_modules/@tauri-apps/api"].version = js;
    fails(input, /package-lock\.json node_modules\/@tauri-apps\/api/);
  }
  fails(fixture({ rust: "2.12.2-beta.1" }), /Cargo\.lock tauri must contain one stable/);
  fails(fixture({ jsPin: "2.12.1-beta.1" }), /package\.json @tauri-apps\/api must pin/);
});

test("malformed Cargo inline tables, duplicate version fields and lock fields fail", () => {
  for (const table of ['"~2.12.0"', '{ version = "~2.12.0", version = "~2.12.1" }', '{ version = 2.12.0 }', '{ version = "~2.12.0, features = [] }', '{ features = ["tray-icon"] }']) {
    fails({ ...fixture(), cargoToml: `[dependencies]\ntauri = ${table}\n` }, /Cargo\.toml tauri/);
  }
  fails({ ...fixture(), cargoLock: '[[package]]\nname = "tauri"\nversion = "2.12.2"\nversion = "2.12.1"\n' }, /Cargo\.lock tauri record.*exactly one version field; found 2/);
  fails({ ...fixture(), cargoLock: '[[package]]\nname = "tauri"\nversion = 2.12.2\n' }, /Cargo\.lock tauri record version must be a plain quoted string/);
});

test("comments, literal strings and unrelated tauri names do not confuse record parsing", () => {
  const input = fixture();
  input.cargoToml = '# tauri = { version = "~9.0.0" }\n[dependencies]\ntauri = { features = ["version,feature", "hash#feature"], version = \'~2.12.0\' } # version = "9.0.0"\n[dev-dependencies]\ntauri = { version = "=9.0.0" }\n';
  input.cargoLock = '# [[package]]\n# name = "tauri"\n[[package]]\nname = \'tauri\'\nversion = \'2.12.2\' # trailing comment\nsource = "url#fragment"\n';
  const result = validateTauriVersions(input);
  assert.equal(result.ok, true, result.errors.join("\n"));
});

test("absent inputs return actionable errors instead of throwing", () => {
  fails(undefined, /package\.json must declare/);
  fails(null, /package\.json must declare/);
});

test("CLI exits successfully for alignment and nonzero for the reported mismatch or malformed JSON", () => {
  const directory = mkdtempSync(join(tmpdir(), "tauri-version-guard-"));
  const script = fileURLToPath(new URL("./check-tauri-versions.mjs", import.meta.url));
  mkdirSync(join(directory, "src-tauri"));
  function write(input) {
    writeFileSync(join(directory, "package.json"), JSON.stringify(input.packageJson));
    writeFileSync(join(directory, "package-lock.json"), JSON.stringify(input.packageLock));
    writeFileSync(join(directory, "src-tauri/Cargo.toml"), input.cargoToml);
    writeFileSync(join(directory, "src-tauri/Cargo.lock"), input.cargoLock);
  }
  function run() { return spawnSync(process.execPath, [script], { cwd: directory, encoding: "utf8", windowsHide: true, timeout: 10_000 }); }
  try {
    write(fixture());
    const success = run(); assert.equal(success.status, 0, success.stderr); assert.match(success.stdout, /versions align/);
    write(fixture({ js: "2.11.1", jsPin: "~2.11.0" }));
    const mismatch = run(); assert.equal(mismatch.status, 1); assert.match(mismatch.stderr, /@tauri-apps\/api v2\.11\.1.*tauri v2\.12\.2/);
    writeFileSync(join(directory, "package.json"), "{ broken JSON");
    const malformed = run(); assert.equal(malformed.status, 1); assert.match(malformed.stderr, /Tauri version check failed: package\.json:/);
  } finally {
    // The path is returned directly by mkdtempSync within the OS temp directory.
    assert.ok(directory.startsWith(join(tmpdir(), "tauri-version-guard-")));
    rmSync(directory, { recursive: true, force: true });
  }
});
