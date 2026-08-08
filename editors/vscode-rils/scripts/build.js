"use strict";

const path = require("node:path");
const esbuild = require("esbuild");

const root = path.resolve(__dirname, "..");

esbuild.buildSync({
  entryPoints: [path.join(root, "extension.js")],
  outfile: path.join(root, "out", "extension.js"),
  bundle: true,
  external: ["vscode"],
  format: "cjs",
  platform: "node",
  target: "node20",
  minify: true,
  legalComments: "none",
  logLevel: "info",
});
