import { realpath } from "node:fs/promises";
import { dirname, basename, resolve, isAbsolute, relative, sep } from "node:path";

const providers = ["claude", "codex", "opencode", "pi"];

function providerName(value) {
  if (!providers.includes(value)) {
    throw new Error(`Unknown provider ${JSON.stringify(value)}. Use codex, pi, claude, or opencode.`);
  }
  return value;
}

async function canonicalPath(path) {
  let ancestor = path;
  const missing = [];
  for (;;) {
    try {
      return resolve(await realpath(ancestor), ...missing);
    } catch (error) {
      if (!["ENOENT", "ENOTDIR"].includes(error.code)) {
        if (["EACCES", "EPERM", "ELOOP"].includes(error.code)) return path;
        throw error;
      }
      const parent = dirname(ancestor);
      if (parent === ancestor) return path;
      missing.unshift(basename(ancestor));
      ancestor = parent;
    }
  }
}

function contained(directory, path) {
  const suffix = relative(directory, path);
  return suffix !== ".." && !suffix.startsWith(`..${sep}`) && !isAbsolute(suffix);
}

export async function resolveSystemConfig({ roots = [], disabledProviders = [], cwd, env, home }) {
  if (!Array.isArray(roots) || !Array.isArray(disabledProviders)) {
    throw new Error("System roots and disabled providers must be arrays.");
  }
  const disabled = new Set(disabledProviders.map(providerName));
  const overrides = new Map();
  for (const root of roots) {
    if (typeof root !== "string" || !root.includes("=")) {
      throw new Error("Use --root <provider>=<path> with a nonempty path.");
    }
    const separator = root.indexOf("=");
    const provider = providerName(root.slice(0, separator));
    const path = root.slice(separator + 1);
    if (!path || path.includes("\0")) throw new Error("Use --root <provider>=<path> with a nonempty path.");
    if (provider === "opencode" && path === ":memory:") throw new Error("OpenCode :memory: cannot be discovered. Select an on-disk database.");
    if (disabled.has(provider)) throw new Error(`Provider ${provider} cannot have both --root and --disable-provider.`);
    if (!overrides.has(provider)) overrides.set(provider, []);
    overrides.get(provider).push(path);
  }
  const defaults = {
    codex: () => env.CODEX_HOME || resolve(home, ".codex"),
    pi: () => env.PI_CODING_AGENT_SESSION_DIR || resolve(cwd, env.PI_CODING_AGENT_DIR || resolve(home, ".pi/agent"), "sessions"),
    claude: () => resolve(cwd, env.CLAUDE_CONFIG_DIR || resolve(home, ".claude"), "projects"),
    opencode: async () => {
      const directory = resolve(cwd, env.XDG_DATA_HOME || resolve(home, ".local/share"), "opencode");
      const selected = env.OPENCODE_DB;
      if (selected === ":memory:") throw new Error("OPENCODE_DB=:memory: cannot be discovered. Select an on-disk database with --root opencode=<path> or disable opencode.");
      if (!selected) return directory;
      if (isAbsolute(selected)) return selected;
      const path = resolve(directory, selected);
      const [canonicalDirectory, canonicalSelected] = await Promise.all([canonicalPath(directory), canonicalPath(path)]);
      if (!contained(directory, path) || !contained(canonicalDirectory, canonicalSelected)) {
        throw new Error("Relative OPENCODE_DB must stay inside the OpenCode data directory. Use an absolute database path to select an external file.");
      }
      return canonicalSelected;
    },
  };
  const stores = [];
  const args = ["system"];
  for (const provider of providers) {
    if (disabled.has(provider)) {
      args.push("--disable-provider", provider);
      continue;
    }
    const selected = overrides.get(provider) ?? [await defaults[provider]()];
    const paths = [...new Set(await Promise.all(selected.map((path) => canonicalPath(resolve(cwd, path)))))].sort();
    stores.push({ provider, paths });
    for (const path of paths) args.push("--root", `${provider}=${path}`);
  }
  return { args, identity: JSON.stringify({ stores, disabledProviders: [...disabled].sort() }) };
}
