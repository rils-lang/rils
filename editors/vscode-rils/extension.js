"use strict";

const fs = require("node:fs");
const path = require("node:path");
const vscode = require("vscode");
const {
  LanguageClient,
  TransportKind,
} = require("vscode-languageclient/node");

let client;

function resolveServer() {
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
  for (const folder of vscode.workspace.workspaceFolders ?? []) {
    for (const profile of ["release", "debug"]) {
      const candidate = path.join(
        folder.uri.fsPath,
        "target",
        profile,
        executable,
      );
      if (fs.existsSync(candidate)) {
        return candidate;
      }
    }
  }
  return "rils-analyzer";
}

async function activate(context) {
  const serverOptions = {
    command: resolveServer(),
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
