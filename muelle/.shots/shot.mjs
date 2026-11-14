// Dev-only screenshot helper: drives the installed Chrome over CDP so a section
// below the fold can be captured after scrolling (headless --screenshot cannot scroll).
// Usage: node .shots/shot.mjs <url> <out.png> <width> <height> <selector|y>
import { spawn } from "node:child_process";
import { writeFileSync } from "node:fs";

const CHROME = "C:/Program Files/Google/Chrome/Application/chrome.exe";
const PORT = 9333;
const [, , url = "http://localhost:4321/", out = ".shots/shot.png", w = "1440", h = "1000", target = "0"] =
  process.argv;

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const chrome = spawn(
  CHROME,
  [
    "--headless=new",
    `--remote-debugging-port=${PORT}`,
    "--user-data-dir=C:/Users/USUARIO/AppData/Local/Temp/muelle-cdp",
    "--hide-scrollbars",
    "--no-first-run",
    "--disable-gpu",
    "about:blank",
  ],
  { stdio: "ignore" },
);

let ws;
try {
  for (let i = 0; i < 60; i++) {
    try {
      const res = await fetch(`http://127.0.0.1:${PORT}/json/new?about:blank`, { method: "PUT" });
      if (res.ok) {
        ws = new WebSocket((await res.json()).webSocketDebuggerUrl);
        break;
      }
    } catch {
      await sleep(250);
    }
  }
  if (!ws) throw new Error("no CDP target");

  await new Promise((r) => ws.addEventListener("open", r, { once: true }));

  let seq = 0;
  const pending = new Map();
  ws.addEventListener("message", (event) => {
    const msg = JSON.parse(event.data);
    const done = pending.get(msg.id);
    if (done) {
      pending.delete(msg.id);
      done(msg.result ?? {});
      return;
    }
    if (msg.method === "Runtime.exceptionThrown") {
      const d = msg.params.exceptionDetails;
      console.log("EXCEPTION:", d.exception?.description ?? d.text);
    }
    if (msg.method === "Runtime.consoleAPICalled" && msg.params.type === "error") {
      console.log("CONSOLE:", msg.params.args.map((a) => a.value ?? a.description).join(" "));
    }
  });
  const send = (method, params = {}) =>
    new Promise((resolve) => {
      const id = ++seq;
      pending.set(id, resolve);
      ws.send(JSON.stringify({ id, method, params }));
    });

  await send("Page.enable");
  await send("Runtime.enable");
  await send("Emulation.setDeviceMetricsOverride", {
    width: Number(w),
    height: Number(h),
    deviceScaleFactor: 1,
    mobile: false,
  });
  await send("Page.navigate", { url });

  for (let i = 0; i < 80; i++) {
    const state = await send("Runtime.evaluate", { expression: "document.readyState" });
    if (state.result?.value === "complete") break;
    await sleep(150);
  }
  await sleep(1500);

  const scroll = /^\d+$/.test(target)
    ? Number(target)
    : ((
        await send("Runtime.evaluate", {
          expression: `Math.max(0, document.querySelector(${JSON.stringify(target)}).getBoundingClientRect().top + scrollY - 40)`,
        })
      ).result?.value ?? 0);
  await send("Runtime.evaluate", { expression: `scrollTo(0, ${scroll})` });
  await sleep(1800);

  const hover = process.argv[7];
  if (hover) {
    const [hx, hy] = hover.split(",").map(Number);
    await send("Page.bringToFront");
    await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: hx, y: hy, button: "none", buttons: 0 });
    await sleep(700);
    const probe = await send("Runtime.evaluate", {
      expression: `(() => {
        const card = document.querySelectorAll("[data-spotlight]")[1];
        return JSON.stringify({
          mx: card.style.getPropertyValue("--mx"),
          tiltX: card.style.getPropertyValue("--tx"),
          slab: getComputedStyle(card.querySelector(".slab")).transform,
        });
      })()`,
    });
    console.log("hover:", probe.result?.value);

    // Headless CDP mouse input is not always delivered; a synthetic event still proves
    // the listeners are attached and writing the variables the CSS reads.
    await send("Runtime.evaluate", {
      expression: `(() => {
        try {
          const card = document.querySelectorAll("[data-spotlight]")[1];
          card.dispatchEvent(new PointerEvent("pointermove", { clientX: ${hx}, clientY: ${hy}, bubbles: true }));
          return JSON.stringify({
            mx: card.style.getPropertyValue("--mx"),
            my: card.style.getPropertyValue("--my"),
            tiltX: card.style.getPropertyValue("--tx"),
            slab: getComputedStyle(card.querySelector(".slab")).transform,
          });
        } catch (error) {
          return "ERR " + error.message;
        }
      })()`,
    }).then((r) => console.log("synthetic:", r.result?.value));
  }

  const state = await send("Runtime.evaluate", {
    expression: `JSON.stringify({
      revealed: document.querySelector("#features")?.dataset.revealed ?? "sin marcar",
      sliderTick: document.querySelector("[data-magnify-range]")?.style.getPropertyValue("--p") ?? "sin valor",
      cardOpacity: getComputedStyle(document.querySelector("[data-spotlight]")).opacity,
    })`,
  });
  console.log("page:", state.result?.value);

  const shot = await send("Page.captureScreenshot", { format: "png", captureBeyondViewport: false });
  writeFileSync(out, Buffer.from(shot.data, "base64"));
  console.log(`saved ${out} at y=${scroll}`);
  await send("Browser.close");
} finally {
  try {
    ws?.close();
  } catch {}
  setTimeout(() => chrome.kill(), 300);
}
