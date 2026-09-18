import * as vscode from "vscode";
import { LanguageClient } from "vscode-languageclient/node";
import { resolveHostManifestPaths } from "./manifests";

export function watchManifests(context: vscode.ExtensionContext, client: LanguageClient): void {
  const manifestWatchers: vscode.FileSystemWatcher[] = [];
  for (const folder of vscode.workspace.workspaceFolders ?? []) {
    const pattern = new vscode.RelativePattern(folder, ".rils/manifest/**/*.rilhm");
    const watcher = vscode.workspace.createFileSystemWatcher(pattern);
    const refresh = async () => {
      try {
        await client.sendNotification("rils/hostManifestChanged", {
          hostManifestPaths: resolveHostManifestPaths(true),
        });
      } catch (error) {
        console.error("Rils manifest refresh failed:", error);
      }
    };
    watcher.onDidCreate(refresh, null, context.subscriptions);
    watcher.onDidChange(refresh, null, context.subscriptions);
    watcher.onDidDelete(refresh, null, context.subscriptions);
    manifestWatchers.push(watcher);

    const projectWatcher = vscode.workspace.createFileSystemWatcher(
      new vscode.RelativePattern(folder, "**/rils.toml"),
    );
    projectWatcher.onDidChange(refresh, null, context.subscriptions);
    projectWatcher.onDidCreate(refresh, null, context.subscriptions);
    projectWatcher.onDidDelete(refresh, null, context.subscriptions);
    manifestWatchers.push(projectWatcher);
  }
  context.subscriptions.push(...manifestWatchers);
}
