// Renders crates/app/assets/icon.svg into icon.png (256 px, the window icon) and icon.ico
// (256/64/48/32/24/16, embedded in the exe by crates/app/build.rs).
//
// If icon-small.svg exists next to it, the 32 px and smaller frames use that simpler drawing
// so the icon stays readable in the taskbar and Explorer's small views.
//
// Needs Node and Microsoft Edge (headless) only:  node scripts/make-icon.mjs

import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const assets = join(root, "crates", "app", "assets");
const SIZES = [256, 64, 48, 32, 24, 16];
const SMALL_MAX = 32;

const EDGE = [
  "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
  "C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe",
].find(existsSync);
if (!EDGE) throw new Error("Microsoft Edge not found");

const dataUrl = (file) =>
  "data:image/svg+xml;base64," + readFileSync(join(assets, file)).toString("base64");
const big = dataUrl("icon.svg");
const small = existsSync(join(assets, "icon-small.svg")) ? dataUrl("icon-small.svg") : big;

// The page draws each frame onto a canvas and writes the PNG data URLs into the DOM, which
// headless Edge prints with --dump-dom once the virtual-time budget has let the images load.
const frames = SIZES.map((s) => ({ size: s, src: s <= SMALL_MAX ? small : big }));
const page = `<!doctype html><meta charset="utf-8"><pre id="out"></pre><script>
const frames = ${JSON.stringify(frames)};
Promise.all(frames.map(f => new Promise((ok, fail) => {
  const img = new Image();
  img.onload = () => {
    const c = document.createElement("canvas");
    c.width = c.height = f.size;
    const g = c.getContext("2d");
    g.imageSmoothingQuality = "high";
    g.drawImage(img, 0, 0, f.size, f.size);
    ok(f.size + " " + c.toDataURL("image/png"));
  };
  img.onerror = fail;
  img.src = f.src;
}))).then(lines => { document.getElementById("out").textContent = lines.join("\\n"); });
</script>`;

const tmp = mkdtempSync(join(tmpdir(), "beatslopper-icon-"));
let dom;
try {
  const html = join(tmp, "render.html");
  writeFileSync(html, page);
  dom = execFileSync(
    EDGE,
    [
      "--headless=new",
      "--disable-gpu",
      `--user-data-dir=${join(tmp, "profile")}`,
      "--virtual-time-budget=5000",
      "--dump-dom",
      pathToFileURL(html).href,
    ],
    { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 },
  );
} finally {
  rmSync(tmp, { recursive: true, force: true });
}

const pngs = new Map();
for (const m of dom.matchAll(/(\d+) data:image\/png;base64,([A-Za-z0-9+/=]+)/g)) {
  pngs.set(Number(m[1]), Buffer.from(m[2], "base64"));
}
for (const s of SIZES) if (!pngs.has(s)) throw new Error(`Edge did not render the ${s} px frame`);

// ICO with PNG-compressed entries (supported since Windows Vista).
const header = Buffer.alloc(6);
header.writeUInt16LE(0, 0);
header.writeUInt16LE(1, 2);
header.writeUInt16LE(SIZES.length, 4);
const entries = [];
let offset = 6 + 16 * SIZES.length;
for (const s of SIZES) {
  const png = pngs.get(s);
  const e = Buffer.alloc(16);
  e.writeUInt8(s >= 256 ? 0 : s, 0);
  e.writeUInt8(s >= 256 ? 0 : s, 1);
  e.writeUInt16LE(1, 4); // colour planes
  e.writeUInt16LE(32, 6); // bits per pixel
  e.writeUInt32LE(png.length, 8);
  e.writeUInt32LE(offset, 12);
  offset += png.length;
  entries.push(e);
}
writeFileSync(join(assets, "icon.ico"), Buffer.concat([header, ...entries, ...SIZES.map((s) => pngs.get(s))]));
writeFileSync(join(assets, "icon.png"), pngs.get(256));
console.log(`wrote icon.ico (${SIZES.join("/")} px) and icon.png (256 px) in ${assets}`);
