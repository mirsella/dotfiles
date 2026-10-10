import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";

const { attachDownloadListener, captureFetchedResource, clearTabDownloads } =
  await import(pathToFileURL(process.argv[2]));
const directory = await fs.mkdtemp(
  path.join(process.env.CAMOFOX_DOWNLOADS_DIR || os.tmpdir(), "camofox-download-test-"),
);
process.env.CAMOFOX_DOWNLOADS_DIR = directory;
const group = (await fs.stat(directory)).gid;
const contents = Buffer.from("private download fixture\n");
let onDownload;
const tab = {
  downloads: [],
  visitedUrls: new Set(),
  page: {
    on: (event, handler) => {
      assert.equal(event, "download");
      onDownload = handler;
    },
  },
};

try {
  attachDownloadListener(tab, "test-tab", () => {}, null, "test-user");
  await onDownload({
    suggestedFilename: () => "test.pdf",
    url: () => "https://example.com/test.pdf",
    failure: async () => {
      throw new Error("saveAs already owns download failure");
    },
    saveAs: async (file) => {
      await fs.writeFile(file, contents, { mode: 0o600 });
      // A transferred browser artifact can retain the staging process's group.
      await fs.chown(file, process.getuid(), process.getgid());
    },
  });
  await captureFetchedResource(tab, {
    url: "https://example.com/resource.pdf",
    mimeType: "application/pdf",
    filename: "resource.pdf",
    body: contents,
  });
  assert.equal(tab.downloads.length, 2);
  for (const download of tab.downloads) {
    assert.equal(download.failure, null);
    const metadata = await fs.stat(download.filePath);
    assert.equal(metadata.uid, process.getuid());
    assert.equal(metadata.gid, group);
    assert.equal(metadata.mode & 0o777, 0o640);
    assert.deepEqual(await fs.readFile(download.filePath), contents);
  }
  await clearTabDownloads(tab);
  assert.deepEqual(await fs.readdir(directory), []);

  await onDownload({
    suggestedFilename: () => "partial.pdf",
    saveAs: async (file) => {
      await fs.writeFile(file, contents, { mode: 0o600 });
      throw new Error("download interrupted");
    },
  });
  assert.equal(tab.downloads.length, 1);
  assert.equal(tab.downloads[0].failure, "download interrupted");
  assert.equal(tab.downloads[0].filePath, null);
  assert.deepEqual(await fs.readdir(directory), []);
  await clearTabDownloads(tab);

  const chmod = fs.chmod;
  try {
    fs.chmod = async (file) => {
      assert.equal((await fs.stat(file)).mode & 0o777, 0o600);
      throw new Error("publication denied");
    };
    await assert.rejects(captureFetchedResource(tab, {
      url: "https://example.com/partial.pdf",
      filename: "partial.pdf",
      body: contents,
    }), /publication denied/);
    assert.equal(tab.downloads.length, 0);
    assert.deepEqual(await fs.readdir(directory), []);

    const remove = fs.rm;
    try {
      fs.rm = async () => { throw new Error("cleanup denied"); };
      await assert.rejects(captureFetchedResource(tab, {
        url: "https://example.com/partial.pdf",
        filename: "partial.pdf",
        body: contents,
      }), (error) => {
        assert(error instanceof AggregateError);
        assert.deepEqual(error.errors.map((error) => error.message), [
          "publication denied", "cleanup denied",
        ]);
        assert.match(error.message, /publication denied.*cleanup denied/);
        return true;
      });
      assert.equal(tab.downloads.length, 0);
      const partial = await fs.readdir(directory);
      assert.equal(partial.length, 1);
      assert.equal((await fs.stat(path.join(directory, partial[0]))).mode & 0o777, 0o600);
    } finally {
      fs.rm = remove;
    }
  } finally {
    fs.chmod = chmod;
  }
  console.log("Downloads preserve private publication, failure cleanup, sharing, and native lifecycle");
} finally {
  await fs.rm(directory, { recursive: true, force: true });
}
