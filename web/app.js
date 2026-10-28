import init, { Network, blank_hard_disk_10mb } from "./pkg/icon_web.js";

const FS = 0;
const $ = (id) => document.getElementById(id);
// Per-viewer conveniences; storage can be unavailable (private windows).
const local = {
  get(k) { try { return localStorage.getItem(k); } catch { return null; } },
  set(k, v) { try { localStorage.setItem(k, v); } catch {} },
};

let net = null;
let focus = FS;
let floppy = null;   // { name, data }
let hardDisk = null; // { name, data }, kept across power cycles
let rom = null;      // { name, data }

// ---- Machines: slot 0 is the fileserver, built into the page; workstations
// are cloned from a template. Each entry holds that machine's elements.
const machines = [{
  root: $("m0"), canvas: $("screen0"), status: $("status0"), pwr: $("pwr0"),
  trouble: $("trouble0"), power: $("power0"),
}];
function addMachine(slot) {
  const root = $("ws-template").content.firstElementChild.cloneNode(true);
  root.dataset.slot = slot;
  root.querySelector(".ws-title").textContent = `ICON workstation ${slot} · node ${15 + slot}`;
  $("workstations").append(root);
  machines[slot] = {
    root, canvas: root.querySelector("canvas"), status: root.querySelector(".status"),
    pwr: root.querySelector(".pwr"), trouble: root.querySelector(".trouble"), power: root.querySelector(".power"),
  };
  setupMachine(slot);
}
function setupMachine(slot) {
  const m = machines[slot];
  m.ctx = m.canvas.getContext("2d");
  m.ctx.fillRect(0, 0, 640, 240);
  m.image = m.ctx.createImageData(640, 240);
  m.locks = {};
  m.root.addEventListener("mousedown", () => setFocus(slot));
  m.power.addEventListener("click", (e) => { e.stopPropagation(); power(slot); });
  m.canvas.addEventListener("click", () => { if (net && net.is_on(slot)) m.canvas.requestPointerLock(); });
}
function slotOf(el) { return machines.findIndex((m) => m && m.canvas === el); }

// ---- Focus: the clicked machine gets the keyboard
function setFocus(slot) {
  focus = slot;
  machines.forEach((m, i) => m.root.classList.toggle("focus", i === slot));
}

// ---- Keyboard: KeyboardEvent.code -> ICON key number (manual Table D-1)
const KEYS = {
  Escape: 0x01, Digit1: 0x02, Digit2: 0x03, Digit3: 0x04, Digit4: 0x05, Digit5: 0x06,
  Digit6: 0x07, Digit7: 0x08, Digit8: 0x09, Digit9: 0x0a, Digit0: 0x0b, Minus: 0x0c,
  Equal: 0x0d, Backquote: 0x0e, Backspace: 0x0f, Tab: 0x11,
  KeyQ: 0x12, KeyW: 0x13, KeyE: 0x14, KeyR: 0x15, KeyT: 0x16, KeyY: 0x17, KeyU: 0x18,
  KeyI: 0x19, KeyO: 0x1a, KeyP: 0x1b, BracketLeft: 0x1f, BracketRight: 0x20,
  ControlLeft: 0x21, ControlRight: 0x21,
  KeyA: 0x22, KeyS: 0x23, KeyD: 0x24, KeyF: 0x25, KeyG: 0x26, KeyH: 0x27, KeyJ: 0x28,
  KeyK: 0x29, KeyL: 0x2a, Semicolon: 0x2b, Quote: 0x2c, Enter: 0x2d,
  ShiftLeft: 0x30, KeyZ: 0x31, KeyX: 0x32, KeyC: 0x33, KeyV: 0x34, KeyB: 0x35,
  KeyN: 0x36, KeyM: 0x37, Comma: 0x38, Period: 0x39, Slash: 0x3a, ShiftRight: 0x3b,
  Insert: 0x3d /* HELP */, Pause: 0x3e, Space: 0x40, AltLeft: 0x41, AltRight: 0x41,
  Delete: 0x42 /* RUBOUT */, Backslash: 0x43,
  F11: 0x45 /* ACTION (left) */, F12: 0x52 /* ACTION (right) */,
  F1: 0x47, F2: 0x48, F3: 0x49, F4: 0x4a, F5: 0x4b, F6: 0x4c, F7: 0x4d, F8: 0x4e,
  F9: 0x4f, F10: 0x50, Home: 0x53 /* CLR */,
  NumpadDivide: 0x54, NumpadMultiply: 0x55, NumpadAdd: 0x56, Numpad7: 0x57, Numpad8: 0x58,
  Numpad9: 0x59, NumpadSubtract: 0x5a, Numpad4: 0x5b, Numpad5: 0x5c, Numpad6: 0x5d,
  NumpadComma: 0x5e, Numpad1: 0x5f, Numpad2: 0x60, Numpad3: 0x61, NumpadEnter: 0x62,
  Numpad0: 0x63, NumpadDecimal: 0x65,
};
const ARROWS = { ArrowUp: 0x5c, ArrowLeft: 0x5f, ArrowDown: 0x60, ArrowRight: 0x61 };
const LOCKS = { CapsLock: 0x3f, ScrollLock: 0x2f /* CURSOR LOCK */ };
const held = new Map(); // key -> slot it was pressed on

function send(slot, code) { if (net && net.is_on(slot)) net.key(slot, code); }

function keyEvent(e, down) {
  if (!net || e.metaKey || e.target.tagName === "INPUT") return;
  const slot = focus;
  if (e.code in LOCKS) {
    e.preventDefault();
    if (down && !e.repeat) {
      const k = LOCKS[e.code];
      const locks = machines[slot].locks;
      locks[k] = !locks[k];
      send(slot, locks[k] ? k : k | 0x80);
    }
    return;
  }
  if (e.code in ARROWS) {
    e.preventDefault();
    if (e.repeat) return;
    const k = ARROWS[e.code];
    if (down) { send(slot, 0x30); send(slot, k); held.set(k, slot); }
    else { send(held.get(k) ?? slot, k | 0x80); send(held.get(k) ?? slot, 0xb0); held.delete(k); }
    return;
  }
  const k = KEYS[e.code];
  if (k === undefined) return;
  e.preventDefault();
  if (e.repeat) return; // the ICON keyboard makes its own repeat ticks
  if (down) { send(slot, k); held.set(k, slot); }
  else { send(held.get(k) ?? slot, k | 0x80); held.delete(k); }
}
window.addEventListener("keydown", (e) => keyEvent(e, true));
window.addEventListener("keyup", (e) => keyEvent(e, false));
window.addEventListener("blur", () => { for (const [k, s] of held) send(s, k | 0x80); held.clear(); });
setInterval(() => { for (const s of new Set(held.values())) send(s, 0x7f); }, 50); // repeat ticks

// ---- Trackball: click a screen to capture the mouse (Esc releases it)
let tbAcc = [0, 0];
document.addEventListener("mousemove", (e) => {
  const locked = document.pointerLockElement;
  if (!net || !locked) return;
  const slot = slotOf(locked);
  if (slot < 0) return;
  tbAcc[0] += e.movementX; tbAcc[1] += e.movementY;
  const dx = Math.trunc(tbAcc[0]), dy = Math.trunc(tbAcc[1]);
  if (dx || dy) { net.trackball(slot, dx, dy); tbAcc = [tbAcc[0] - dx, tbAcc[1] - dy]; }
});
// While captured, the mouse buttons are the two ACTION keys.
document.addEventListener("mousedown", (e) => {
  const locked = document.pointerLockElement;
  if (!net || !locked) return;
  const slot = slotOf(locked);
  if (slot < 0) return;
  send(slot, e.button === 2 ? 0x52 : 0x45);
});
document.addEventListener("mouseup", (e) => {
  const locked = document.pointerLockElement;
  if (!net || !locked) return;
  const slot = slotOf(locked);
  if (slot < 0) return;
  send(slot, (e.button === 2 ? 0x52 : 0x45) | 0x80);
});
document.addEventListener("contextmenu", (e) => { if (document.pointerLockElement) e.preventDefault(); });

// ---- Sound
const RATE = 48000;
let audio = null, audioTime = 0;
function startAudio() {
  if (audio || !$("sound").checked) return;
  audio = new AudioContext({ sampleRate: RATE });
  audioTime = audio.currentTime + 0.05;
}
function pushAudio(samples) {
  if (!audio || !$("sound").checked || samples.length === 0) return;
  const buf = audio.createBuffer(1, samples.length, RATE);
  buf.copyToChannel(samples, 0);
  const src = audio.createBufferSource();
  src.buffer = buf;
  src.connect(audio.destination);
  const now = audio.currentTime;
  if (audioTime < now || audioTime > now + 0.3) audioTime = now + 0.05;
  src.start(audioTime);
  audioTime += buf.duration;
}
$("sound").addEventListener("change", () => { if ($("sound").checked) startAudio(); });

// ---- Hard disk persistence (IndexedDB, this browser only)
function idb() {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open("icon-emu", 1);
    req.onupgradeneeded = () => req.result.createObjectStore("disks");
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}
async function idbPut(key, value) {
  const db = await idb();
  await new Promise((res, rej) => {
    const tx = db.transaction("disks", "readwrite");
    tx.objectStore("disks").put(value, key);
    tx.oncomplete = res; tx.onerror = () => rej(tx.error);
  });
}
async function idbGet(key) {
  // Some browsers leave the open request pending (blocked storage), so give up after a while.
  const timeout = new Promise((_, rej) => setTimeout(() => rej(new Error("IndexedDB timed out")), 3000));
  const db = await Promise.race([idb(), timeout]);
  return new Promise((res, rej) => {
    const req = db.transaction("disks").objectStore("disks").get(key);
    req.onsuccess = () => res(req.result); req.onerror = () => rej(req.error);
  });
}
function showHd(note) {
  $("hd-name").textContent = hardDisk
    ? `${hardDisk.name} (${(hardDisk.data.length / 1048576).toFixed(1)} MB)${note ? " · " + note : ""}`
    : "no hard disk";
}
function syncHd() {
  if (net && net.is_on(FS) && hardDisk && net.take_hard_disk_dirty()) {
    hardDisk.data = net.hard_disk_image();
    return true;
  }
  return false;
}
async function saveHd() {
  if (syncHd()) {
    await idbPut("fileserver-hd", hardDisk);
    showHd("saved " + new Date().toLocaleTimeString());
  }
}
setInterval(() => { saveHd().catch(console.error); }, 5000);
document.addEventListener("visibilitychange", () => { if (document.hidden) saveHd(); });

$("hd-new").addEventListener("click", () => {
  if (hardDisk && !confirm("Replace the current hard disk with a blank 10 MB disk?")) return;
  hardDisk = { name: "lexicon-10mb.img", data: blank_hard_disk_10mb() };
  idbPut("fileserver-hd", hardDisk);
  showHd(net && net.is_on(FS) ? "attached at next power-on" : "");
});
// A factory-formatted 64 MB drive: blocks written by hdformat/fdisk/dinit on
// the emulated LEXICON, stored sparsely ("ICONHD64", size, then u32 block +
// 512 bytes per non-zero block). Built by scripts/prep-64mb.txt.
$("hd-new64").addEventListener("click", async () => {
  if (hardDisk && !confirm("Replace the current hard disk with a blank 64 MB disk?")) return;
  const res = await fetch("rom/hd64-factory.bin");
  if (!res.ok) { alert("hd64-factory.bin is missing: run scripts/prep-64mb.txt (see docs/INSTALL.md)"); return; }
  const t = new Uint8Array(await res.arrayBuffer());
  const v = new DataView(t.buffer);
  const data = new Uint8Array(v.getUint32(8, true));
  for (let o = 12; o + 516 <= t.length; o += 516) data.set(t.subarray(o + 4, o + 516), v.getUint32(o, true) * 512);
  hardDisk = { name: "lexicon-64mb.img", data };
  idbPut("fileserver-hd", hardDisk);
  showHd(net && net.is_on(FS) ? "attached at next power-on" : "");
});
$("hd-load").addEventListener("change", async (e) => {
  const file = e.target.files[0];
  if (!file) return;
  hardDisk = { name: file.name, data: new Uint8Array(await file.arrayBuffer()) };
  idbPut("fileserver-hd", hardDisk);
  showHd(net && net.is_on(FS) ? "attached at next power-on" : "");
  e.target.value = "";
});
$("hd-download").addEventListener("click", () => {
  syncHd();
  if (!hardDisk) return;
  const url = URL.createObjectURL(new Blob([hardDisk.data], { type: "application/octet-stream" }));
  const a = document.createElement("a");
  a.href = url;
  a.download = hardDisk.name;
  a.click();
  URL.revokeObjectURL(url);
});

// ---- Floppy
$("floppy").addEventListener("change", async (e) => {
  const file = e.target.files[0];
  if (!file) return;
  floppy = { name: file.name, data: new Uint8Array(await file.arrayBuffer()) };
  $("floppy-name").textContent = file.name;
  $("slot0").classList.add("loaded");
  if (net && net.is_on(FS)) {
    try { net.insert_floppy(floppy.data); } catch (err) { alert(err); }
  }
  e.target.value = "";
});
$("eject").addEventListener("click", () => {
  floppy = null;
  $("floppy-name").textContent = "no diskette";
  $("slot0").classList.remove("loaded");
  if (net && net.is_on(FS)) net.eject_floppy();
});

// ---- Power
function localTime() { return Date.now() / 1000 - new Date().getTimezoneOffset() * 60; }
function power(slot) {
  if (!net) { alert("Load the boot ROM first (top of the page)."); return; }
  startAudio();
  const m = machines[slot];
  if (net.is_on(slot)) {
    if (slot === FS && hardDisk) { syncHd(); idbPut("fileserver-hd", hardDisk); }
    net.power_off(slot);
    m.power.classList.remove("on");
    m.ctx.fillRect(0, 0, 640, 240);
    m.status.textContent = "off";
    return;
  }
  net.power_on(slot, localTime());
  if (slot === FS) {
    if (floppy) net.insert_floppy(floppy.data);
    if (hardDisk) net.attach_hard_disk(hardDisk.data);
  }
  m.power.classList.add("on");
  m.power.blur();
  setFocus(slot);
}

// ---- Workstations: add more nodes, or remove the last one while it is off
function workstations() { return machines.length - 1; }
function addWorkstation() {
  const slot = machines.length;
  addMachine(slot);
  if (net && net.slots() <= slot) net.add_workstation();
  local.set("icon-emu.workstations", String(workstations()));
}
$("ws-add").addEventListener("click", addWorkstation);
$("ws-remove").addEventListener("click", () => {
  const slot = machines.length - 1;
  if (slot <= 1) return;
  if (net && net.is_on(slot)) { alert(`Power off workstation ${slot} first.`); return; }
  if (net) net.remove_workstation();
  machines.pop().root.remove();
  if (focus === slot) setFocus(FS);
  local.set("icon-emu.workstations", String(workstations()));
});

// ---- Boot ROM: chosen by the viewer and kept in this browser (IndexedDB), so
// it never has to be published with the page. The 32 KB image is the two
// 16 KB EPROMs interleaved: 5053-1 holds the even bytes, 5053-2 the odd.
const STOCK_ROM_SHA1 = "5d7f7b173f599edf1f0e8655852c3adafa61fd89"; // 5053-1/-2 ISS 2
async function sha1(data) {
  const h = new Uint8Array(await crypto.subtle.digest("SHA-1", data));
  return Array.from(h, (b) => b.toString(16).padStart(2, "0")).join("");
}
async function showRom(note) {
  const bar = $("rom-bar");
  bar.classList.toggle("missing", !rom);
  $("rom-forget").hidden = !rom || rom.builtin;
  $("rom-note").textContent = note || "";
  if (!rom) { $("rom-name").textContent = "none loaded. Choose the ICON boot ROM to start."; return; }
  let kind = "";
  try { kind = (await sha1(rom.data)) === STOCK_ROM_SHA1 ? " · stock 5053 ISS 2" : " · custom"; } catch {}
  $("rom-name").textContent = `${rom.name}${kind}`;
}
function useRom(r, note) {
  rom = r;
  if (!net) {
    net = new Network(rom.data);
    while (net.slots() < machines.length) net.add_workstation();
  } else {
    net.set_rom(rom.data);
    if (machines.some((_, i) => net.is_on(i))) note = "takes effect at each machine's next power-on";
  }
  showRom(note);
}
$("rom-load").addEventListener("change", async (e) => {
  const files = [...e.target.files].sort((a, b) => a.name.localeCompare(b.name));
  e.target.value = "";
  if (files.length === 0) return;
  const parts = await Promise.all(files.map(async (f) => new Uint8Array(await f.arrayBuffer())));
  let data, name;
  if (parts.length === 1 && parts[0].length === 0x8000) {
    data = parts[0]; name = files[0].name;
  } else if (parts.length === 2 && parts.every((p) => p.length === 0x4000)) {
    // Sorted by name, so "...-1" / "part1" (even bytes) comes first.
    data = new Uint8Array(0x8000);
    for (let i = 0; i < 0x4000; i++) { data[2 * i] = parts[0][i]; data[2 * i + 1] = parts[1][i]; }
    name = `${files[0].name} + ${files[1].name}`;
  } else {
    alert("Expected one 32 KB ROM image, or the two 16 KB EPROM dumps (select both at once).");
    return;
  }
  try {
    useRom({ name, data });
    await idbPut("rom", { name, data });
  } catch (err) { alert(err); }
});
$("rom-forget").addEventListener("click", async () => {
  try { await idbPut("rom", null); } catch {}
  showRom("forgotten: it will be asked for again next time");
});

// ---- Main loop
let lastPackets = 0;
function frame() {
  requestAnimationFrame(frame);
  if (!net) return;
  net.run_frame();
  pushAudio(net.audio(RATE));
  machines.forEach((m, slot) => {
    const on = net.is_on(slot);
    m.pwr.classList.toggle("on", on);
    if (!on) return;
    m.image.data.set(net.screen(slot));
    m.ctx.putImageData(m.image, 0, 0);
    const l = net.lights(slot);
    m.trouble.classList.toggle("on", (l & 1) !== 0);
    if (slot === FS) {
      $("hd0").classList.toggle("on", (l & 2) !== 0);
      $("fd0").classList.toggle("on", (l & 4) !== 0);
    }
    m.status.textContent = net.status(slot);
  });
  const p = net.packets();
  $("cable").classList.toggle("busy", p !== lastPackets);
  $("packets").textContent = `${p} packets`;
  lastPackets = p;
}

setupMachine(FS);
const wanted = Math.max(1, Math.min(8, parseInt(local.get("icon-emu.workstations") || "1", 10) || 1));
for (let slot = 1; slot <= wanted; slot++) addMachine(slot);
setFocus(FS);

await init();
// The ROM comes from browser storage, else from a local build's rom/ folder
// (`make web` copies it there; it is never published).
let saved = null;
try { saved = await idbGet("rom"); } catch (e) { console.warn("IndexedDB unavailable", e); }
if (saved && saved.data) {
  useRom(saved, "restored from browser storage");
} else {
  try {
    const res = await fetch("rom/icon1_rom_combined.bin");
    if (res.ok) {
      const data = new Uint8Array(await res.arrayBuffer());
      if (data.length === 0x8000) useRom({ name: "rom/icon1_rom_combined.bin", data, builtin: true }, "from this build's rom/ folder");
    }
  } catch {}
  if (!rom) showRom();
}
try {
  const savedHd = await idbGet("fileserver-hd");
  if (savedHd) { hardDisk = savedHd; showHd("restored from browser storage"); }
} catch (e) { console.warn("IndexedDB unavailable", e); }
requestAnimationFrame(frame);
