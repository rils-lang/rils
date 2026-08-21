"use strict";

const fs = require("node:fs");
const path = require("node:path");

const root = path.resolve(__dirname, "..");
const repositoryRoot = path.resolve(root, "..", "..");
for (const file of [
  "package.json",
  "language-configuration.json",
  "syntaxes/rils.tmLanguage.json",
]) {
  JSON.parse(fs.readFileSync(path.join(root, file), "utf8"));
}

const manifest = JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8"));
const lockfile = JSON.parse(fs.readFileSync(path.join(root, "package-lock.json"), "utf8"));
const cargoManifest = fs.readFileSync(path.join(repositoryRoot, "Cargo.toml"), "utf8");
const grammar = fs.readFileSync(
  path.join(root, "syntaxes/rils.tmLanguage.json"),
  "utf8",
);
const cargoVersion = cargoManifest.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
const bundledExtension = path.join(root, manifest.main);

if (!fs.existsSync(bundledExtension)) {
  throw new Error(`Bundled extension was not generated: ${bundledExtension}`);
}

if (!cargoVersion) {
  throw new Error("Could not read the Rils version from Cargo.toml.");
}
if (manifest.version !== cargoVersion) {
  throw new Error(
    `Extension version ${manifest.version} does not match Rils ${cargoVersion}.`,
  );
}
if (
  lockfile.version !== manifest.version ||
  lockfile.packages?.[""]?.version !== manifest.version
) {
  throw new Error("package-lock.json does not match the extension version.");
}

for (const requiredSyntax of [
  '"#characters"',
  "constant.numeric.float.rils",
  "HashMap|HashSet",
  "derive|Default",
  "constant.other.symbol.enum.rils",
  "keyword.other.self.rils",
]) {
  if (!grammar.includes(requiredSyntax)) {
    throw new Error(`TextMate grammar is missing support for ${requiredSyntax}.`);
  }
}

new Function(fs.readFileSync(bundledExtension, "utf8"));
console.log("VS Code extension files are valid.");
