
import * as fs from "node:fs";
import * as path from "node:path";

// Return partial results and diagnostics; one inaccessible subtree must not
// prevent the extension from starting its language client.
export function scanManifests(
  root: string,
  readDirectory: (path: string, options: { withFileTypes: true }) => Pick<fs.Dirent, "name" | "isDirectory" | "isFile">[] = fs.readdirSync,
): { paths: string[]; errors: string[] } {
  const paths: string[] = [];
  const errors: string[] = [];
  const pending = [root];
  while (pending.length) {
    const directory = pending.pop()!;
    let entries;
    try {
      entries = readDirectory(directory, { withFileTypes: true });
    } catch (caught) {
      const error = caught as NodeJS.ErrnoException;
      if (error.code !== "ENOENT") {
        errors.push(`Cannot scan Rils manifests in ${directory}: ${error.message}`);
      }
      continue;
    }
    for (const entry of entries) {
      const entryPath = path.join(directory, entry.name);
      if (entry.isDirectory()) pending.push(entryPath);
      else if (entry.isFile() && entry.name.toLowerCase().endsWith(".rilhm")) paths.push(entryPath);
    }
  }
  return { paths, errors };
}


