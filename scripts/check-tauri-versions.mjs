import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const API = "@tauri-apps/api";
const dependencySections = ["dependencies", "devDependencies", "optionalDependencies"];
const stableVersion = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;

function version(value, label, errors) {
  if (typeof value !== "string" || !stableVersion.test(value)) {
    errors.push(`${label} must contain one stable major.minor.patch version; found ${JSON.stringify(value) ?? "missing"}.`);
    return null;
  }
  const [major, minor, patch] = value.split(".").map(Number);
  if (![major, minor, patch].every(Number.isSafeInteger)) {
    errors.push(`${label} has an invalid version: ${value}.`);
    return null;
  }
  return { value, major, minor, patch };
}

function pin(value, label, errors, rust = false) {
  const pattern = rust ? /^(~|=)(.+)$/ : /^(~|=)?(.+)$/;
  const match = typeof value === "string" ? value.match(pattern) : null;
  if (!match || !stableVersion.test(match[2])) {
    errors.push(`${label} must pin a minor release with ${rust ? '"~2.12.2" or "=2.12.2"' : '"~2.12.2" or "2.12.2"'}; found ${JSON.stringify(value) ?? "missing"}.`);
    return null;
  }
  const parsed = version(match[2], label, errors);
  return parsed && { ...parsed, range: match[1] === "~" ? "tilde" : "exact" };
}

function dependency(document, label, errors) {
  const found = dependencySections.flatMap((section) => {
    const entries = document?.[section];
    return entries && Object.hasOwn(entries, API) ? [{ section, value: entries[API] }] : [];
  });
  if (found.length !== 1) {
    errors.push(`${label} must declare ${API} exactly once; found ${found.length}${found.length ? ` (${found.map((entry) => entry.section).join(", ")})` : ""}.`);
    return undefined;
  }
  return found[0].value;
}

// These files need only TOML string fields, section headers and one inline
// dependency table. Strip comments while preserving hashes inside strings.
function withoutComments(source) {
  let result = "", quote = null, escaped = false, comment = false;
  for (const character of source) {
    if (comment) {
      if (character === "\n") { comment = false; result += character; }
      continue;
    }
    if (quote) {
      result += character;
      if (escaped) escaped = false;
      else if (quote === '"' && character === "\\") escaped = true;
      else if (character === quote) quote = null;
    } else if (character === "#") comment = true;
    else {
      result += character;
      if (character === '"' || character === "'") quote = character;
    }
  }
  return result;
}

function stringField(source, field, label, errors) {
  const fields = [...source.matchAll(new RegExp(`^\\s*${field}\\s*=\\s*(.*?)\\s*$`, "gm"))];
  if (fields.length !== 1) {
    errors.push(`${label} must contain exactly one ${field} field; found ${fields.length}.`);
    return undefined;
  }
  const match = fields[0][1].match(/^(?:"([^"\\]*)"|'([^']*)')$/);
  if (!match) {
    errors.push(`${label} ${field} must be a plain quoted string; found ${fields[0][1] || "empty"}.`);
    return undefined;
  }
  return match[1] ?? match[2];
}

function cargoDeclaration(source, errors) {
  if (typeof source !== "string") {
    errors.push("src-tauri/Cargo.toml is missing or is not text.");
    return undefined;
  }
  let section = "";
  const entries = [];
  for (const line of withoutComments(source).split(/\r?\n/)) {
    const header = line.match(/^\s*\[([^\]]+)\]\s*$/);
    if (header) { section = header[1].trim(); continue; }
    if (section === "dependencies") {
      const match = line.match(/^\s*tauri\s*=\s*(.*?)\s*$/);
      if (match) entries.push(match[1]);
    }
  }
  if (entries.length !== 1) {
    errors.push(`src-tauri/Cargo.toml [dependencies] must declare tauri exactly once; found ${entries.length}.`);
    return undefined;
  }
  const inline = entries[0].match(/^\{(.*)\}$/);
  if (!inline) {
    errors.push("src-tauri/Cargo.toml tauri must use an inline dependency table with one version field.");
    return undefined;
  }
  // Split fields only outside quoted strings and feature arrays. This prevents
  // a feature named version (or containing a comma) from becoming a false pin.
  const fields = [], text = inline[1];
  let start = 0, quote = null, depth = 0, escaped = false;
  for (let index = 0; index < text.length; index += 1) {
    const character = text[index];
    if (quote) {
      if (escaped) escaped = false;
      else if (quote === '"' && character === "\\") escaped = true;
      else if (character === quote) quote = null;
    } else if (character === '"' || character === "'") quote = character;
    else if (character === "[") depth += 1;
    else if (character === "]") depth -= 1;
    else if (character === "," && depth === 0) { fields.push(text.slice(start, index)); start = index + 1; }
    if (depth < 0) break;
  }
  if (quote || depth !== 0) {
    errors.push("src-tauri/Cargo.toml tauri inline table is malformed.");
    return undefined;
  }
  fields.push(text.slice(start));
  return stringField(fields.join("\n"), "version", "src-tauri/Cargo.toml tauri", errors);
}

function cargoResolved(source, errors) {
  if (typeof source !== "string") {
    errors.push("src-tauri/Cargo.lock is missing or is not text.");
    return undefined;
  }
  const records = withoutComments(source).split(/^\s*\[\[package\]\]\s*$/m).slice(1);
  const candidates = records.filter((record) => /^\s*name\s*=\s*["']tauri["']\s*$/m.test(record));
  if (candidates.length !== 1) {
    errors.push(`src-tauri/Cargo.lock must contain exactly one tauri package record; found ${candidates.length}.`);
    return undefined;
  }
  const name = stringField(candidates[0], "name", "src-tauri/Cargo.lock tauri record", errors);
  const resolved = stringField(candidates[0], "version", "src-tauri/Cargo.lock tauri record", errors);
  return name === "tauri" ? resolved : undefined;
}

function verifyResolution(declared, locked, label, errors) {
  if (!declared || !locked) return;
  const sameRelease = declared.major === locked.major && declared.minor === locked.minor;
  const allowedPatch = declared.range === "tilde" ? locked.patch >= declared.patch : locked.patch === declared.patch;
  if (!sameRelease || !allowedPatch) {
    errors.push(`${label} lock drift: declared ${declared.range === "tilde" ? "~" : "="}${declared.value}, resolved ${locked.value}. Refresh the lockfile after aligning the declaration.`);
  }
}

/** Validate core Tauri declarations and locks without I/O or third-party code.
 * CLI and plugin versions have independent release lines and are excluded.
 */
export function validateTauriVersions(input = {}) {
  const { packageJson, packageLock, cargoToml, cargoLock } = input ?? {};
  const errors = [];
  const npmDeclaration = dependency(packageJson, "package.json", errors);
  const npmLockDeclaration = dependency(packageLock?.packages?.[""], "package-lock.json root package", errors);
  if (npmDeclaration !== undefined && npmLockDeclaration !== undefined && npmDeclaration !== npmLockDeclaration) {
    errors.push(`package-lock.json root ${API} declaration drift: package.json has ${JSON.stringify(npmDeclaration)}, lockfile root has ${JSON.stringify(npmLockDeclaration)}.`);
  }
  const npmResolved = packageLock?.packages?.[`node_modules/${API}`]?.version;
  const rustDeclaration = cargoDeclaration(cargoToml, errors);
  const rustResolved = cargoResolved(cargoLock, errors);
  const npmPin = pin(npmDeclaration, `package.json ${API}`, errors);
  pin(npmLockDeclaration, `package-lock.json root ${API}`, errors);
  const rustPin = pin(rustDeclaration, "src-tauri/Cargo.toml tauri", errors, true);
  const npmVersion = version(npmResolved, `package-lock.json node_modules/${API}`, errors);
  const rustVersion = version(rustResolved, "src-tauri/Cargo.lock tauri", errors);
  verifyResolution(npmPin, npmVersion, API, errors);
  verifyResolution(rustPin, rustVersion, "tauri", errors);
  if (npmVersion && rustVersion && (npmVersion.major !== rustVersion.major || npmVersion.minor !== rustVersion.minor)) {
    errors.push(`Tauri core release mismatch: ${API} v${npmVersion.value} (package-lock.json) and tauri v${rustVersion.value} (src-tauri/Cargo.lock) must share major/minor releases.`);
  }
  return {
    ok: errors.length === 0,
    errors,
    versions: { npmDeclaration, npmLockDeclaration, npmResolved, rustDeclaration, rustResolved },
  };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const read = (path) => readFileSync(resolve(process.cwd(), path), "utf8");
    const readJson = (path) => {
      const source = read(path);
      try { return JSON.parse(source); }
      catch (error) { throw new Error(`${path}: ${error.message}`); }
    };
    const result = validateTauriVersions({
      packageJson: readJson("package.json"),
      packageLock: readJson("package-lock.json"),
      cargoToml: read("src-tauri/Cargo.toml"),
      cargoLock: read("src-tauri/Cargo.lock"),
    });
    if (!result.ok) {
      console.error(`Tauri version check failed:\n${result.errors.map((error) => `- ${error}`).join("\n")}`);
      process.exitCode = 1;
    } else {
      console.log(`Tauri core versions align: ${API} v${result.versions.npmResolved}, tauri v${result.versions.rustResolved}.`);
    }
  } catch (error) {
    console.error(`Tauri version check failed: ${error.message}`);
    process.exitCode = 1;
  }
}
