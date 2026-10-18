import { execFileSync } from "node:child_process";
import { createWriteStream } from "node:fs";
import { cp, mkdir, mkdtemp, readFile, readdir, rename, rm, stat } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";

const EXTENSION_ID = "afmcagbpgggkpdkokjhjkllpegnadmkignlonpjm";
const INSTALL_METADATA =
  `https://install.overwolf.com/install/clean?partnerId=0&channel=web_dl_btn&extensionId=${EXTENSION_ID}`;
const REQUIRED_FILES = [
  "web/relicOverlay.html",
  "web/relicRecommendation.html",
];

export async function resolveAlecaWebRoot(repositoryRoot) {
  const candidates = await researchCandidates(repositoryRoot);
  const selected = newest(candidates);
  if (!selected) {
    throw new Error(
      "AlecaFrame package missing; run ./scripts/setup-ui-reference alecaframe",
    );
  }
  return path.join(selected.path, "web");
}

export async function prepareAlecaSource(repositoryRoot, explicitSource) {
  const candidates = explicitSource
    ? await inspectSource(path.resolve(explicitSource))
    : await discoverCandidates(repositoryRoot);
  let selected = newest(candidates);
  if (!selected) {
    throw new Error(
      "no compatible AlecaFrame package found; pass an OPK or extracted extension directory",
    );
  }

  let downloadDirectory;
  if (selected.kind === "remote") {
    downloadDirectory = await mkdtemp(path.join(os.tmpdir(), "wfcli-aleca-download-"));
    const downloaded = path.join(downloadDirectory, "app.opk");
    await download(selected.url, downloaded);
    const inspected = inspectOpk(downloaded);
    if (!inspected || inspected.version !== selected.version) {
      await rm(downloadDirectory, { recursive: true, force: true });
      throw new Error("downloaded AlecaFrame OPK failed validation");
    }
    selected = inspected;
  }

  try {
    return await installCandidate(repositoryRoot, selected);
  } finally {
    if (downloadDirectory) {
      await rm(downloadDirectory, { recursive: true, force: true });
    }
  }
}

async function installCandidate(repositoryRoot, selected) {
  const versionRoot = path.join(repositoryRoot, "research/alecaframe", selected.version);
  const destination = path.join(versionRoot, "package");
  if (selected.kind === "directory" && path.resolve(selected.path) === path.resolve(destination)) {
    return { version: selected.version, path: destination };
  }

  await mkdir(versionRoot, { recursive: true });
  const temporary = await mkdtemp(path.join(versionRoot, ".package-"));
  try {
    if (selected.kind === "opk") {
      execFileSync("unzip", ["-q", selected.path, "-d", temporary], {
        stdio: "inherit",
      });
    } else {
      await cp(selected.path, temporary, { recursive: true });
    }
    await inspectPackageDirectory(temporary);
    await rm(destination, { recursive: true, force: true });
    await rename(temporary, destination);
    if (selected.kind === "opk") {
      const retained = path.join(versionRoot, `alecaframe-${selected.version}.opk`);
      if (path.resolve(selected.path) !== path.resolve(retained)) {
        await cp(selected.path, retained);
      }
    }
  } catch (error) {
    await rm(temporary, { recursive: true, force: true });
    throw error;
  }
  return { version: selected.version, path: destination };
}

async function discoverCandidates(repositoryRoot) {
  const candidates = await researchCandidates(repositoryRoot);
  const localAppData = process.env.LOCALAPPDATA;
  if (localAppData) {
    candidates.push(
      ...(await packageCandidates(path.join(localAppData, "Overwolf/Extensions"), 3)),
      ...(await opkCandidates(path.join(localAppData, "Overwolf/PackagesCache"), 4)),
    );
  }
  try {
    candidates.push(await officialCandidate());
  } catch (error) {
    if (candidates.length === 0) throw error;
    process.stderr.write(`AlecaFrame update check failed; using local package: ${error.message}\n`);
  }
  return candidates;
}

async function researchCandidates(repositoryRoot) {
  const root = path.join(repositoryRoot, "research/alecaframe");
  return [
    ...(await packageCandidates(root, 2)),
    ...(await opkCandidates(root, 2)),
  ];
}

async function inspectSource(source) {
  const details = await stat(source).catch(() => null);
  if (!details) {
    throw new Error(`AlecaFrame source not found: ${source}`);
  }
  if (details.isFile()) {
    const candidate = inspectOpk(source);
    return candidate ? [candidate] : [];
  }
  if (!details.isDirectory()) {
    return [];
  }

  const direct = await inspectPackageDirectory(source).catch(() => null);
  if (direct) {
    return [direct];
  }
  const packaged = await inspectPackageDirectory(path.join(source, "package")).catch(() => null);
  if (packaged) {
    return [packaged];
  }
  return packageCandidates(source, 3);
}

async function packageCandidates(root, depth) {
  const files = await findNamed(root, "manifest.json", depth);
  const candidates = await Promise.all(
    files.map((manifest) => inspectPackageDirectory(path.dirname(manifest)).catch(() => null)),
  );
  return candidates.filter(Boolean);
}

async function opkCandidates(root, depth) {
  const files = await findExtension(root, ".opk", depth);
  return files.map(inspectOpk).filter(Boolean);
}

async function inspectPackageDirectory(packageRoot) {
  const manifest = JSON.parse(await readFile(path.join(packageRoot, "manifest.json"), "utf8"));
  if (manifest.meta?.name !== "AlecaFrame" || !manifest.meta?.version) {
    throw new Error(`${packageRoot} is not an AlecaFrame package`);
  }
  await Promise.all(REQUIRED_FILES.map((file) => stat(path.join(packageRoot, file))));
  return {
    kind: "directory",
    path: packageRoot,
    version: String(manifest.meta.version),
  };
}

function inspectOpk(opk) {
  try {
    const manifest = JSON.parse(execFileSync("unzip", ["-p", opk, "manifest.json"], {
      encoding: "utf8",
    }));
    if (manifest.meta?.name !== "AlecaFrame" || !manifest.meta?.version) {
      return null;
    }
    const entries = new Set(
      execFileSync("unzip", ["-Z1", opk], { encoding: "utf8" })
        .split(/\r?\n/)
        .filter(Boolean),
    );
    if (!REQUIRED_FILES.every((file) => entries.has(file))) {
      return null;
    }
    return {
      kind: "opk",
      path: opk,
      version: String(manifest.meta.version),
    };
  } catch {
    return null;
  }
}

async function findNamed(root, wanted, depth) {
  return findFiles(root, depth, (entry) => entry.name === wanted);
}

async function findExtension(root, extension, depth) {
  return findFiles(root, depth, (entry) => entry.name.endsWith(extension));
}

async function findFiles(root, depth, accepts) {
  if (depth < 0) return [];
  const entries = await readdir(root, { withFileTypes: true }).catch(() => []);
  const matches = [];
  for (const entry of entries) {
    const value = path.join(root, entry.name);
    if (entry.isFile() && accepts(entry)) {
      matches.push(value);
    } else if (entry.isDirectory() && depth > 0) {
      matches.push(...(await findFiles(value, depth - 1, accepts)));
    }
  }
  return matches;
}

function newest(candidates) {
  return candidates.toSorted((left, right) => {
    const version = compareVersions(right.version, left.version);
    if (version !== 0) return version;
    return candidateRank(right) - candidateRank(left);
  })[0];
}

function compareVersions(left, right) {
  const leftParts = left.split(".").map(Number);
  const rightParts = right.split(".").map(Number);
  const length = Math.max(leftParts.length, rightParts.length);
  for (let index = 0; index < length; index += 1) {
    const difference = (leftParts[index] ?? 0) - (rightParts[index] ?? 0);
    if (difference !== 0) return difference;
  }
  return 0;
}

async function officialCandidate() {
  const response = await fetch(INSTALL_METADATA, {
    signal: AbortSignal.timeout(15_000),
  });
  if (!response.ok) {
    throw new Error(`Overwolf metadata returned HTTP ${response.status}`);
  }
  const metadata = await response.json();
  const packageInfo = metadata.partnerConfiguration?.dock?.find(
    (entry) => entry.packageId === EXTENSION_ID && entry.url,
  );
  if (!packageInfo) {
    throw new Error("Overwolf metadata contains no AlecaFrame package");
  }
  const url = new URL(packageInfo.url);
  const match = new RegExp(
    `^/prod/apps/${EXTENSION_ID.replaceAll(".", "\\.")}/([^/]+)/app\\.opk$`,
  ).exec(url.pathname);
  if (url.protocol !== "https:" || url.hostname !== "appsdl.overwolf.com" || !match) {
    throw new Error(`unexpected AlecaFrame package URL: ${url.href}`);
  }
  return {
    kind: "remote",
    url: url.href,
    version: decodeURIComponent(match[1]),
  };
}

async function download(url, destination) {
  const response = await fetch(url, {
    signal: AbortSignal.timeout(120_000),
  });
  if (!response.ok || !response.body) {
    throw new Error(`AlecaFrame download returned HTTP ${response.status}`);
  }
  await pipeline(Readable.fromWeb(response.body), createWriteStream(destination, { mode: 0o600 }));
}

function candidateRank(candidate) {
  if (candidate.kind === "directory") return 2;
  if (candidate.kind === "opk") return 1;
  return 0;
}
