"use strict";

const fs = require("node:fs");
const path = require("node:path");
const vscode = require("vscode");
const {
  LanguageClient,
  TransportKind,
} = require("vscode-languageclient/node");

let client;

function resolveServer(context) {
  const configured = vscode.workspace
    .getConfiguration("rils")
    .get("server.path", "")
    .trim();
  if (configured) {
    return configured;
  }

  const executable = process.platform === "win32"
    ? "rils-analyzer.exe"
    : "rils-analyzer";
  const bundled = path.join(context.extensionPath, "server", executable);
  if (fs.existsSync(bundled)) {
    return bundled;
  }

  const workspaceCandidates = [];
  for (const folder of vscode.workspace.workspaceFolders ?? []) {
    for (const profile of ["release", "debug"]) {
      const candidate = path.join(
        folder.uri.fsPath,
        "target",
        profile,
        executable,
      );
      if (fs.existsSync(candidate)) {
        workspaceCandidates.push({
          path: candidate,
          modified: fs.statSync(candidate).mtimeMs,
        });
      }
    }
  }
  workspaceCandidates.sort((left, right) => right.modified - left.modified);
  if (workspaceCandidates.length > 0) {
    return workspaceCandidates[0].path;
  }
  return "rils-analyzer";
}

async function activate(context) {
  const serverOptions = {
    command: resolveServer(context),
    args: [],
    transport: TransportKind.stdio,
  };
  const clientOptions = {
    documentSelector: [
      { scheme: "file", language: "rils" },
      { scheme: "untitled", language: "rils" },
    ],
    synchronize: {
      configurationSection: "rils",
    },
  };

  client = new LanguageClient(
    "rilsAnalyzer",
    "Rils Analyzer",
    serverOptions,
    clientOptions,
  );
  context.subscriptions.push(client);
  await client.start();
}

async function deactivate() {
  if (client) {
    await client.stop();
    client = undefined;
  }
}

module.exports = { activate, deactivate };
