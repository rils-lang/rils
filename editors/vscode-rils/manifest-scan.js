"use strict";

const fs = require("node:fs");
const path = require("node:path");

// Return partial results and diagnostics; one inaccessible subtree must not
// prevent the extension from starting its language client.
function scanManifests(root, readDirectory = fs.readdirSync) {
  const paths = [];
  const errors = [];
  const pending = [root];
  while (pending.length) {
    const directory = pending.pop();
    let entries;
    try {
      entries = readDirectory(directory, { withFileTypes: true });
    } catch (error) {
      if (error.code !== "ENOENT") {
        errors.push(`Cannot scan Rils manifests in ${directory}: ${error.message}`);
      }
      continue;
    }
    for (const entry of entries) {
      const entryPath = path.join(directory, entry.name);
      if (entry.isDirectory()) pending.push(entryPath);
      else if (entry.isFile() && entry.name.toLowerCase().endsWith(".rilhm")) paths.push(entryPath);
    }
  }
  return { paths, errors };
}

module.exports = { scanManifests };
