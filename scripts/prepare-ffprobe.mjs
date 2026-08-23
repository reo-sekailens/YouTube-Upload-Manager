import { createHash, randomUUID } from "node:crypto";
import { createReadStream, createWriteStream } from "node:fs";
import {
  chmod,
  mkdir,
  readFile,
  rename,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import { join, resolve } from "node:path";
import { Readable, Transform } from "node:stream";
import { pipeline } from "node:stream/promises";
import { pathToFileURL } from "node:url";

export const FFMPEG_STATIC_RELEASE = "b6.1.1";
export const PREPARATION_VERSION = 2;
export const RECEIPT_FILENAME = ".ffprobe-provenance.json";

const LICENSE_DIGESTS = {
  "win32-x64.LICENSE": "8ceb4b9ee5adedde47b31e975c1d90c73ad27b6b165a1dcd80c7c545eb65b903",
  "linux-x64.LICENSE": "8ceb4b9ee5adedde47b31e975c1d90c73ad27b6b165a1dcd80c7c545eb65b903",
  "darwin-arm64.LICENSE": "cb48bf09a11f5fb576cddb0431c8f5ed0a60157a9ec942adffc13907cbe083f2",
  "darwin-x64.LICENSE": "2e1d16c72fd74e12063776371da757322f8b77589386532f4fd8634bde7de1af",
};

export const FFMPEG_STATIC_TARGETS = {
  "x86_64-pc-windows-msvc": {
    binary: "ffprobe-win32-x64",
    license: "win32-x64.LICENSE",
    digest: "3a7e2dc003dc2cd1472827e4c7c4f056ae1ae0ae7c5bbc580c99b49827351ba4",
    licenseDigest: LICENSE_DIGESTS["win32-x64.LICENSE"],
  },
  "x86_64-unknown-linux-gnu": {
    binary: "ffprobe-linux-x64",
    license: "linux-x64.LICENSE",
    digest: "4f231a1960d83e403d08f7971e271707bec278a9ae18e21b8b5b03186668450d",
    licenseDigest: LICENSE_DIGESTS["linux-x64.LICENSE"],
  },
  "aarch64-apple-darwin": {
    binary: "ffprobe-darwin-arm64",
    license: "darwin-arm64.LICENSE",
    digest: "bb2db6f5d8cef919da12fbf592119a987202a8c060a886f3cab091f9cab90b64",
    licenseDigest: LICENSE_DIGESTS["darwin-arm64.LICENSE"],
  },
  "x86_64-apple-darwin": {
    binary: "ffprobe-darwin-x64",
    license: "darwin-x64.LICENSE",
    digest: "fa3add0ce901f7241abe0dfc0155d958fc834aca3f8ce61f87cc712ae669c1e0",
    licenseDigest: LICENSE_DIGESTS["darwin-x64.LICENSE"],
  },
};

function isMobileTarget(target) {
  return target.includes("android") || target.includes("apple-ios");
}

function targetForHost(platform, architecture) {
  if (platform === "win32" && architecture === "x64") return "x86_64-pc-windows-msvc";
  if (platform === "linux" && architecture === "x64") return "x86_64-unknown-linux-gnu";
  if (platform === "darwin" && architecture === "arm64") return "aarch64-apple-darwin";
  if (platform === "darwin" && architecture === "x64") return "x86_64-apple-darwin";
  return undefined;
}

export function selectFfprobeTargets({
  envTarget,
  args = [],
  platform = process.platform,
  architecture = process.arch,
  targets = FFMPEG_STATIC_TARGETS,
} = {}) {
  const universal = envTarget === "universal-apple-darwin" || args.includes("--universal");
  const explicitTargets = [envTarget, ...args.filter((argument) => argument !== "--universal")]
    .filter(Boolean);

  if (explicitTargets.some(isMobileTarget)) {
    return { targets: [], reason: "mobile" };
  }

  const unknownTargets = explicitTargets.filter(
    (target) => target !== "universal-apple-darwin" && !(target in targets),
  );
  if (unknownTargets.length) {
    throw new Error(`Unsupported FFprobe target: ${unknownTargets.join(", ")}`);
  }

  if (universal) {
    const incompatibleTargets = explicitTargets.filter(
      (target) => target !== "universal-apple-darwin" && !target.endsWith("apple-darwin"),
    );
    if (incompatibleTargets.length) {
      throw new Error("Universal FFprobe preparation is only supported for macOS targets.");
    }
    return {
      targets: ["aarch64-apple-darwin", "x86_64-apple-darwin"],
      reason: "universal",
    };
  }

  const selectedTargets = [...new Set(explicitTargets)];
  if (selectedTargets.length > 1) {
    throw new Error("Select one FFprobe target, or pass --universal for an explicit macOS universal build.");
  }
  if (selectedTargets.length === 1) {
    return { targets: selectedTargets, reason: "explicit" };
  }

  const hostTarget = targetForHost(platform, architecture);
  return hostTarget && hostTarget in targets
    ? { targets: [hostTarget], reason: "host" }
    : { targets: [], reason: "unsupported-host" };
}

function sha256Text(value) {
  return createHash("sha256").update(value).digest("hex");
}

function destinationFilename(target) {
  const extension = target.includes("windows") ? ".exe" : "";
  return `ffprobe-${target}${extension}`;
}

function provenanceFor({
  selectedTargets,
  release,
  releaseRoot,
  preparationVersion,
  targets,
}) {
  return {
    preparationVersion,
    release,
    releaseRoot,
    targets: selectedTargets.map((target) => ({
      target,
      binary: targets[target].binary,
      binarySha256: targets[target].digest,
      destination: destinationFilename(target),
      license: targets[target].license,
      licenseSha256: targets[target].licenseDigest,
    })),
  };
}

function provenanceKey(provenance) {
  return sha256Text(JSON.stringify(provenance));
}

async function fileIdentity(path) {
  const value = await stat(path, { bigint: true });
  if (!value.isFile()) throw new Error(`${path} is not a regular file.`);
  return {
    device: value.dev.toString(),
    inode: value.ino.toString(),
    size: value.size.toString(),
    modifiedNanoseconds: value.mtimeNs.toString(),
    changedNanoseconds: value.ctimeNs.toString(),
  };
}

function sameIdentity(left, right) {
  return left && right &&
    left.device === right.device &&
    left.inode === right.inode &&
    left.size === right.size &&
    left.modifiedNanoseconds === right.modifiedNanoseconds &&
    left.changedNanoseconds === right.changedNanoseconds;
}

export async function streamFileSha256(path) {
  const hash = createHash("sha256");
  let byteLength = 0;
  for await (const chunk of createReadStream(path)) {
    hash.update(chunk);
    byteLength += chunk.length;
  }
  return { digest: hash.digest("hex"), byteLength };
}

async function streamToFile(readable, path) {
  const hash = createHash("sha256");
  let byteLength = 0;
  const verifier = new Transform({
    transform(chunk, _encoding, callback) {
      hash.update(chunk);
      byteLength += chunk.length;
      callback(null, chunk);
    },
  });
  await pipeline(readable, verifier, createWriteStream(path, { flags: "wx", mode: 0o644 }));
  return { digest: hash.digest("hex"), byteLength };
}

async function replaceFile(temporaryPath, destination) {
  await rm(destination, { force: true });
  await rename(temporaryPath, destination);
}

async function downloadVerified({ url, destination, expectedDigest, executable, fetchImpl }) {
  const temporaryPath = `${destination}.download-${process.pid}-${randomUUID()}`;
  try {
    const response = await fetchImpl(url, { redirect: "follow" });
    if (!response.ok || !response.body) {
      throw new Error(`Could not download ${url}: ${response.status}`);
    }
    const result = await streamToFile(response.body, temporaryPath);
    if (result.digest !== expectedDigest) {
      throw new Error(
        `FFprobe checksum mismatch for ${url}: expected ${expectedDigest}, got ${result.digest}`,
      );
    }
    if (executable) await chmod(temporaryPath, 0o755);
    await replaceFile(temporaryPath, destination);
    return result;
  } finally {
    await rm(temporaryPath, { force: true });
  }
}

async function ensureVerifiedFile({
  destination,
  expectedDigest,
  url,
  executable,
  fetchImpl,
  hashFileImpl,
}) {
  try {
    const result = await hashFileImpl(destination);
    if (result.digest === expectedDigest) {
      if (executable) await chmod(destination, 0o755);
      return;
    }
  } catch {
    // A missing or unreadable cache entry is replaced only after a verified download.
  }
  await downloadVerified({ url, destination, expectedDigest, executable, fetchImpl });
}

async function writeCompositeLicense({ sources, destination, releaseRoot, fetchImpl }) {
  const stagedSources = [];
  const outputTemporaryPath = `${destination}.download-${process.pid}-${randomUUID()}`;
  try {
    for (const source of sources) {
      const temporaryPath = `${destination}.${source.target}-${randomUUID()}`;
      stagedSources.push(temporaryPath);
      await downloadVerified({
        url: `${releaseRoot}/${source.license}`,
        destination: temporaryPath,
        expectedDigest: source.licenseSha256,
        executable: false,
        fetchImpl,
      });
    }

    async function* combinedLicenses() {
      for (let index = 0; index < sources.length; index += 1) {
        const source = sources[index];
        yield Buffer.from(
          `${index === 0 ? "" : "\n"}===== ${source.target}: ${source.license} =====\n`,
          "utf8",
        );
        yield* createReadStream(stagedSources[index]);
        yield Buffer.from("\n", "utf8");
      }
    }

    await streamToFile(Readable.from(combinedLicenses()), outputTemporaryPath);
    await replaceFile(outputTemporaryPath, destination);
  } finally {
    await rm(outputTemporaryPath, { force: true });
    await Promise.all(stagedSources.map((path) => rm(path, { force: true })));
  }
}

async function cachedReceipt({ receiptPath, expectedProvenance, outputDirectory }) {
  try {
    const receipt = JSON.parse(await readFile(receiptPath, "utf8"));
    const expectedKey = provenanceKey(expectedProvenance);
    if (receipt.provenanceKey !== expectedKey ||
        JSON.stringify(receipt.provenance) !== JSON.stringify(expectedProvenance)) {
      return undefined;
    }

    const expectedBinaries = expectedProvenance.targets;
    if (!Array.isArray(receipt.binaries) || receipt.binaries.length !== expectedBinaries.length) {
      return undefined;
    }
    for (const expected of expectedBinaries) {
      const recorded = receipt.binaries.find((binary) => binary.target === expected.target);
      if (!recorded || recorded.destination !== expected.destination) return undefined;
      const currentIdentity = await fileIdentity(join(outputDirectory, expected.destination));
      if (!sameIdentity(recorded.identity, currentIdentity)) return undefined;
    }

    if (receipt.license?.destination !== "ffprobe-license.txt") return undefined;
    const currentLicenseIdentity = await fileIdentity(join(outputDirectory, "ffprobe-license.txt"));
    return sameIdentity(receipt.license.identity, currentLicenseIdentity) ? receipt : undefined;
  } catch {
    return undefined;
  }
}

async function writeReceipt({ receiptPath, provenance, outputDirectory }) {
  const receipt = {
    provenanceKey: provenanceKey(provenance),
    provenance,
    binaries: await Promise.all(provenance.targets.map(async (target) => ({
      target: target.target,
      destination: target.destination,
      identity: await fileIdentity(join(outputDirectory, target.destination)),
    }))),
    license: {
      destination: "ffprobe-license.txt",
      identity: await fileIdentity(join(outputDirectory, "ffprobe-license.txt")),
    },
  };
  const temporaryPath = `${receiptPath}.write-${process.pid}-${randomUUID()}`;
  try {
    await writeFile(temporaryPath, `${JSON.stringify(receipt, null, 2)}\n`, { flag: "wx" });
    await replaceFile(temporaryPath, receiptPath);
  } finally {
    await rm(temporaryPath, { force: true });
  }
  return receipt;
}

export async function prepareFfprobe({
  cwd = process.cwd(),
  selectedTargets,
  release = FFMPEG_STATIC_RELEASE,
  releaseRoot = `https://github.com/eugeneware/ffmpeg-static/releases/download/${release}`,
  preparationVersion = PREPARATION_VERSION,
  targets = FFMPEG_STATIC_TARGETS,
  fetchImpl = globalThis.fetch,
  hashFileImpl = streamFileSha256,
} = {}) {
  if (!Array.isArray(selectedTargets) || selectedTargets.length === 0) {
    throw new Error("At least one selected desktop FFprobe target is required.");
  }
  for (const target of selectedTargets) {
    if (!(target in targets)) throw new Error(`Unsupported FFprobe target: ${target}`);
    if (!targets[target].licenseDigest) {
      throw new Error(`Missing pinned FFprobe license checksum for ${target}.`);
    }
  }

  const outputDirectory = join(cwd, "src-tauri", "binaries");
  const receiptPath = join(outputDirectory, RECEIPT_FILENAME);
  const provenance = provenanceFor({
    selectedTargets,
    release,
    releaseRoot,
    preparationVersion,
    targets,
  });
  const receipt = await cachedReceipt({ receiptPath, expectedProvenance: provenance, outputDirectory });
  if (receipt) {
    return { cacheHit: true, outputDirectory, receipt, selectedTargets: [...selectedTargets] };
  }

  await mkdir(outputDirectory, { recursive: true });
  for (const target of provenance.targets) {
    await ensureVerifiedFile({
      destination: join(outputDirectory, target.destination),
      expectedDigest: target.binarySha256,
      url: `${releaseRoot}/${target.binary}`,
      executable: !target.target.includes("windows"),
      fetchImpl,
      hashFileImpl,
    });
  }

  const licenseDestination = join(outputDirectory, "ffprobe-license.txt");
  const distinctLicenses = [...new Map(
    provenance.targets.map((target) => [target.license, target]),
  ).values()];
  if (distinctLicenses.length === 1) {
    const license = distinctLicenses[0];
    await ensureVerifiedFile({
      destination: licenseDestination,
      expectedDigest: license.licenseSha256,
      url: `${releaseRoot}/${license.license}`,
      executable: false,
      fetchImpl,
      hashFileImpl,
    });
  } else {
    await writeCompositeLicense({
      sources: distinctLicenses,
      destination: licenseDestination,
      releaseRoot,
      fetchImpl,
    });
  }

  const newReceipt = await writeReceipt({ receiptPath, provenance, outputDirectory });
  return {
    cacheHit: false,
    outputDirectory,
    receipt: newReceipt,
    selectedTargets: [...selectedTargets],
  };
}

export async function runFfprobePreparation({
  cwd = process.cwd(),
  envTarget = process.env.TAURI_ENV_TARGET_TRIPLE,
  args = process.argv.slice(2),
  platform = process.platform,
  architecture = process.arch,
  targets = FFMPEG_STATIC_TARGETS,
  fetchImpl = globalThis.fetch,
  hashFileImpl = streamFileSha256,
  log = console.log,
} = {}) {
  const selection = selectFfprobeTargets({ envTarget, args, platform, architecture, targets });
  if (selection.targets.length === 0) {
    log(selection.reason === "mobile"
      ? "Skipping desktop FFprobe preparation for a mobile target."
      : "No supported desktop FFprobe sidecar is available for this host.");
    return { skipped: true, reason: selection.reason, selectedTargets: [] };
  }

  const result = await prepareFfprobe({
    cwd,
    selectedTargets: selection.targets,
    targets,
    fetchImpl,
    hashFileImpl,
  });
  log(result.cacheHit
    ? `Reused verified bundled FFprobe ${FFMPEG_STATIC_RELEASE} for ${selection.targets.join(", ")}.`
    : `Prepared bundled FFprobe ${FFMPEG_STATIC_RELEASE} for ${selection.targets.join(", ")}.`);
  return { ...result, skipped: false, reason: selection.reason };
}

const invokedPath = process.argv[1] ? pathToFileURL(resolve(process.argv[1])).href : undefined;
if (invokedPath === import.meta.url) {
  await runFfprobePreparation();
}
