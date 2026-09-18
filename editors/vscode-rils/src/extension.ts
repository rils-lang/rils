import type { ExtensionContext } from "vscode";
import { startClient, stopClient } from "./client";

export function activate(context: ExtensionContext): Promise<void> {
  return startClient(context);
}

export function deactivate(): Promise<void> {
  return stopClient();
}
