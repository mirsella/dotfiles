import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";

const { attachDownloadListener, captureFetchedResource, clearTabDownloads } =
  await import(pathToFileURL(process.argv[2]));
const directory = await fs.mkdtemp(path.join(os.tmpdir(), "camofox-download-test-"));
process.env.CAMOFOX_DOWNLOADS_DIR = directory;
const tab = { downloads: [] };
const resource = {
  url: "https://example.com/resource.pdf",
  filename: "resource.pdf",
  body: Buffer.from("download fixture\n"),
};

try {
  await captureFetchedResource(tab, resource);
  assert.equal(tab.downloads.length, 1);
  assert.equal((await fs.stat(tab.downloads[0].filePath)).mode & 0o777, 0o640);
  await clearTabDownloads(tab);
  assert.deepEqual(await fs.readdir(directory), []);

  const chmod = fs.chmod;
  fs.chmod = async (file) => {
    assert.equal((await fs.stat(file)).mode & 0o777, 0o600);
    throw new Error("publication denied");
  };
  try {
    await assert.rejects(captureFetchedResource(tab, resource), /publication denied/);
    assert.deepEqual(await fs.readdir(directory), []);
  } finally {
    fs.chmod = chmod;
  }

  let onDownload;
  const rm = fs.rm;
  const tabState = {
    downloads: [],
    visitedUrls: new Set(),
    page: { on: (_event, listener) => (onDownload = listener) },
  };
  attachDownloadListener(tabState, "tab-1", () => {}, null, null);
  const download = {
    url: () => resource.url,
    suggestedFilename: () => resource.filename,
    saveAs: (file) => fs.writeFile(file, resource.body),
    failure: async () => null,
  };
  await onDownload(download);
  assert.equal(tabState.downloads.length, 1);
  assert.equal((await fs.stat(tabState.downloads[0].filePath)).mode & 0o777, 0o640);
  await clearTabDownloads(tabState);
  assert.deepEqual(await fs.readdir(directory), []);

  const stat = fs.stat;
  fs.stat = async (file) => {
    if (file.startsWith(directory)) throw new Error("download stat failed");
    return stat(file);
  };
  try {
    await onDownload(download);
    assert.equal(tabState.downloads.at(-1).filePath, null);
    assert.match(tabState.downloads.at(-1).failure, /download stat failed/);
    assert.deepEqual(await fs.readdir(directory), []);
  } finally {
    fs.stat = stat;
  }
  await clearTabDownloads(tabState);

  fs.chmod = async () => {
    throw new Error("publication denied");
  };
  try {
    await onDownload(download);
    assert.equal(tabState.downloads.length, 1);
    assert.equal(tabState.downloads[0].filePath, null);
    assert.match(tabState.downloads[0].failure, /publication denied/);
    assert.deepEqual(await fs.readdir(directory), []);
  } finally {
    fs.chmod = chmod;
  }

  fs.chmod = async () => {
    throw new Error("publication denied");
  };
  fs.rm = async () => {
    throw new Error("cleanup denied");
  };
  try {
    await onDownload(download);
    assert.match(tabState.downloads.at(-1).failure, /publication denied/);
    assert.match(tabState.downloads.at(-1).failure, /cleanup denied/);
  } finally {
    fs.chmod = chmod;
    fs.rm = rm;
  }
  const orphaned = await fs.readdir(directory);
  assert.equal(orphaned.length, 1);
  await fs.rm(path.join(directory, orphaned[0]), { force: true });

  fs.chmod = async () => {
    throw new Error("publication denied");
  };
  fs.rm = async () => {
    throw new Error("cleanup denied");
  };
  try {
    await assert.rejects(captureFetchedResource(tab, resource), (error) => {
      assert(error instanceof AggregateError);
      assert.deepEqual(error.errors.map(({ message }) => message), [
        "publication denied",
        "cleanup denied",
      ]);
      return true;
    });
    const partial = await fs.readdir(directory);
    assert.equal(partial.length, 1);
    assert.equal((await fs.stat(path.join(directory, partial[0]))).mode & 0o777, 0o600);
  } finally {
    fs.chmod = chmod;
    fs.rm = rm;
  }
} finally {
  await fs.rm(directory, { recursive: true, force: true });
}
