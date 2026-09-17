"use strict";

const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const vscode = require("vscode");
const { scanManifests } = require("./manifest-scan");
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

  const rilsHome = process.env.RILS_HOME?.trim() || path.join(os.homedir(), ".rils");
  const managed = path.join(rilsHome, "bin", executable);
  if (fs.existsSync(managed)) {
    return managed;
  }

  const workspaceCandidates = [];
  for (const folder of vscode.workspace.workspaceFolders ?? []) {
    // A Unity integration project lives below the Rils repository and usually
    // has no local target directory. Walk its parents so the analyzer built by
    // the workspace is preferred over an unrelated executable on PATH.
    let directory = folder.uri.fsPath;
    while (directory) {
      for (const profile of ["release", "debug"]) {
        const candidate = path.join(directory, "target", profile, executable);
        if (fs.existsSync(candidate)) {
          let modified;
          try {
            modified = fs.statSync(candidate).mtimeMs;
          } catch {
            continue; // A build may replace a candidate during discovery.
          }
          workspaceCandidates.push({
            path: candidate,
            modified,
          });
        }
      }
      const parent = path.dirname(directory);
      if (parent === directory) break;
      directory = parent;
    }
  }
  workspaceCandidates.sort((left, right) => right.modified - left.modified);
  if (workspaceCandidates.length > 0) {
    return workspaceCandidates[0].path;
  }
  return "rils-analyzer";
}

function resolveHostManifestPaths(strict = false) {
  const paths = new Set();
  for (const folder of vscode.workspace.workspaceFolders ?? []) {
    const configured = vscode.workspace
      .getConfiguration("rils", folder.uri)
      .get("hostManifest.path", "")
      .trim();
    if (configured) {
      paths.add(path.isAbsolute(configured)
        ? configured
        : path.join(folder.uri.fsPath, configured));
      continue;
    }

    const manifestDirectory = path.join(folder.uri.fsPath, ".rils", "manifest");
    const scan = scanManifests(manifestDirectory);
    for (const manifest of scan.paths) paths.add(manifest);
    for (const error of scan.errors) {
      console.error(error);
      void vscode.window.showWarningMessage(error);
    }
    if (strict && scan.errors.length) {
      throw new Error("Rils manifest scan failed; retaining the previous host model.");
    }
  }
  return [...paths].sort((left, right) => left.localeCompare(right));
}

async function activate(context) {
  const workspaceDirectory = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
  const serverOptions = {
    command: resolveServer(context),
    args: [],
    transport: TransportKind.stdio,
    options: workspaceDirectory ? { cwd: workspaceDirectory } : undefined,
  };
  const clientOptions = {
    documentSelector: [
      { scheme: "file", language: "rils" },
      { scheme: "untitled", language: "rils" },
    ],
    synchronize: {
      configurationSection: "rils",
    },
    initializationOptions: {
      hostManifestPaths: resolveHostManifestPaths(),
    },
  };

  client = new LanguageClient(
    "rilsAnalyzer",
    "Rils Analyzer",
    serverOptions,
    clientOptions,
  );
  context.subscriptions.push(client);
  const manifestWatchers = [];
  for (const folder of vscode.workspace.workspaceFolders ?? []) {
    const pattern = new vscode.RelativePattern(folder, ".rils/manifest/**/*.rilhm");
    const watcher = vscode.workspace.createFileSystemWatcher(pattern);
    const refresh = async () => {
      if (client) {
        try {
          await client.sendNotification("rils/hostManifestChanged", {
            hostManifestPaths: resolveHostManifestPaths(true),
          });
        } catch (error) {
          console.error("Rils manifest refresh failed:", error);
        }
      }
    };
    watcher.onDidCreate(refresh, null, context.subscriptions);
    watcher.onDidChange(refresh, null, context.subscriptions);
    watcher.onDidDelete(refresh, null, context.subscriptions);
    manifestWatchers.push(watcher);

    const projectWatcher = vscode.workspace.createFileSystemWatcher(
      new vscode.RelativePattern(folder, "rils.toml"),
    );
    projectWatcher.onDidChange(refresh, null, context.subscriptions);
    projectWatcher.onDidCreate(refresh, null, context.subscriptions);
    projectWatcher.onDidDelete(refresh, null, context.subscriptions);
    manifestWatchers.push(projectWatcher);
  }
  context.subscriptions.push(...manifestWatchers);
  await client.start();
}

async function deactivate() {
  if (client) {
    await client.stop();
    client = undefined;
  }
}

module.exports = { activate, deactivate };
