import { createLauncher } from "../scripts/lib/launcher.mjs";

export default function (pi) {
  const launcher = createLauncher();

  pi.registerCommand("pi-trace", {
    description: "Build or open Pi Trace; serve or watch a recording, or browse native sessions with system",
    handler: async (args, ctx) => {
      try {
        const command = args.trim();
        switch (command) {
          case "build":
            ctx.ui.notify("Building Pi Trace. Rust and Node.js with npm are required.", "info");
            await launcher.build();
            ctx.ui.notify("Pi Trace built. Run /pi-trace, /pi-trace serve <directory>, /pi-trace watch <directory>, or /pi-trace system to open the local app.", "info");
            return;
          case "stop":
            await launcher.stop();
            ctx.ui.notify("The Pi Trace process owned by this session has stopped.", "info");
            return;
          default: {
            const selection = /^\s*(serve|watch)\s(.+)$/s.exec(args);
            const source = selection?.[2];
            if (/^system\s/s.test(command)) {
              ctx.ui.notify("Use /pi-trace system without arguments to inherit provider settings. For explicit roots or disabled providers, use pi-trace-rs system --root <provider>=<path> --disable-provider <provider> in a terminal. Providers: codex, pi, claude, opencode.", "info");
              return;
            }
            if (command !== "" && command !== "system" && source === undefined) {
              ctx.ui.notify("Use /pi-trace, /pi-trace serve <directory>, /pi-trace watch <directory>, /pi-trace system, /pi-trace build, or /pi-trace stop.", "info");
              return;
            }
            const options = command === "system" ? { mode: "system", cwd: ctx.cwd } : { source, cwd: ctx.cwd, ...(selection?.[1] === "watch" ? { mode: "watch" } : {}) };
            const { url } = await launcher.start(options);
            ctx.ui.notify(`Pi Trace: ${url}`, "info");
            if (process.platform !== "linux" || process.env.PI_TRACE_NO_OPEN === "1") return;
            try {
              const result = await pi.exec("xdg-open", [url], { timeout: 5000, maxBuffer: 16_384 });
              if (result.code === 0) return;
            } catch {}
            ctx.ui.notify(`Open ${url} in your browser. The local server is running.`, "warning");
            return;
          }
        }
      } catch (error) {
        ctx.ui.notify(error.message, "error");
      }
    },
  });

  pi.on("session_shutdown", async () => {
    await launcher.dispose();
  });
}
