import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { DECODER_TARGETS } from "./prepare-perceptual-decoder.mjs";

const repository = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const workflow = await readFile(join(repository, ".github/workflows/nightly-prerelease.yml"), "utf8");
const step = workflow.match(/      - name: Package Windows portable ZIP\r?\n[\s\S]*?        run: \|\r?\n([\s\S]*?)(?=      - name: Package Linux portable AppImage)/)?.[1];
assert.ok(step, "the Windows portable package step must be present");
const script = step.split(/\r?\n/).map((line) => line.startsWith("          ") ? line.slice(10) : line).join("\n");
const run = promisify(execFile), temporaryDirectories = [];
const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
const windows = "x86_64-pc-windows-msvc";
const requiredEntries = ["YouTube-Upload-Manager.exe", "ffprobe.exe", "ffprobe-license.txt",
  "ffmpeg.exe", "ffmpeg-license.txt", "ffmpeg-source-notice.txt"];
test.afterEach(async () => {
  for (const directory of temporaryDirectories.splice(0)) {
    assert.ok(resolve(directory).startsWith(join(resolve(tmpdir()), "portable-decoder-package-test-")));
    await rm(directory, { recursive: true, force: true });
  }
});

function syntheticPe(machine = 0x8664, validSignature = true) {
  const bytes = Buffer.alloc(128);
  bytes[0] = 0x4d; bytes[1] = 0x5a; bytes.writeInt32LE(80, 0x3c);
  bytes.write(validSignature ? "PE\0\0" : "XX\0\0", 80, "ascii");
  bytes.writeUInt16LE(machine, 84);
  return bytes;
}

async function fixture({ decoder = syntheticPe(), missingDecoder = false,
  license = "fixture decoder GPL notice", sourceNotice = "fixture decoder source notice" } = {}) {
  const directory = await mkdtemp(join(tmpdir(), "portable-decoder-package-test-"));
  temporaryDirectories.push(directory);
  const binaries = join(directory, "src-tauri", "binaries");
  await mkdir(binaries, { recursive: true });
  await mkdir(join(directory, "src-tauri", "target", "release"), { recursive: true });
  await mkdir(join(directory, "scripts"));
  await writeFile(join(directory, "src-tauri", "target", "release", "youtube-upload-manager.exe"), "synthetic app fixture");
  await writeFile(join(binaries, "ffprobe-x86_64-pc-windows-msvc.exe"), syntheticPe());
  await writeFile(join(binaries, "ffprobe-license.txt"), "fixture FFprobe license");
  if (!missingDecoder) await writeFile(join(binaries, "ffmpeg-x86_64-pc-windows-msvc.exe"), decoder);
  await writeFile(join(binaries, "ffmpeg-license.txt"), license);
  await writeFile(join(binaries, "ffmpeg-source-notice.txt"), sourceNotice);
  // Synthetic PE bytes exercise the real workflow, without distributing a mock
  // as the real decoder. This fixture module substitutes only the artifact pin.
  const pins = { [windows]: { ...DECODER_TARGETS[windows], bytes: decoder.length, digest: sha(decoder) } };
  await writeFile(join(directory, "scripts", "prepare-perceptual-decoder.mjs"), `export const DECODER_TARGETS = ${JSON.stringify(pins)};\n`);
  return { directory, binaries, decoder, pins };
}

async function runWorkflow(f, { beforeReadback = "", beforeCopy = "" } = {}) {
  let source = script;
  if (beforeReadback) source = source.replace("$zip = [System.IO.Compression.ZipFile]::OpenRead", `${beforeReadback}\n$zip = [System.IO.Compression.ZipFile]::OpenRead`);
  if (beforeCopy) source = source.replace("New-Item -ItemType Directory -Force portable", `${beforeCopy}\nNew-Item -ItemType Directory -Force portable`);
  const inspect = `\n$proofZip = [System.IO.Compression.ZipFile]::OpenRead((Resolve-Path 'portable/YouTube-Upload-Manager-windows-x64-portable.zip'))\ntry { @($proofZip.Entries | ForEach-Object { @{ name = $_.FullName; bytes = $_.Length } }) | ConvertTo-Json -Compress } finally { $proofZip.Dispose() }\n`;
  const path = join(f.directory, "package.ps1");
  await writeFile(path, `$ErrorActionPreference = 'Stop'\n${source}${inspect}`);
  return run("pwsh", ["-NoProfile", "-NonInteractive", "-File", path], {
    cwd: f.directory, windowsHide: true, timeout: 30_000, maxBuffer: 1024 * 1024,
  });
}

function zipEdit(name, { remove = false, duplicate = false } = {}) {
  return `$editZip = [System.IO.Compression.ZipFile]::Open((Resolve-Path 'portable/YouTube-Upload-Manager-windows-x64-portable.zip'), [System.IO.Compression.ZipArchiveMode]::Update)\ntry {\n` +
    `${duplicate ? "" : `$editZip.GetEntry('${name}').Delete()\n`}` +
    `${remove ? "" : `$changedEntry = $editZip.CreateEntry('${name}')\n$changedStream = $changedEntry.Open()\ntry { $changedBytes = [System.Text.Encoding]::UTF8.GetBytes('tampered archive bytes'); $changedStream.Write($changedBytes, 0, $changedBytes.Length) } finally { $changedStream.Dispose() }\n`}` +
    "} finally { $editZip.Dispose() }";
}

async function rejectsWorkflow(f, pattern, options) {
  await assert.rejects(runWorkflow(f, options), (error) => {
    assert.match(`${error.stdout ?? ""}\n${error.stderr ?? ""}`, pattern);
    return true;
  });
}

test("Windows portable packaging preserves FFprobe and adds decoder plus both notices with archive readback", () => {
  for (const name of requiredEntries) assert.ok(script.includes(`'${name}'`) || script.includes(`/${name}`), name);
  assert.match(script, /DECODER_TARGETS\['x86_64-pc-windows-msvc'\]/);
  assert.match(script, /Get-FileHash -LiteralPath \$decoder -Algorithm SHA256/);
  assert.match(script, /Assert-WindowsX64Decoder \$archivedDecoderStream \$entry.Length/);
  assert.match(script, /ComputeHash\(\$entryStream\)/);
  assert.match(script, /\$archivedDigest -ne \$decoderPin.digest/);
  assert.match(script, /\$matchingEntries.Count -ne 1/);
  assert.match(script, /\$entry.Length -eq 0/);
  assert.match(script, /FFprobe has machine/);
  assert.equal(DECODER_TARGETS[windows].digest, "04e1307997530f9cf2fe35cba2ca7e8875ca91da02f89d6c7243df819c94ad00");
});

const windowsOnly = { skip: process.platform !== "win32" };
test("actual Windows workflow creates six nonempty entries and reads their exact source bytes back", windowsOnly, async () => {
  const f = await fixture(), result = await runWorkflow(f);
  const entries = JSON.parse(result.stdout.trim());
  assert.deepEqual(entries.map((entry) => entry.name).sort(), [...requiredEntries].sort());
  assert.ok(entries.every((entry) => entry.bytes > 0));
  assert.equal(entries.find((entry) => entry.name === "ffmpeg.exe").bytes, f.decoder.length);
});

test("a missing decoder and empty license/source notices fail before creating an archive", windowsOnly, async () => {
  for (const [options, message] of [[{ missingDecoder: true }, /Required decoder file is missing/],
    [{ license: "" }, /Required decoder file is empty/], [{ sourceNotice: "" }, /Required decoder file is empty/]]) {
    const f = await fixture(options); await rejectsWorkflow(f, message);
    await assert.rejects(readFile(join(f.directory, "portable", "YouTube-Upload-Manager-windows-x64-portable.zip")));
  }
});

test("a decoder with changed bytes fails the exact pin check", windowsOnly, async () => {
  const f = await fixture(), changed = Buffer.from(f.decoder); changed[127] = 1;
  await writeFile(join(f.binaries, "ffmpeg-x86_64-pc-windows-msvc.exe"), changed);
  await rejectsWorkflow(f, /SHA-256 does not match its pinned artifact/);
});

test("wrong architecture and invalid PE signatures fail even when the fixture pin matches their bytes", windowsOnly, async () => {
  await rejectsWorkflow(await fixture({ decoder: syntheticPe(0x014c) }), /expected Windows x64/);
  await rejectsWorkflow(await fixture({ decoder: syntheticPe(0x8664, false) }), /PE signature is invalid/);
});

test("a decoder changed after the initial check still fails the archived pinned-byte check", windowsOnly, async () => {
  const f = await fixture();
  const beforeCopy = "$changedSource = [System.IO.File]::ReadAllBytes((Resolve-Path -LiteralPath $decoder)); $changedSource[127] = 1; [System.IO.File]::WriteAllBytes((Resolve-Path -LiteralPath $decoder), $changedSource)";
  await rejectsWorkflow(f, /Archived decoder does not match the pinned Windows artifact/, { beforeCopy });
});

test("tampered archived decoder and source notice bytes fail archive readback", windowsOnly, async () => {
  for (const name of ["ffmpeg.exe", "ffmpeg-source-notice.txt"]) {
    await rejectsWorkflow(await fixture(), /Portable archive bytes differ from the prepared source/,
      { beforeReadback: zipEdit(name) });
  }
});

test("missing and duplicate archived notices fail instead of producing a misleading portable receipt", windowsOnly, async () => {
  for (const options of [{ remove: true }, { duplicate: true }]) {
    await rejectsWorkflow(await fixture(), /Portable archive contains an unexpected entry count/,
      { beforeReadback: zipEdit("ffmpeg-license.txt", options) });
  }
});
