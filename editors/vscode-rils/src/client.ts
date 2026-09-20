import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  Trace,
  TransportKind,
} from "vscode-languageclient/node";
import { resolveServer } from "./server";
import { resolveHostManifestPaths } from "./manifests";
import { watchManifests } from "./watchers";

let client: LanguageClient | undefined;

export async function startClient(context: vscode.ExtensionContext): Promise<void> {
  const workspaceDirectory = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
  const trace = vscode.workspace
    .getConfiguration("rils")
    .get<"off" | "messages" | "verbose">("server.trace", "off");
  const traceOutput = vscode.window.createOutputChannel("Rils LSP Trace", { log: true });
  context.subscriptions.push(traceOutput);
  const serverOptions: ServerOptions = {
    command: resolveServer(context),
    args: [],
    transport: TransportKind.stdio,
    options: workspaceDirectory ? { cwd: workspaceDirectory } : undefined,
  };
  const clientOptions: LanguageClientOptions = {
    traceOutputChannel: traceOutput,
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
  await client.setTrace(
    trace === "verbose" ? Trace.Verbose : trace === "messages" ? Trace.Messages : Trace.Off,
  );
  traceOutput.appendLine(`Rils Analyzer command: ${serverOptions.command}`);
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
