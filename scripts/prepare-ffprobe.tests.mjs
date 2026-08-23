import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { access, mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import {
  RECEIPT_FILENAME,
  prepareFfprobe,
  runFfprobePreparation,
  selectFfprobeTargets,
  streamFileSha256,
} from "./prepare-ffprobe.mjs";

const temporaryDirectories = [];

test.afterEach(async () => {
  await Promise.all(temporaryDirectories.splice(0).map((path) => rm(path, { recursive: true, force: true })));
});

function digest(value) {
  return createHash("sha256").update(value).digest("hex");
}

function fixtureTargets(binary, license) {
  return {
    "x86_64-pc-windows-msvc": {
      binary: "fixture-win-ffprobe",
      license: "fixture-win.LICENSE",
      digest: digest(binary),
      licenseDigest: digest(license),
    },
    "x86_64-unknown-linux-gnu": {
      binary: "fixture-linux-ffprobe",
      license: "fixture-linux.LICENSE",
      digest: digest(Buffer.from("linux-binary")),
      licenseDigest: digest(Buffer.from("linux-license")),
    },
    "aarch64-apple-darwin": {
      binary: "fixture-darwin-arm64-ffprobe",
      license: "fixture-darwin-arm64.LICENSE",
      digest: digest(Buffer.from("darwin-arm64-binary")),
      licenseDigest: digest(Buffer.from("darwin-arm64-license")),
    },
    "x86_64-apple-darwin": {
      binary: "fixture-darwin-x64-ffprobe",
      license: "fixture-darwin-x64.LICENSE",
      digest: digest(Buffer.from("darwin-x64-binary")),
      licenseDigest: digest(Buffer.from("darwin-x64-license")),
    },
  };
}

function streamingFetch(responses, requests) {
  return async (url) => {
    requests.push(url);
    const content = responses.get(url.split("/").at(-1));
    assert.ok(content, `unexpected request: ${url}`);
    return {
      ok: true,
      status: 200,
      body: new ReadableStream({
        start(controller) {
          controller.enqueue(content.subarray(0, 2));
          controller.enqueue(content.subarray(2));
          controller.close();
        },
      }),
      arrayBuffer() {
        throw new Error("the preparation path must not buffer an entire response");
      },
    };
  };
}

async function fixtureDirectory() {
  const path = await mkdtemp(join(tmpdir(), "ffprobe-preparation-test-"));
  temporaryDirectories.push(path);
  return path;
}

test("selects one host architecture and skips mobile targets without falling back", () => {
  assert.deepEqual(
    selectFfprobeTargets({ platform: "darwin", architecture: "arm64" }),
    { targets: ["aarch64-apple-darwin"], reason: "host" },
  );
  assert.deepEqual(
    selectFfprobeTargets({ platform: "darwin", architecture: "x64" }),
    { targets: ["x86_64-apple-darwin"], reason: "host" },
  );
  assert.deepEqual(
    selectFfprobeTargets({
      envTarget: "aarch64-linux-android",
      platform: "win32",
      architecture: "x64",
    }),
    { targets: [], reason: "mobile" },
  );
  assert.deepEqual(
    selectFfprobeTargets({ envTarget: "universal-apple-darwin" }),
    {
      targets: ["aarch64-apple-darwin", "x86_64-apple-darwin"],
      reason: "universal",
    },
  );
});

test("streams cold downloads and writes a receipt with all provenance inputs", async () => {
  const cwd = await fixtureDirectory();
  const binary = Buffer.from("small fixture ffprobe binary");
  const license = Buffer.from("small fixture license");
  const targets = fixtureTargets(binary, license);
  const requests = [];
  const releaseRoot = "https://fixture.invalid/release";

  const result = await prepareFfprobe({
    cwd,
    selectedTargets: ["x86_64-pc-windows-msvc"],
    release: "fixture-release",
    releaseRoot,
    preparationVersion: 41,
    targets,
    fetchImpl: streamingFetch(new Map([
      ["fixture-win-ffprobe", binary],
      ["fixture-win.LICENSE", license],
    ]), requests),
  });

  assert.equal(result.cacheHit, false);
  assert.deepEqual(requests, [
    `${releaseRoot}/fixture-win-ffprobe`,
    `${releaseRoot}/fixture-win.LICENSE`,
  ]);
  assert.deepEqual(
    await readFile(join(cwd, "src-tauri", "binaries", "ffprobe-x86_64-pc-windows-msvc.exe")),
    binary,
  );
  const receipt = JSON.parse(await readFile(
    join(cwd, "src-tauri", "binaries", RECEIPT_FILENAME),
    "utf8",
  ));
  assert.equal(receipt.provenance.preparationVersion, 41);
  assert.equal(receipt.provenance.release, "fixture-release");
  assert.equal(receipt.provenance.targets[0].target, "x86_64-pc-windows-msvc");
  assert.equal(receipt.provenance.targets[0].binarySha256, digest(binary));
  assert.equal(receipt.provenance.targets[0].licenseSha256, digest(license));
  assert.match(receipt.provenanceKey, /^[a-f0-9]{64}$/);
});

test("a valid receipt performs no network access and no full-file checksum", async () => {
  const cwd = await fixtureDirectory();
  const binary = Buffer.from("cache fixture ffprobe binary");
  const license = Buffer.from("cache fixture license");
  const targets = fixtureTargets(binary, license);
  const releaseRoot = "https://fixture.invalid/release";
  await prepareFfprobe({
    cwd,
    selectedTargets: ["x86_64-pc-windows-msvc"],
    releaseRoot,
    targets,
    fetchImpl: streamingFetch(new Map([
      ["fixture-win-ffprobe", binary],
      ["fixture-win.LICENSE", license],
    ]), []),
  });

  let hashCalls = 0;
  let fetchCalls = 0;
  const result = await prepareFfprobe({
    cwd,
    selectedTargets: ["x86_64-pc-windows-msvc"],
    releaseRoot,
    targets,
    fetchImpl: async () => {
      fetchCalls += 1;
      throw new Error("network access on a valid cache");
    },
    hashFileImpl: async () => {
      hashCalls += 1;
      throw new Error("full-file hash on a valid cache");
    },
  });

  assert.equal(result.cacheHit, true);
  assert.equal(fetchCalls, 0);
  assert.equal(hashCalls, 0);
});

test("changed provenance revalidates existing artifacts by streaming without network access", async () => {
  const cwd = await fixtureDirectory();
  const binary = Buffer.from("provenance fixture ffprobe binary");
  const license = Buffer.from("provenance fixture license");
  const targets = fixtureTargets(binary, license);
  const releaseRoot = "https://fixture.invalid/release";
  await prepareFfprobe({
    cwd,
    selectedTargets: ["x86_64-pc-windows-msvc"],
    releaseRoot,
    preparationVersion: 1,
    targets,
    fetchImpl: streamingFetch(new Map([
      ["fixture-win-ffprobe", binary],
      ["fixture-win.LICENSE", license],
    ]), []),
  });

  let hashCalls = 0;
  const result = await prepareFfprobe({
    cwd,
    selectedTargets: ["x86_64-pc-windows-msvc"],
    releaseRoot,
    preparationVersion: 2,
    targets,
    fetchImpl: async () => {
      throw new Error("valid existing artifacts should not be downloaded");
    },
    hashFileImpl: async (path) => {
      hashCalls += 1;
      return streamFileSha256(path);
    },
  });

  assert.equal(result.cacheHit, false);
  assert.equal(hashCalls, 2);
});

test("mobile preparation does no filesystem or network provisioning work", async () => {
  const cwd = await fixtureDirectory();
  let fetchCalls = 0;
  const result = await runFfprobePreparation({
    cwd,
    envTarget: "aarch64-apple-ios",
    platform: "win32",
    architecture: "x64",
    fetchImpl: async () => {
      fetchCalls += 1;
      throw new Error("mobile provisioning attempted a download");
    },
    log() {},
  });

  assert.equal(result.skipped, true);
  assert.equal(result.reason, "mobile");
  assert.equal(fetchCalls, 0);
  await assert.rejects(access(join(cwd, "src-tauri", "binaries")));
});

test("checksum failure never replaces a prior artifact or writes a receipt", async () => {
  const cwd = await fixtureDirectory();
  const binary = Buffer.from("expected binary");
  const license = Buffer.from("fixture license");
  const targets = fixtureTargets(binary, license);
  const outputDirectory = join(cwd, "src-tauri", "binaries");
  const destination = join(outputDirectory, "ffprobe-x86_64-pc-windows-msvc.exe");
  await mkdir(outputDirectory, { recursive: true });
  await writeFile(destination, "prior invalid cache entry");

  await assert.rejects(
    prepareFfprobe({
      cwd,
      selectedTargets: ["x86_64-pc-windows-msvc"],
      releaseRoot: "https://fixture.invalid/release",
      targets,
      fetchImpl: streamingFetch(new Map([
        ["fixture-win-ffprobe", Buffer.from("tampered download")],
      ]), []),
    }),
    /checksum mismatch/,
  );

  assert.equal(await readFile(destination, "utf8"), "prior invalid cache entry");
  await assert.rejects(access(join(outputDirectory, RECEIPT_FILENAME)));
});
