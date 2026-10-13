import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { access, lstat, mkdir, mkdtemp, readFile, readdir, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import test from "node:test";
import { DECODER_TARGETS, MAX_BINARY_BYTES, MAX_NOTICE_BYTES, RECEIPT_FILENAME,
  preparePerceptualDecoder, runDecoderPreparation, selectDecoderTargets } from "./prepare-perceptual-decoder.mjs";

const directories = [];
test.afterEach(async () => {
  for (const directory of directories.splice(0)) {
    assert.ok(resolve(directory).startsWith(join(resolve(tmpdir()), "perceptual-decoder-test-")));
    await rm(directory, { recursive: true, force: true });
  }
});
const sha = (value) => createHash("sha256").update(value).digest("hex");
const windows = "x86_64-pc-windows-msvc";
const arm = "aarch64-apple-darwin";
const intel = "x86_64-apple-darwin";
async function fixture() {
  const cwd = await mkdtemp(join(tmpdir(), "perceptual-decoder-test-"));
  directories.push(cwd);
  const bodies = new Map();
  const targets = {};
  for (const target of [windows, arm, intel]) {
    const binary = Buffer.from(`fixture binary for ${target}`);
    const license = Buffer.from(`license for ${target}`);
    const readme = Buffer.from(`source and build configuration for ${target}`);
    targets[target] = { binary: `${target}.binary`, digest: sha(binary), bytes: binary.length,
      license: `${target}.LICENSE`, licenseDigest: sha(license), licenseBytes: license.length,
      readme: `${target}.README`, readmeDigest: sha(readme), readmeBytes: readme.length,
      buildSource: "https://fixture.invalid/source" };
    bodies.set(targets[target].binary, binary);
    bodies.set(targets[target].license, license);
    bodies.set(targets[target].readme, readme);
  }
  const gpl = Buffer.from("complete GNU GPL fixture text");
  const gplText = { name: "complete.LICENSE", digest: sha(gpl), bytes: gpl.length };
  bodies.set(gplText.name, gpl);
  const requests = [];
  const fetchImpl = async (url) => {
    requests.push(url);
    const body = bodies.get(url.split("/").at(-1));
    assert.ok(body, `unexpected fixture request ${url}`);
    return { ok: true, status: 200, body: new ReadableStream({ start(controller) {
      controller.enqueue(body.subarray(0, 3)); controller.enqueue(body.subarray(3)); controller.close();
    } }), arrayBuffer() { throw new Error("downloads must remain streamed"); } };
  };
  const options = { cwd, release: "fixture", releaseRoot: "https://fixture.invalid/release",
    selectedTargets: [windows], targets, gplText, fetchImpl };
  return { cwd, bodies, targets, gplText, requests, options,
    output: join(cwd, "src-tauri", "binaries") };
}

async function redirectedOptions(f) {
  const cwd = join(f.cwd, "redirected-root");
  await mkdir(join(cwd, "src-tauri"), { recursive: true });
  await symlink(f.output, join(cwd, "src-tauri", "binaries"), process.platform === "win32" ? "junction" : "dir");
  return { ...f.options, cwd,
    fetchImpl() { throw new Error("a redirected output must never download"); } };
}

async function snapshotFiles(directory, prefix = "") {
  const entries = await readdir(directory, { withFileTypes: true });
  const snapshot = {};
  for (const entry of entries) {
    const path = join(directory, entry.name), name = `${prefix}${entry.name}`;
    if (entry.isDirectory()) Object.assign(snapshot, await snapshotFiles(path, `${name}/`));
    else {
      const identity = await lstat(path);
      snapshot[name] = { digest: sha(await readFile(path)), size: identity.size, mode: identity.mode,
        mtimeMs: identity.mtimeMs, ctimeMs: identity.ctimeMs };
    }
  }
  return snapshot;
}

test("production pins include bounded exact sizes, binary/license/README digests for each supported desktop", () => {
  assert.equal(Object.keys(DECODER_TARGETS).length, 4);
  for (const item of Object.values(DECODER_TARGETS)) {
    assert.match(item.digest, /^[a-f0-9]{64}$/);
    assert.match(item.licenseDigest, /^[a-f0-9]{64}$/);
    assert.match(item.readmeDigest, /^[a-f0-9]{64}$/);
    assert.ok(item.bytes > 0 && item.bytes < MAX_BINARY_BYTES);
    assert.ok(item.licenseBytes > 0 && item.licenseBytes < MAX_NOTICE_BYTES);
    assert.ok(item.readmeBytes > 0 && item.readmeBytes < MAX_NOTICE_BYTES);
  }
});

test("target selection respects an explicit architecture and macOS universal builds", () => {
  assert.deepEqual(selectDecoderTargets({ platform: "win32", architecture: "x64" }), { targets: [windows], reason: "host" });
  assert.deepEqual(selectDecoderTargets({ platform: "darwin", architecture: "arm64" }), { targets: [arm], reason: "host" });
  assert.deepEqual(selectDecoderTargets({ envTarget: "universal-apple-darwin" }), { targets: [arm, intel], reason: "universal" });
  assert.throws(() => selectDecoderTargets({ args: [windows, "--universal"] }), /requires macOS/);
  assert.throws(() => selectDecoderTargets({ args: [windows, "x86_64-unknown-linux-gnu"] }), /Select one/);
  assert.throws(() => selectDecoderTargets({ envTarget: "unknown-target" }), /Unsupported/);
});

test("mobile and unsupported hosts do not provision filesystem artifacts or download", async () => {
  const f = await fixture();
  for (const envTarget of ["aarch64-linux-android", "aarch64-apple-ios"]) {
    const result = await runDecoderPreparation({ ...f.options, envTarget, args: [], log() {} });
    assert.equal(result.skipped, true); assert.equal(result.reason, "mobile");
  }
  const unsupported = await runDecoderPreparation({ ...f.options, envTarget: undefined,
    args: [], platform: "linux", architecture: "riscv64", log() {} });
  assert.equal(unsupported.reason, "unsupported-host");
  assert.equal(f.requests.length, 0);
  await assert.rejects(access(f.output));
});

test("cold preparation streams verified artifacts, complete license and verbatim source notices", async () => {
  const f = await fixture();
  const result = await preparePerceptualDecoder(f.options);
  assert.equal(result.cacheHit, false);
  assert.equal(f.requests.length, 4);
  assert.deepEqual(await readFile(join(f.output, `ffmpeg-${windows}.exe`)), f.bodies.get(f.targets[windows].binary));
  assert.match(await readFile(join(f.output, "ffmpeg-license.txt"), "utf8"), /complete GNU GPL fixture text/);
  const source = await readFile(join(f.output, "ffmpeg-source-notice.txt"), "utf8");
  assert.match(source, /source and build configuration for x86_64-pc-windows-msvc/);
  assert.ok(source.includes(f.targets[windows].digest));
  assert.match(source, /not a guarantee of a common FFmpeg version/);
  const receipt = JSON.parse(await readFile(join(f.output, RECEIPT_FILENAME), "utf8"));
  assert.equal(receipt.targets[0].digest, f.targets[windows].digest);
  assert.equal(receipt.gplText.digest, f.gplText.digest);
  assert.match(receipt.notices.sourceNoticeSha256, /^[a-f0-9]{64}$/);
});

test("warm preparation rehashes every pinned asset and restores changed generated notices without a network call", async () => {
  const f = await fixture(); await preparePerceptualDecoder(f.options);
  await writeFile(join(f.output, "ffmpeg-source-notice.txt"), "tampered notice");
  const result = await preparePerceptualDecoder({ ...f.options,
    fetchImpl() { throw new Error("verified cache must not download"); } });
  assert.equal(result.cacheHit, true);
  assert.match(await readFile(join(f.output, "ffmpeg-source-notice.txt"), "utf8"), /source and build configuration/);
});

test("a changed binary is revalidated even when its receipt was retained", async () => {
  const f = await fixture(); await preparePerceptualDecoder(f.options);
  const destination = join(f.output, `ffmpeg-${windows}.exe`);
  await writeFile(destination, "tampered original cache entry");
  f.requests.splice(0);
  const result = await preparePerceptualDecoder(f.options);
  assert.equal(result.cacheHit, false);
  assert.equal(f.requests.length, 1);
  assert.deepEqual(await readFile(destination), f.bodies.get(f.targets[windows].binary));
});

test("universal preparation retains each architecture's independent README and license", async () => {
  const f = await fixture();
  await preparePerceptualDecoder({ ...f.options, selectedTargets: [arm, intel] });
  const license = await readFile(join(f.output, "ffmpeg-license.txt"), "utf8");
  const source = await readFile(join(f.output, "ffmpeg-source-notice.txt"), "utf8");
  for (const target of [arm, intel]) {
    assert.match(license, new RegExp(`license for ${target}`));
    assert.match(source, new RegExp(`source and build configuration for ${target}`));
    await access(join(f.output, `ffmpeg-${target}`));
  }
});

test("checksum mismatch preserves a prior binary and writes no receipt or staging debris", async () => {
  const f = await fixture();
  await mkdir(f.output, { recursive: true });
  const destination = join(f.output, `ffmpeg-${windows}.exe`);
  await writeFile(destination, "prior entry");
  f.bodies.set(f.targets[windows].binary, Buffer.alloc(f.targets[windows].bytes, 65));
  await assert.rejects(preparePerceptualDecoder(f.options), /checksum or length mismatch/);
  assert.equal(await readFile(destination, "utf8"), "prior entry");
  await assert.rejects(access(join(f.output, RECEIPT_FILENAME)));
  assert.equal((await readdir(f.output)).some((value) => value.includes(".download-")), false);
});

test("oversized and truncated streams are rejected before publishing an executable", async () => {
  for (const delta of [1, -1]) {
    const f = await fixture();
    f.bodies.set(f.targets[windows].binary, Buffer.alloc(f.targets[windows].bytes + delta));
    await assert.rejects(preparePerceptualDecoder(f.options), delta > 0 ? /exceeded its byte limit/ : /checksum or length mismatch/);
    await assert.rejects(access(join(f.output, `ffmpeg-${windows}.exe`)));
  }
});

test("untrusted declared response lengths are rejected without reading their bodies", async () => {
  const f = await fixture();
  await assert.rejects(preparePerceptualDecoder({ ...f.options, fetchImpl: async () => ({
    ok: true, status: 200, headers: { get() { return "999999999"; } },
    body: new ReadableStream({ pull() { throw new Error("body should not be read"); } }),
  }) }), /length mismatch/);
  await assert.rejects(access(join(f.output, `ffmpeg-${windows}.exe`)));
});

test("a timeout before headers aborts and leaves no executable or provenance receipt", async () => {
  const f = await fixture(); let aborted = false;
  await assert.rejects(preparePerceptualDecoder({ ...f.options, downloadTimeoutMs: 20,
    fetchImpl: (_url, { signal }) => new Promise((_resolve, reject) => {
      signal.addEventListener("abort", () => { aborted = true; reject(signal.reason); }, { once: true });
    }),
  }), /timed out after 20 ms/);
  assert.equal(aborted, true);
  await assert.rejects(access(join(f.output, RECEIPT_FILENAME)));
  assert.equal((await readdir(f.output)).some((value) => value.includes(".download-")), false);
});

test("a stalled body is aborted and does not replace prior content", async () => {
  const f = await fixture(); await mkdir(f.output, { recursive: true });
  const destination = join(f.output, `ffmpeg-${windows}.exe`); await writeFile(destination, "prior content");
  let canceled = false;
  await assert.rejects(preparePerceptualDecoder({ ...f.options, downloadTimeoutMs: 20,
    fetchImpl: async () => ({ ok: true, status: 200, body: new ReadableStream({
      start(controller) { controller.enqueue(Buffer.from("part")); }, cancel() { canceled = true; },
    }) }),
  }), /timed out after 20 ms/);
  assert.equal(canceled, true); assert.equal(await readFile(destination, "utf8"), "prior content");
});

test("invalid artifact paths, unpinned digests, unbounded sizes and insecure release URLs fail before provisioning", async () => {
  const f = await fixture();
  for (const patch of [{ binary: "../outside" }, { digest: "" }, { bytes: MAX_BINARY_BYTES + 1 }]) {
    await assert.rejects(preparePerceptualDecoder({ ...f.options,
      targets: { ...f.targets, [windows]: { ...f.targets[windows], ...patch } } }), /safe filename/);
  }
  await assert.rejects(preparePerceptualDecoder({ ...f.options, releaseRoot: "http://fixture.invalid/release" }), /require HTTPS/);
  await assert.rejects(preparePerceptualDecoder({ ...f.options, downloadTimeoutMs: 120_001 }), /between 1/);
  await assert.rejects(access(f.output));
});

test("a non-regular cache destination is refused without modifying unrelated content", async () => {
  const f = await fixture(); await mkdir(f.output, { recursive: true });
  const destination = join(f.output, `ffmpeg-${windows}.exe`); await mkdir(destination);
  await writeFile(join(destination, "keep.txt"), "keep");
  await assert.rejects(preparePerceptualDecoder(f.options), /non-regular cache/);
  assert.equal(await readFile(join(destination, "keep.txt"), "utf8"), "keep");
});

test("a redirected output directory cannot write artifacts into an unrelated folder", async () => {
  const f = await fixture();
  const external = join(f.cwd, "outside-binaries");
  await mkdir(join(f.cwd, "src-tauri")); await mkdir(external);
  await writeFile(join(external, "keep.txt"), "keep");
  await symlink(external, f.output, process.platform === "win32" ? "junction" : "dir");
  await assert.rejects(preparePerceptualDecoder(f.options), /already-prepared exact pinned/);
  assert.equal(await readFile(join(external, "keep.txt"), "utf8"), "keep");
  assert.deepEqual(await readdir(external), ["keep.txt"]);
  assert.equal(f.requests.length, 0);
});

test("a fully prepared junction is accepted read-only with no fetch, chmod, repair or provenance rewrite", async () => {
  const f = await fixture(); await preparePerceptualDecoder(f.options);
  const options = await redirectedOptions(f), before = await snapshotFiles(f.output);
  const result = await preparePerceptualDecoder(options);
  assert.equal(result.cacheHit, true); assert.equal(result.readOnly, true);
  assert.equal(result.receipt.targets[0].digest, f.targets[windows].digest);
  assert.deepEqual(await snapshotFiles(f.output), before);
});

test("changed binaries, raw notices, generated notices and provenance in a junction fail without repair", async () => {
  for (const name of [`ffmpeg-${windows}.exe`, `.perceptual-decoder-notices/${windows}.LICENSE`,
    "ffmpeg-license.txt", "ffmpeg-source-notice.txt", RECEIPT_FILENAME]) {
    const f = await fixture(); await preparePerceptualDecoder(f.options);
    const options = await redirectedOptions(f);
    await writeFile(join(f.output, name), "tampered cache entry");
    const before = await snapshotFiles(f.output);
    await assert.rejects(preparePerceptualDecoder(options), /already-prepared exact pinned/);
    assert.deepEqual(await snapshotFiles(f.output), before);
  }
});

test("a missing junction receipt is rejected without recreating it", async () => {
  const f = await fixture(); await preparePerceptualDecoder(f.options);
  const options = await redirectedOptions(f);
  await rm(join(f.output, RECEIPT_FILENAME));
  const before = await snapshotFiles(f.output);
  await assert.rejects(preparePerceptualDecoder(options), /already-prepared exact pinned/);
  assert.deepEqual(await snapshotFiles(f.output), before);
  await assert.rejects(access(join(f.output, RECEIPT_FILENAME)));
});

test("symlinked cache files cannot substitute an external artifact", { skip: process.platform === "win32" }, async () => {
  const f = await fixture(); await mkdir(f.output, { recursive: true });
  const external = join(f.cwd, "external.binary"); await writeFile(external, f.bodies.get(f.targets[windows].binary));
  await symlink(external, join(f.output, `ffmpeg-${windows}.exe`));
  await assert.rejects(preparePerceptualDecoder(f.options), /non-regular cache/);
  assert.deepEqual(await readFile(external), f.bodies.get(f.targets[windows].binary));
});
