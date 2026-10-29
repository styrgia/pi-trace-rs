import { createLauncher } from "../scripts/lib/launcher.mjs";

export default function (pi) {
  const launcher = createLauncher();

  pi.registerCommand("pi-trace", {
    description: "Build or open the Pi Trace development bootstrap",
    handler: async (args, ctx) => {
      try {
        switch (args.trim()) {
          case "build":
            ctx.ui.notify("Building Pi Trace. Rust and Node.js with npm are required.", "info");
            await launcher.build();
            ctx.ui.notify("Pi Trace built. Run /pi-trace to open the local bootstrap.", "info");
            return;
          case "stop":
            await launcher.stop();
            ctx.ui.notify("The Pi Trace process owned by this session has stopped.", "info");
            return;
          case "": {
            const { url } = await launcher.start();
            ctx.ui.notify(`Pi Trace development bootstrap: ${url}. Trace analysis is not implemented yet.`, "info");
            if (process.platform !== "linux" || process.env.PI_TRACE_NO_OPEN === "1") return;
            try {
              const result = await pi.exec("xdg-open", [url], { timeout: 5000, maxBuffer: 16_384 });
              if (result.code === 0) return;
            } catch {}
            ctx.ui.notify(`Open ${url} in your browser. The local server is running.`, "warning");
            return;
          }
          default:
            ctx.ui.notify("Use /pi-trace, /pi-trace build, or /pi-trace stop.", "info");
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
