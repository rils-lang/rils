import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import * as vscode from "vscode";

export function resolveServer(context: vscode.ExtensionContext): string {
  const configured = vscode.workspace
    .getConfiguration("rils")
    .get<string>("server.path", "")
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

  const workspaceCandidates: { path: string; modified: number }[] = [];
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

