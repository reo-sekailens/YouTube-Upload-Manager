import { createHash, randomUUID } from "node:crypto";
import { createReadStream, createWriteStream } from "node:fs";
import { chmod, lstat, mkdir, readFile, rename, rm, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { Readable, Transform } from "node:stream";
import { pipeline } from "node:stream/promises";
import { pathToFileURL } from "node:url";

// Pin artifact bytes, not the advertised release's FFmpeg version: upstream
// READMEs for this tag describe different platform builds. Preserve them below.
export const DECODER_RELEASE = "b6.1.1";
export const DECODER_RELEASE_ROOT = `https://github.com/eugeneware/ffmpeg-static/releases/download/${DECODER_RELEASE}`;
export const PREPARATION_VERSION = 1;
export const RECEIPT_FILENAME = ".perceptual-decoder-provenance.json";
export const DOWNLOAD_TIMEOUT_MS = 120_000;
export const MAX_BINARY_BYTES = 128 * 1024 * 1024;
export const MAX_NOTICE_BYTES = 1024 * 1024;
const GPL_TEXT = {
  name: "win32-x64.LICENSE",
  digest: "8ceb4b9ee5adedde47b31e975c1d90c73ad27b6b165a1dcd80c7c545eb65b903",
  bytes: 35147,
};

export const DECODER_TARGETS = {
  "x86_64-pc-windows-msvc": {
    binary: "ffmpeg-win32-x64", bytes: 82797568,
    digest: "04e1307997530f9cf2fe35cba2ca7e8875ca91da02f89d6c7243df819c94ad00",
    license: "win32-x64.LICENSE", licenseBytes: 35147,
    licenseDigest: GPL_TEXT.digest,
    readme: "win32-x64.README", readmeBytes: 39494,
    readmeDigest: "a636a7183c58006351acbaf35303c0ed85c6e1320fd4e80de453ba6157de6311",
    buildSource: "https://www.gyan.dev/ffmpeg/builds/",
  },
  "x86_64-unknown-linux-gnu": {
    binary: "ffmpeg-linux-x64", bytes: 79826272,
    digest: "e7e7fb30477f717e6f55f9180a70386c62677ef8a4d4d1a5d948f4098aa3eb99",
    license: "linux-x64.LICENSE", licenseBytes: 35147,
    licenseDigest: GPL_TEXT.digest,
    readme: "linux-x64.README", readmeBytes: 2235,
    readmeDigest: "72f4b1b06d419d22ace6e7cc75f06826f90737345aa0b1736158929f4aacc537",
    buildSource: "https://johnvansickle.com/ffmpeg/",
  },
  "aarch64-apple-darwin": {
    binary: "ffmpeg-darwin-arm64", bytes: 45568216,
    digest: "a90e3db6a3fd35f6074b013f948b1aa45b31c6375489d39e572bea3f18336584",
    license: "darwin-arm64.LICENSE", licenseBytes: 4376,
    licenseDigest: "cb48bf09a11f5fb576cddb0431c8f5ed0a60157a9ec942adffc13907cbe083f2",
    readme: "darwin-arm64.README", readmeBytes: 1810,
    readmeDigest: "05ba4b92c96605434b1aaae3eedf5a2c280c9607bf78ffca9a5b536d9af2dc6a",
    buildSource: "https://osxexperts.net/",
  },
  "x86_64-apple-darwin": {
    binary: "ffmpeg-darwin-x64", bytes: 78862176,
    digest: "ebdddc936f61e14049a2d4b549a412b8a40deeff6540e58a9f2a2da9e6b18894",
    license: "darwin-x64.LICENSE", licenseBytes: 4346,
    licenseDigest: "2e1d16c72fd74e12063776371da757322f8b77589386532f4fd8634bde7de1af",
    readme: "darwin-x64.README", readmeBytes: 6227,
    readmeDigest: "e88a0325f8e5b75210355e37341824f074d3cd82def2125be54c914b62848a36",
    buildSource: "https://evermeet.cx/ffmpeg/",
  },
};

export function selectDecoderTargets({ envTarget, args = [], platform = process.platform,
  architecture = process.arch, targets = DECODER_TARGETS } = {}) {
  const explicit = [envTarget, ...args.filter((value) => value !== "--universal")].filter(Boolean);
  if (explicit.some((value) => value.includes("android") || value.includes("apple-ios"))) {
    return { targets: [], reason: "mobile" };
  }
  const unknown = explicit.filter((value) => value !== "universal-apple-darwin" && !(value in targets));
  if (unknown.length) throw new Error(`Unsupported perceptual decoder target: ${unknown.join(", ")}`);
  if (envTarget === "universal-apple-darwin" || args.includes("--universal")) {
    if (explicit.some((value) => !value.endsWith("apple-darwin"))) {
      throw new Error("Universal decoder preparation requires macOS targets.");
    }
    return { targets: ["aarch64-apple-darwin", "x86_64-apple-darwin"], reason: "universal" };
  }
  const selected = [...new Set(explicit)];
  if (selected.length > 1) throw new Error("Select one decoder target, or --universal for macOS.");
  if (selected.length) return { targets: selected, reason: "explicit" };
  const host = {
    "win32-x64": "x86_64-pc-windows-msvc", "linux-x64": "x86_64-unknown-linux-gnu",
    "darwin-arm64": "aarch64-apple-darwin", "darwin-x64": "x86_64-apple-darwin",
  }[`${platform}-${architecture}`];
  return host && host in targets ? { targets: [host], reason: "host" }
    : { targets: [], reason: "unsupported-host" };
}

function assertArtifact(name, digest, bytes, maximum) {
  if (!/^[a-zA-Z0-9._-]+$/.test(name) || !/^[a-f0-9]{64}$/.test(digest) ||
      !Number.isSafeInteger(bytes) || bytes < 1 || bytes > maximum) {
    throw new Error("Decoder artifacts require a safe filename, SHA-256 and bounded exact byte length.");
  }
}

export async function streamDecoderSha256(path, maximum = MAX_BINARY_BYTES) {
  const identity = await lstat(path);
  if (!identity.isFile() || identity.isSymbolicLink() || identity.size > maximum) {
    throw new Error("The decoder cache must contain bounded regular files.");
  }
  const hash = createHash("sha256");
  let bytes = 0;
  for await (const chunk of createReadStream(path)) {
    bytes += chunk.length;
    if (bytes > maximum) throw new Error("Decoder artifact exceeded its byte limit.");
    hash.update(chunk);
  }
  return { digest: hash.digest("hex"), bytes };
}

async function replaceFile(temporary, destination) {
  try {
    await rename(temporary, destination);
  } catch (error) {
    // Windows cannot replace an existing destination with rename. It is removed
    // only after the complete new artifact has passed both byte and digest checks.
    if (process.platform !== "win32" || !["EEXIST", "EPERM"].includes(error.code)) throw error;
    await rm(destination, { force: true });
    await rename(temporary, destination);
  }
}

async function ensureArtifact({ path, name, digest, bytes, maximum, executable = false,
  releaseRoot, fetchImpl, hashFileImpl, downloadTimeoutMs }) {
  const identity = await lstat(path).catch((error) => {
    if (error.code === "ENOENT") return undefined;
    throw error;
  });
  if (identity && (!identity.isFile() || identity.isSymbolicLink())) {
    throw new Error("Decoder preparation refuses a non-regular cache destination.");
  }
  if (identity) {
    const cached = await hashFileImpl(path, maximum);
    if (cached.digest === digest && cached.bytes === bytes) {
      if (executable) await chmod(path, 0o755);
      return false;
    }
  }
  const temporary = `${path}.download-${process.pid}-${randomUUID()}`;
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), downloadTimeoutMs);
  try {
    const response = await fetchImpl(`${releaseRoot}/${name}`, { redirect: "follow", signal: controller.signal });
    if (!response.ok || !response.body) throw new Error(`Could not download pinned decoder asset ${name}: ${response.status}`);
    const length = response.headers?.get?.("content-length");
    if (length && (!/^\d+$/.test(length) || Number(length) !== bytes)) {
      throw new Error(`Decoder asset length mismatch for ${name}.`);
    }
    const hash = createHash("sha256");
    let received = 0;
    const verifier = new Transform({ transform(chunk, _encoding, callback) {
      received += chunk.length;
      if (received > bytes || received > maximum) {
        callback(new Error(`Decoder asset exceeded its byte limit for ${name}.`));
        return;
      }
      hash.update(chunk);
      callback(null, chunk);
    } });
    const source = typeof response.body.getReader === "function"
      ? Readable.fromWeb(response.body, { signal: controller.signal }) : response.body;
    await pipeline(source, verifier, createWriteStream(temporary, { flags: "wx", mode: 0o644 }), { signal: controller.signal });
    if (received !== bytes || hash.digest("hex") !== digest) throw new Error(`Decoder checksum or length mismatch for ${name}.`);
    if (executable) await chmod(temporary, 0o755);
    await replaceFile(temporary, path);
    return true;
  } catch (error) {
    if (controller.signal.aborted) throw new Error(`Decoder download timed out after ${downloadTimeoutMs} ms for ${name}.`, { cause: error });
    throw error;
  } finally {
    clearTimeout(timer);
    controller.abort();
    await rm(temporary, { force: true });
  }
}

async function writeVerifiedText(path, value) {
  const current = await lstat(path).catch((error) => {
    if (error.code === "ENOENT") return undefined;
    throw error;
  });
  if (current && (!current.isFile() || current.isSymbolicLink())) throw new Error("Decoder notice destination must be a regular file.");
  if (current && current.size === Buffer.byteLength(value) && await readFile(path, "utf8") === value) return;
  const temporary = `${path}.write-${process.pid}-${randomUUID()}`;
  try {
    await writeFile(temporary, value, { flag: "wx" });
    await replaceFile(temporary, path);
  } finally {
    await rm(temporary, { force: true });
  }
}

async function readPinnedText(path, expectedDigest, expectedBytes, maximum = MAX_NOTICE_BYTES) {
  const identity = await lstat(path);
  if (!identity.isFile() || identity.isSymbolicLink() || identity.size !== expectedBytes ||
      identity.size > maximum) throw new Error("Decoder notice must be a bounded regular file of its exact pinned length.");
  const chunks = [];
  let bytes = 0;
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(path)) {
    bytes += chunk.length;
    if (bytes > expectedBytes || bytes > maximum) throw new Error("Decoder notice exceeded its pinned length.");
    hash.update(chunk); chunks.push(chunk);
  }
  if (bytes !== expectedBytes || hash.digest("hex") !== expectedDigest) {
    throw new Error("Decoder notice checksum or length mismatch.");
  }
  return Buffer.concat(chunks, bytes).toString("utf8");
}

async function preparedNotices({ selected, noticeDirectory, release, releaseRoot, gplText }) {
  const sections = await Promise.all(selected.map(async (item) => ({ item,
    license: await readPinnedText(join(noticeDirectory, item.license), item.licenseDigest, item.licenseBytes),
    readme: await readPinnedText(join(noticeDirectory, item.readme), item.readmeDigest, item.readmeBytes),
  })));
  const licenseText = `Bundled FFmpeg perceptual decoder\nArtifact release: ${release}\n${releaseRoot}\n\n` +
    sections.map(({ item, license }) => `===== ${item.target}: ${item.license} =====\n${license}\n`).join("\n") +
    `\n===== Complete GNU GPL version 3 text supplied with upstream Windows/Linux artifacts =====\n${await readPinnedText(join(noticeDirectory, gplText.name), gplText.digest, gplText.bytes)}\n`;
  const sourceNotice = `FFmpeg decoder source and artifact provenance\nArtifact release tag: ${release}\n${releaseRoot}\n` +
    "FFmpeg project: https://ffmpeg.org/\nFFmpeg source: https://github.com/FFmpeg/FFmpeg\n" +
    "Artifact tag is not a guarantee of a common FFmpeg version across targets. The pinned target README below is retained verbatim.\n" +
    "Source and build configuration notices are supplied by each build distributor; a FFmpeg tag alone does not identify all bundled dependency sources.\n" +
    sections.map(({ item, readme }) => `\n===== ${item.target} =====\nBinary: ${releaseRoot}/${item.binary}\nSHA-256: ${item.digest}\nBytes: ${item.bytes}\nBuild distributor: ${item.buildSource}\nREADME: ${releaseRoot}/${item.readme}\nREADME SHA-256: ${item.readmeDigest}\n\n${readme}\n`).join("");
  const receipt = { preparationVersion: PREPARATION_VERSION, release, releaseRoot, targets: selected, gplText,
    notices: { licenseSha256: createHash("sha256").update(licenseText).digest("hex"),
      sourceNoticeSha256: createHash("sha256").update(sourceNotice).digest("hex") } };
  return { licenseText, sourceNotice, receipt };
}

async function acceptReadOnlyPreparedOutput({ selected, outputDirectory, noticeDirectory,
  release, releaseRoot, gplText, hashFileImpl }) {
  // A pre-existing output junction may expose an already-prepared build cache.
  // This branch performs bounded reads only: no mkdir, chmod, download, staging,
  // repair, receipt rewrite, or other write follows the redirected output path.
  try {
    const noticeIdentity = await lstat(noticeDirectory);
    if (!noticeIdentity.isDirectory() || noticeIdentity.isSymbolicLink()) {
      throw new Error("The prepared raw-notice folder must be an ordinary directory.");
    }
    for (const item of selected) {
      const binary = await hashFileImpl(join(outputDirectory, item.destination), MAX_BINARY_BYTES);
      if (binary.digest !== item.digest || binary.bytes !== item.bytes) {
        throw new Error("The prepared decoder binary does not match its exact pin.");
      }
    }
    const { licenseText, sourceNotice, receipt } = await preparedNotices({
      selected, noticeDirectory, release, releaseRoot, gplText });
    for (const [name, expected] of [["ffmpeg-license.txt", licenseText],
      ["ffmpeg-source-notice.txt", sourceNotice], [RECEIPT_FILENAME, `${JSON.stringify(receipt, null, 2)}\n`]]) {
      await readPinnedText(join(outputDirectory, name), createHash("sha256").update(expected).digest("hex"),
        Buffer.byteLength(expected), 4 * MAX_NOTICE_BYTES);
    }
    return { cacheHit: true, readOnly: true, outputDirectory,
      selectedTargets: selected.map((item) => item.target), receipt };
  } catch (error) {
    throw new Error("Redirected decoder output requires already-prepared exact pinned binaries, notices and provenance; no writes or downloads are permitted.", { cause: error });
  }
}

export async function preparePerceptualDecoder({ cwd = process.cwd(), selectedTargets,
  release = DECODER_RELEASE, releaseRoot = DECODER_RELEASE_ROOT, targets = DECODER_TARGETS,
  gplText = GPL_TEXT, fetchImpl = globalThis.fetch, hashFileImpl = streamDecoderSha256,
  downloadTimeoutMs = DOWNLOAD_TIMEOUT_MS } = {}) {
  if (!Array.isArray(selectedTargets) || selectedTargets.length < 1 || selectedTargets.length > 2 ||
      new Set(selectedTargets).size !== selectedTargets.length) throw new Error("Select one desktop decoder target, or two macOS universal targets.");
  if (!Number.isSafeInteger(downloadTimeoutMs) || downloadTimeoutMs < 1 || downloadTimeoutMs > DOWNLOAD_TIMEOUT_MS) {
    throw new Error("Decoder download timeout must be between 1 and 120000 ms.");
  }
  if (new URL(releaseRoot).protocol !== "https:") throw new Error("Decoder release URLs require HTTPS.");
  assertArtifact(gplText.name, gplText.digest, gplText.bytes, MAX_NOTICE_BYTES);
  const selected = selectedTargets.map((target) => {
    if (typeof target !== "string" || !/^[a-z0-9_]+(?:-[a-z0-9_]+)+$/.test(target)) {
      throw new Error("Decoder target names must be safe desktop target triples.");
    }
    const value = targets[target];
    if (!value) throw new Error(`Unsupported perceptual decoder target: ${target}`);
    assertArtifact(value.binary, value.digest, value.bytes, MAX_BINARY_BYTES);
    assertArtifact(value.license, value.licenseDigest, value.licenseBytes, MAX_NOTICE_BYTES);
    assertArtifact(value.readme, value.readmeDigest, value.readmeBytes, MAX_NOTICE_BYTES);
    return { target, ...value, destination: `ffmpeg-${target}${target.includes("windows") ? ".exe" : ""}` };
  });
  if (selected.length === 2 && !selected.every((value) => value.target.endsWith("apple-darwin"))) {
    throw new Error("Two-target decoder preparation is reserved for macOS universal builds.");
  }
  const outputDirectory = join(cwd, "src-tauri", "binaries");
  const noticeDirectory = join(outputDirectory, ".perceptual-decoder-notices");
  for (const directory of [outputDirectory, noticeDirectory]) {
    const identity = await lstat(directory).catch((error) => {
      if (error.code === "ENOENT") return undefined;
      throw error;
    });
    if (directory === outputDirectory && identity?.isSymbolicLink()) {
      return acceptReadOnlyPreparedOutput({ selected, outputDirectory, noticeDirectory,
        release, releaseRoot, gplText, hashFileImpl });
    }
    if (identity && (!identity.isDirectory() || identity.isSymbolicLink())) {
      throw new Error("Decoder preparation refuses redirected or non-directory output folders.");
    }
  }
  await mkdir(noticeDirectory, { recursive: true });
  // Hash every reused artifact, including notices, before bundling. A receipt is
  // evidence only and never overrides a changed or unverified cache entry.
  let downloaded = false;
  const common = { releaseRoot, fetchImpl, hashFileImpl, downloadTimeoutMs };
  for (const item of selected) {
    downloaded = await ensureArtifact({ ...common, path: join(outputDirectory, item.destination),
      name: item.binary, digest: item.digest, bytes: item.bytes, maximum: MAX_BINARY_BYTES,
      executable: !item.target.includes("windows") }) || downloaded;
    for (const kind of ["license", "readme"]) {
      downloaded = await ensureArtifact({ ...common, path: join(noticeDirectory, item[kind]),
        name: item[kind], digest: item[`${kind}Digest`], bytes: item[`${kind}Bytes`], maximum: MAX_NOTICE_BYTES }) || downloaded;
    }
  }
  downloaded = await ensureArtifact({ ...common, path: join(noticeDirectory, gplText.name),
    name: gplText.name, digest: gplText.digest, bytes: gplText.bytes, maximum: MAX_NOTICE_BYTES }) || downloaded;
  const { licenseText, sourceNotice, receipt } = await preparedNotices({
    selected, noticeDirectory, release, releaseRoot, gplText });
  await writeVerifiedText(join(outputDirectory, "ffmpeg-license.txt"), licenseText);
  await writeVerifiedText(join(outputDirectory, "ffmpeg-source-notice.txt"), sourceNotice);
  await writeVerifiedText(join(outputDirectory, RECEIPT_FILENAME), `${JSON.stringify(receipt, null, 2)}\n`);
  return { cacheHit: !downloaded, outputDirectory, selectedTargets: [...selectedTargets], receipt };
}

export async function runDecoderPreparation({ cwd = process.cwd(), envTarget = process.env.TAURI_ENV_TARGET_TRIPLE,
  args = process.argv.slice(2), platform = process.platform, architecture = process.arch,
  log = console.log, ...dependencies } = {}) {
  const selection = selectDecoderTargets({ envTarget, args, platform, architecture, targets: dependencies.targets });
  if (!selection.targets.length) {
    log(selection.reason === "mobile" ? "Skipping the desktop perceptual decoder for a mobile target."
      : "No supported desktop perceptual decoder is available for this host.");
    return { skipped: true, reason: selection.reason, selectedTargets: [] };
  }
  const result = await preparePerceptualDecoder({ cwd, selectedTargets: selection.targets, ...dependencies });
  log(`${result.cacheHit ? "Revalidated" : "Prepared"} pinned FFmpeg decoder artifacts for ${selection.targets.join(", ")}.`);
  return { ...result, skipped: false, reason: selection.reason };
}

const invokedPath = process.argv[1] ? pathToFileURL(resolve(process.argv[1])).href : undefined;
if (invokedPath === import.meta.url) await runDecoderPreparation();
