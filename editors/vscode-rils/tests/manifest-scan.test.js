"use strict";
const { test } = require("node:test");
const assert = require("node:assert/strict");
const path = require("node:path");
const { scanManifests } = require("../manifest-scan");

test("inaccessible subtree does not discard sibling manifests", () => {
  const root = path.resolve("manifests");
  const entry = (name, directory) => ({ name, isDirectory: () => directory, isFile: () => !directory });
  const result = scanManifests(root, (directory) => {
    if (directory === root) return [entry("blocked", true), entry("good", true)];
    if (directory === path.join(root, "good")) return [entry("game.rilhm", false)];
    throw Object.assign(new Error("denied"), { code: "EACCES" });
  });
  assert.deepEqual(result.paths, [path.join(root, "good", "game.rilhm")]);
  assert.equal(result.errors.length, 1);
  assert.match(result.errors[0], /blocked/);
});

test("absent optional manifest directory is not an error", () => {
  assert.deepEqual(scanManifests("absent", () => {
    throw Object.assign(new Error("absent"), { code: "ENOENT" });
  }), { paths: [], errors: [] });
});
