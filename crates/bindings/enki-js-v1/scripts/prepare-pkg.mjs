import { readFile, rename, writeFile } from "node:fs/promises";
import path from "node:path";

const outDir = process.argv[2];

if (!outDir) {
  throw new Error("Usage: node scripts/prepare-pkg.mjs <out-dir>");
}

const pkgDir = path.resolve(process.cwd(), outDir);
const jsPath = path.join(pkgDir, "enki_js.js");
const corePath = path.join(pkgDir, "enki_js_core.js");
const dtsPath = path.join(pkgDir, "enki_js.d.ts");
const packageJsonPath = path.join(pkgDir, "package.json");

await rename(jsPath, corePath).catch(async (error) => {
  if (error?.code === "EEXIST") {
    await writeFile(corePath, await readFile(jsPath, "utf8"), "utf8");
    return;
  }

  throw error;
});

const wrapperSource = `import initWasm, {
  initSync,
  EnkiJsAgent as RawEnkiJsAgent
} from "./enki_js_core.js";

let initPromise;

function ensureWasm(input) {
  if (!initPromise || input !== undefined) {
    initPromise = initWasm(input);
  }

  return initPromise;
}

async function requireInstance(agent) {
  const instance = await agent.__instancePromise;

  if (!instance) {
    throw new Error("EnkiJsAgent is disposed.");
  }

  return instance;
}

export default function init(input) {
  return ensureWasm(input);
}

export { initSync };

export class EnkiJsAgent {
  constructor(...args) {
    this.__instance = null;
    this.__disposed = false;
    this.__instancePromise = ensureWasm().then(() => {
      if (this.__disposed) {
        return null;
      }

      const instance = new RawEnkiJsAgent(...args);
      this.__instance = instance;

      if (this.__disposed) {
        instance.free();
        this.__instance = null;
        return null;
      }

      return instance;
    });
  }

  async ready() {
    await requireInstance(this);
  }

  async run(session_id, user_message) {
    const instance = await requireInstance(this);
    return instance.run(session_id, user_message);
  }

  toolCatalogJson() {
    if (!this.__instance) {
      throw new Error(
        "EnkiJsAgent is still initializing. Await agent.ready() or agent.run(...) first."
      );
    }

    return this.__instance.toolCatalogJson();
  }

  free() {
    this.__disposed = true;

    if (this.__instance) {
      this.__instance.free();
      this.__instance = null;
      return;
    }

    void this.__instancePromise.then((instance) => {
      if (instance) {
        instance.free();
      }
    });
  }
}

if (Symbol.dispose) {
  EnkiJsAgent.prototype[Symbol.dispose] = EnkiJsAgent.prototype.free;
}
`;

await writeFile(jsPath, wrapperSource, "utf8");

const dtsSource = await readFile(dtsPath, "utf8");
const nextDtsSource = dtsSource.replace(
  /export class EnkiJsAgent \{\r?\n\s+free\(\): void;/,
  `export class EnkiJsAgent {
    ready(): Promise<void>;
    free(): void;`
);

await writeFile(dtsPath, nextDtsSource, "utf8");

const packageJson = JSON.parse(await readFile(packageJsonPath, "utf8"));
const files = new Set(packageJson.files ?? []);
files.add("enki_js.js");
files.add("enki_js_core.js");
files.add("enki_js_bg.wasm");
files.add("enki_js.d.ts");

packageJson.files = Array.from(files);

await writeFile(
  packageJsonPath,
  `${JSON.stringify(packageJson, null, 2)}\n`,
  "utf8"
);
