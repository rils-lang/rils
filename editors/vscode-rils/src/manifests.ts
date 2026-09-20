import * as path from "node:path";
import * as vscode from "vscode";
import { scanManifests } from "./manifest-scan";

export function resolveHostManifestPaths(strict = false): string[] {
  const paths = new Set<string>();
  for (const folder of vscode.workspace.workspaceFolders ?? []) {
    const configured = vscode.workspace
      .getConfiguration("rils", folder.uri)
      .get<string>("hostManifest.path", "")
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

