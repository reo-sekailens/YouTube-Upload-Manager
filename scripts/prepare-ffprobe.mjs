import { createHash } from 'node:crypto';
import { chmod, mkdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

const release = 'b6.1.1';
const releaseRoot = `https://github.com/eugeneware/ffmpeg-static/releases/download/${release}`;
const outputDirectory = join(process.cwd(), 'src-tauri', 'binaries');
const targets = {
  'x86_64-pc-windows-msvc': {
    binary: 'ffprobe-win32-x64',
    license: 'win32-x64.LICENSE',
    digest: '3a7e2dc003dc2cd1472827e4c7c4f056ae1ae0ae7c5bbc580c99b49827351ba4',
  },
  'x86_64-unknown-linux-gnu': {
    binary: 'ffprobe-linux-x64',
    license: 'linux-x64.LICENSE',
    digest: '4f231a1960d83e403d08f7971e271707bec278a9ae18e21b8b5b03186668450d',
  },
  'aarch64-apple-darwin': {
    binary: 'ffprobe-darwin-arm64',
    license: 'darwin-arm64.LICENSE',
    digest: 'bb2db6f5d8cef919da12fbf592119a987202a8c060a886f3cab091f9cab90b64',
  },
  'x86_64-apple-darwin': {
    binary: 'ffprobe-darwin-x64',
    license: 'darwin-x64.LICENSE',
    digest: 'fa3add0ce901f7241abe0dfc0155d958fc834aca3f8ce61f87cc712ae669c1e0',
  },
};

function targetsForHost() {
  if (process.platform === 'win32') return ['x86_64-pc-windows-msvc'];
  if (process.platform === 'linux') return ['x86_64-unknown-linux-gnu'];
  if (process.platform === 'darwin') return ['aarch64-apple-darwin', 'x86_64-apple-darwin'];
  return [];
}

function sha256(data) {
  return createHash('sha256').update(data).digest('hex');
}

async function download(url) {
  const response = await fetch(url, { redirect: 'follow' });
  if (!response.ok) throw new Error(`Could not download ${url}: ${response.status}`);
  return Buffer.from(await response.arrayBuffer());
}

async function provision(target) {
  const source = targets[target];
  const extension = target.includes('windows') ? '.exe' : '';
  const destination = join(outputDirectory, `ffprobe-${target}${extension}`);
  try {
    const existing = await readFile(destination);
    if (sha256(existing) === source.digest) return;
  } catch {}

  const binary = await download(`${releaseRoot}/${source.binary}`);
  const actualDigest = sha256(binary);
  if (actualDigest !== source.digest) {
    throw new Error(`FFprobe checksum mismatch for ${target}: expected ${source.digest}, got ${actualDigest}`);
  }
  await writeFile(destination, binary, { mode: 0o755 });
  if (!target.includes('windows')) await chmod(destination, 0o755);
}

async function provisionLicense(source) {
  const destination = join(outputDirectory, 'ffprobe-license.txt');
  const license = await download(`${releaseRoot}/${source.license}`);
  await writeFile(destination, license);
}

await mkdir(outputDirectory, { recursive: true });
const requestedTargets = [process.env.TAURI_ENV_TARGET_TRIPLE, ...process.argv.slice(2)]
  .filter((target) => target in targets);
const selectedTargets = requestedTargets.length ? requestedTargets : targetsForHost();
if (!selectedTargets.length) {
  console.log('No desktop FFprobe sidecar is needed for this host.');
  process.exit(0);
}

await Promise.all(selectedTargets.map(provision));
await provisionLicense(targets[selectedTargets[0]]);
console.log(`Prepared bundled FFprobe ${release} for ${selectedTargets.join(', ')}.`);
