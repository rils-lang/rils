import * as vscode from "vscode";
import { LanguageClient, LanguageClientOptions, ServerOptions, TransportKind } from "vscode-languageclient/node";
import { resolveServer } from "./server";
import { resolveHostManifestPaths } from "./manifests";
import { watchManifests } from "./watchers";

let client: LanguageClient | undefined;

export async function startClient(context: vscode.ExtensionContext): Promise<void> {
  const workspaceDirectory = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
  const serverOptions: ServerOptions = {
    command: resolveServer(context),
    args: [],
    transport: TransportKind.stdio,
    options: workspaceDirectory ? { cwd: workspaceDirectory } : undefined,
  };
  const clientOptions: LanguageClientOptions = {
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
  watchManifests(context, client);
  await client.start();
}

export async function stopClient(): Promise<void> {
  if (client) {
    await client.stop();
    client = undefined;
  }
}

