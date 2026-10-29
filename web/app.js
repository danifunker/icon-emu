import init, {
  Network, blank_hard_disk_10mb, blank_floppy_640k,
  disk_volumes, disk_view, disk_read, disk_put, disk_mkdir, disk_remove, disk_count, disk_copy, qnx_name, normalize_hard_disk,
  text_open, text_save,
} from "./pkg/icon_web.js";

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

// ---- Machine kinds. Each kind has its own boot ROM (unless one ROM is shared).
// `node: true` kinds can be added as workstations; ICON2 and ICON3 go here once
// they are emulated.
const KINDS = {
  lexicon: { label: "LEXICON", title: "LEXICON fileserver" },
  icon: { label: "ICON", title: "ICON workstation", node: true },
};

// ---- Machines: slot 0 is the fileserver, built into the page; workstations
// are cloned from a template. Each entry holds that machine's elements.
const machines = [{
  kind: "lexicon", root: $("m0"), canvas: $("screen0"), status: $("status0"), pwr: $("pwr0"),
  trouble: $("trouble0"), power: $("power0"),
}];
function addMachine(slot, kind) {
  const root = $("ws-template").content.firstElementChild.cloneNode(true);
  root.dataset.slot = slot;
  root.querySelector(".ws-title").textContent = `${KINDS[kind].title} ${slot} · node ${15 + slot}`;
  $("workstations").append(root);
  machines[slot] = {
    kind, root, canvas: root.querySelector("canvas"), status: root.querySelector(".status"),
    pwr: root.querySelector(".pwr"), trouble: root.querySelector(".trouble"), power: root.querySelector(".power"),
    tap: root.querySelector(".tap"), drop: root.querySelector(".drop"),
  };
  setupMachine(slot);
}
function setupMachine(slot) {
  const m = machines[slot];
  m.ctx = m.canvas.getContext("2d");
  m.ctx.fillRect(0, 0, 640, 240);
  m.image = m.ctx.createImageData(640, 240);
  m.locks = {};
  m.activity = 0;
  m.root.addEventListener("mousedown", () => setFocus(slot));
  m.power.addEventListener("click", (e) => { e.stopPropagation(); power(slot); });
  // Capture notices sit over the screen.
  m.notice = document.createElement("div");
  m.notice.className = "notice";
  m.notice.hidden = true;
  m.canvas.parentElement.append(m.notice);
  m.canvas.addEventListener("click", () => capture(slot));
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

function releaseAll() { for (const [k, s] of held) send(s, k | 0x80); held.clear(); }

function keyEvent(e, down) {
  if (!net || e.metaKey || e.target.tagName === "INPUT") return;
  // Ctrl+Alt releases a captured keyboard and mouse.
  if (down && document.pointerLockElement && e.ctrlKey && e.altKey) {
    e.preventDefault();
    releaseAll();
    document.exitPointerLock();
    return;
  }
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
window.addEventListener("blur", releaseAll);
setInterval(() => { for (const s of new Set(held.values())) send(s, 0x7f); }, 50); // repeat ticks

// ---- Capture: clicking a running machine's screen captures the mouse (as its
// trackball) and gives it the keyboard. The first time, a notice explains this
// before anything is captured; afterwards a short reminder shows instead.
// Ctrl+Alt releases; browsers also always release pointer lock on Esc.
const RELEASE_HINT = "Press <b>Ctrl+Alt</b> to release (Esc also works).";
function showNotice(m, html, buttons = [], ms = 0) {
  clearTimeout(m.noticeTimer);
  m.notice.innerHTML = html;
  m.notice.classList.toggle("toast", buttons.length === 0);
  for (const [label, action, secondary] of buttons) {
    const b = document.createElement("button");
    b.textContent = label;
    if (secondary) b.className = "secondary";
    b.addEventListener("click", (e) => { e.stopPropagation(); action(); });
    m.notice.append(b);
  }
  m.notice.hidden = false;
  if (ms) m.noticeTimer = setTimeout(() => { m.notice.hidden = true; }, ms);
}
function capture(slot) {
  const m = machines[slot];
  if (!net || !net.is_on(slot) || document.pointerLockElement === m.canvas) return;
  setFocus(slot);
  if (local.get("icon-emu.capture-explained")) { m.canvas.requestPointerLock(); return; }
  showNotice(m,
    `<div><b>Capture keyboard and mouse?</b></div>
     <div>Your keystrokes go to this machine, the mouse moves its trackball, and the mouse buttons are its two ACTION keys.</div>
     <div>${RELEASE_HINT}</div>`,
    [["Capture", () => {
      local.set("icon-emu.capture-explained", "1");
      m.notice.hidden = true;
      m.canvas.requestPointerLock();
    }], ["Not now", () => { m.notice.hidden = true; }, true]]);
}
document.addEventListener("pointerlockchange", () => {
  const locked = document.pointerLockElement;
  machines.forEach((m) => {
    const on = m.canvas === locked;
    m.root.classList.toggle("captured", on);
    if (on) showNotice(m, `Keyboard and mouse captured. ${RELEASE_HINT}`, [], 3000);
    else if (m.notice.classList.contains("toast")) m.notice.hidden = true;
  });
  if (!locked) releaseAll();
});

// ---- Trackball (while captured)
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
  frefresh();
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
  frefresh();
});
$("hd-load").addEventListener("change", async (e) => {
  const file = e.target.files[0];
  if (!file) return;
  hardDisk = { name: file.name, data: new Uint8Array(await file.arrayBuffer()) };
  idbPut("fileserver-hd", hardDisk);
  showHd(net && net.is_on(FS) ? "attached at next power-on" : "");
  frefresh();
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
  syncFloppy();
  if (floppy) lastFloppy = floppy;
  insertFloppy({ name: file.name, data: new Uint8Array(await file.arrayBuffer()) });
  e.target.value = "";
});
// What QNX wrote to the diskette lives in the emulator; copy it back first.
function syncFloppy() {
  if (floppy && net && net.is_on(FS)) {
    const data = net.floppy_image();
    if (data.length) floppy.data = data;
  }
}
let lastFloppy = null; // the diskette most recently ejected, still downloadable
function insertFloppy(f) {
  floppy = f;
  $("floppy-name").textContent = f.name;
  $("slot0").classList.add("loaded");
  if (net && net.is_on(FS)) {
    try { net.insert_floppy(f.data); } catch (err) { alert(err); }
  }
  frefresh();
}
$("floppy-new").addEventListener("click", () => {
  syncFloppy();
  if (floppy) lastFloppy = floppy;
  insertFloppy({ name: "blank-640k.img", data: blank_floppy_640k() });
});
$("floppy-download").addEventListener("click", () => {
  syncFloppy();
  const f = floppy || lastFloppy;
  if (!f) { alert("No diskette to download."); return; }
  const url = URL.createObjectURL(new Blob([f.data], { type: "application/octet-stream" }));
  const a = document.createElement("a");
  a.href = url;
  a.download = f.name;
  a.click();
  URL.revokeObjectURL(url);
});
$("eject").addEventListener("click", () => {
  syncFloppy();
  if (floppy) lastFloppy = floppy;
  floppy = null;
  $("floppy-name").textContent = "no diskette";
  $("slot0").classList.remove("loaded");
  if (net && net.is_on(FS)) net.eject_floppy();
  frefresh();
});

// ---- Power
function localTime() { return Date.now() / 1000 - new Date().getTimezoneOffset() * 60; }
function power(slot) {
  if (!net) return;
  startAudio();
  const m = machines[slot];
  if (net.is_on(slot)) {
    if (slot === FS) syncFloppy();
    if (slot === FS && hardDisk) { syncHd(); idbPut("fileserver-hd", hardDisk); }
    net.power_off(slot);
    filesPowerChanged();
    m.power.classList.remove("on");
    m.ctx.fillRect(0, 0, 640, 240);
    m.status.textContent = "off";
    return;
  }
  try {
    net.power_on(slot, localTime());
  } catch {
    alert(`Load a boot ROM for the ${KINDS[m.kind].label} first (top of the page).`);
    return;
  }
  if (slot === FS) {
    if (floppy) net.insert_floppy(floppy.data);
    if (hardDisk) net.attach_hard_disk(hardDisk.data);
  }
  m.power.classList.add("on");
  m.power.blur();
  filesPowerChanged();
  setFocus(slot);
}

// ---- Nodes: add workstations of any emulated kind, or remove the last one while it is off
function saveNodes() { local.set("icon-emu.nodes", JSON.stringify(machines.slice(1).map((m) => m.kind))); }
function addNode(kind) {
  if (!net) return;
  const slot = machines.length;
  if (net.slots() <= slot) net.add_node(kind);
  addMachine(slot, kind);
  saveNodes();
  layoutBus();
}
for (const [kind, k] of Object.entries(KINDS)) {
  if (!k.node) continue;
  const b = document.createElement("button");
  b.textContent = `Add ${k.label} node`;
  b.addEventListener("click", () => addNode(kind));
  $("node-remove").before(b);
}
$("node-remove").addEventListener("click", () => {
  const slot = machines.length - 1;
  if (!net || slot <= 1) return;
  if (net.is_on(slot)) { alert(`Power off node ${15 + slot} first.`); return; }
  net.remove_last_node();
  machines.pop().root.remove();
  if (focus === slot) setFocus(FS);
  saveNodes();
  layoutBus();
});

// The bus trunk runs down to the last row of workstations.
function layoutBus() {
  const last = machines[machines.length - 1].root;
  $("bus").style.setProperty("--trunk", `${Math.max(6, last.offsetTop - 16)}px`);
}
window.addEventListener("resize", layoutBus);

// ---- Boot ROMs: chosen by the viewer and kept in this browser (IndexedDB), so
// they never have to be published with the page. One ROM can serve every
// machine kind ("Use the same ROM for all devices"), or each kind gets its own.
// The 32 KB image is the two 16 KB EPROMs interleaved: 5053-1 holds the even
// bytes, 5053-2 the odd.
// Known dumps, by SHA-1 of the 32 KB interleaved image.
const KNOWN_ROMS = {
  "5d7f7b173f599edf1f0e8655852c3adafa61fd89": "stock 5053 ISS 2",
  "3750cf6f203fe1a167b853e36d30114701b039eb": "Rel 2.11 (200-503/504-000) · workstations need system software 4.10.05",
};
const roms = { shared: null }; // { name, data, builtin? } per kind, plus "shared"
let sameRom = local.get("icon-emu.same-rom") !== "0";
const romRows = { shared: $("rom-shared") };
for (const [kind, k] of Object.entries(KINDS)) {
  const row = $("rom-row-template").content.firstElementChild.cloneNode(true);
  row.querySelector(".label").textContent = `${k.label} ROM`;
  row.hidden = sameRom;
  $("rom-bar").append(row);
  romRows[kind] = row;
}
$("rom-bar").append($("ram-row"));
async function sha1(data) {
  const h = new Uint8Array(await crypto.subtle.digest("SHA-1", data));
  return Array.from(h, (b) => b.toString(16).padStart(2, "0")).join("");
}
function effectiveRom(kind) { return sameRom ? roms.shared : roms[kind] || roms.shared; }
async function describe(r) {
  let tag = "";
  try { tag = " · " + (KNOWN_ROMS[await sha1(r.data)] || "custom"); } catch {}
  return r.name + tag;
}
async function showRoms(note) {
  $("rom-same").checked = sameRom;
  $("rom-note").textContent = note || "";
  const missing = Object.keys(KINDS).some((k) => !effectiveRom(k));
  $("rom-bar").classList.toggle("missing", missing);
  for (const [key, row] of Object.entries(romRows)) {
    row.hidden = (key === "shared") !== sameRom;
    const r = roms[key];
    row.querySelector(".forget").hidden = !r || r.builtin;
    row.querySelector(".name").textContent = r ? await describe(r)
      : key === "shared" ? (sameRom ? "none loaded. Choose the ICON boot ROM to start." : "none")
      : roms.shared ? `none of its own: using ${roms.shared.name}` : "none loaded";
  }
  showRam();
}

// ---- Memory: 512 KB, or the CEMCORP 1 MB upgrade card, which only the
// Rel 2.11 ROM drives (it programs the card's port and the chip selects).
const REL_2_11 = "3750cf6f203fe1a167b853e36d30114701b039eb";
let ramKb = local.get("icon-emu.ram-kb") === "1024" ? 1024 : 512;
async function showRam() {
  $("ram").value = String(ramKb);
  const note = $("ram-note");
  let other = [];
  if (ramKb === 1024) {
    for (const [kind, k] of Object.entries(KINDS)) {
      const r = effectiveRom(kind);
      if (r && (await sha1(r.data)) !== REL_2_11) other.push(k.label);
    }
  }
  note.classList.toggle("warn", other.length > 0);
  note.textContent = other.length
    ? `1 MB needs the Rel 2.11 boot ROM: the ${other.join(" and ")} ROM can't use the card and won't boot.`
    : "1 MB needs the Rel 2.11 boot ROM; the 5053 ISS 2 ROM can't use the card.";
  if (ramKb === 1024 && !other.length) note.textContent += " Takes effect at each machine's next power-on.";
}
$("ram").addEventListener("change", (e) => {
  ramKb = e.target.value === "1024" ? 1024 : 512;
  local.set("icon-emu.ram-kb", String(ramKb));
  if (net) net.set_ram_kb(ramKb);
  showRam();
});
// Hand the current ROMs to the emulator; running machines keep theirs until power-off.
function applyRoms() {
  let running = false;
  for (const kind of Object.keys(KINDS)) {
    const r = effectiveRom(kind);
    if (r) net.set_rom(kind, r.data);
    running ||= machines.some((m, i) => m.kind === kind && net.is_on(i));
  }
  return running ? "takes effect at each machine's next power-on" : "";
}
async function readRomFiles(fileList) {
  const files = [...fileList].sort((a, b) => a.name.localeCompare(b.name));
  if (files.length === 0) return null;
  const parts = await Promise.all(files.map(async (f) => new Uint8Array(await f.arrayBuffer())));
  if (parts.length === 1 && parts[0].length === 0x8000) return { name: files[0].name, data: parts[0] };
  if (parts.length === 2 && parts.every((p) => p.length === 0x4000)) {
    // Sorted by name, so "...-1" / "part1" (even bytes) comes first.
    const data = new Uint8Array(0x8000);
    for (let i = 0; i < 0x4000; i++) { data[2 * i] = parts[0][i]; data[2 * i + 1] = parts[1][i]; }
    return { name: `${files[0].name} + ${files[1].name}`, data };
  }
  alert("Expected one 32 KB ROM image, or the two 16 KB EPROM dumps (select both at once).");
  return null;
}
for (const [key, row] of Object.entries(romRows)) {
  row.querySelector(".load").addEventListener("change", async (e) => {
    const r = await readRomFiles(e.target.files);
    e.target.value = "";
    if (!r) return;
    roms[key] = r;
    try { await idbPut("rom:" + key, r); } catch {}
    showRoms(applyRoms());
  });
  row.querySelector(".forget").addEventListener("click", async () => {
    roms[key] = null;
    try { await idbPut("rom:" + key, null); } catch {}
    showRoms("forgotten: it will be asked for again next time");
  });
}
$("rom-same").addEventListener("change", (e) => {
  sameRom = e.target.checked;
  local.set("icon-emu.same-rom", sameRom ? "1" : "0");
  showRoms(applyRoms());
});

// ---- Files: copy files between this computer and the ICON's disks (the
// qnx2fs crate, inside the emulator). The diskette can be changed at any time:
// take it out, edit it, put it back, as when swapping diskettes. The hard disk
// only while every machine is off: the fileserver caches it, and a running
// workstation may be mid-way through using it.
const fl = { src: null, path: "/", edit: false, open: null, sel: null, entries: null }; // open: the file in the editor; sel: the selected entry
// Standalone: disk images opened from this computer, worked on without any
// machine. Each is { id, name, data, dirty }. They live only in this page.
const images = [];
let nextImageId = 1;
const skey = (s) => `${s.kind}:${s.id || 0}:${s.offset}`;
const fstatus = (msg, err = false) => { $("files-status").textContent = msg; $("files-status").classList.toggle("err", err); };
const enote = (msg, err = false) => { $("ed-note").textContent = msg; $("ed-note").classList.toggle("err", err); };
const anyOn = () => !!net && machines.some((m, i) => net.is_on(i));
function hdImage() {
  syncHd();
  if (!hardDisk.normalized) { hardDisk.data = normalize_hard_disk(hardDisk.data); hardDisk.normalized = true; }
  return hardDisk.data;
}
const imageOf = (src) => images.find((i) => i.id === src.id);
function fimage(src) {
  if (src.kind === "fd") { syncFloppy(); return floppy.data; }
  if (src.kind === "img") return imageOf(src).data;
  return hdImage();
}
function fsources() {
  const out = [];
  if (floppy) for (const v of JSON.parse(disk_volumes(floppy.data))) out.push({ kind: "fd", offset: v.offset, label: "Diskette" });
  if (hardDisk) for (const v of JSON.parse(disk_volumes(hdImage()))) out.push({ kind: "hd", offset: v.offset, label: `Hard disk, ${v.label}` });
  for (const img of images) {
    const vols = JSON.parse(disk_volumes(img.data));
    for (const v of vols) {
      const label = `${img.name}${v.label === "whole disk" ? "" : ", " + v.label}${img.dirty ? " (changed)" : ""}`;
      out.push({ kind: "img", id: img.id, offset: v.offset, label });
    }
    // Keep an image with no QNX files in the menu, so it can be closed.
    if (!vols.length) out.push({ kind: "img", id: img.id, offset: -1, label: `${img.name} (no ICON files)` });
  }
  return out;
}
// Why the selected disk can't be changed right now, or "" if it can.
function readOnlyReason(src = fl.src) {
  if (src && src.kind === "hd" && anyOn()) return "The hard disk can only be changed while every machine is off.";
  return "";
}
// Apply a change to the selected disk: `edit(image) -> new image`.
function fedit(edit) { fedit_on(fl.src, edit); }
// The same for any disk in the menu.
function fedit_on(src, edit) {
  const why = readOnlyReason(src);
  if (why) throw new Error(why);
  const data = edit(fimage(src));
  if (src.kind === "img") {
    const img = imageOf(src);
    img.data = data;
    img.dirty = true;
  } else if (src.kind === "fd") {
    floppy.data = data;
    if (net && net.is_on(FS)) net.insert_floppy(data);
  } else {
    hardDisk.data = data;
    idbPut("fileserver-hd", hardDisk);
    showHd("changed in Files");
  }
}
const fjoin = (dir, name) => (dir === "/" ? "/" : dir + "/") + name;
function fbutton(label, onclick) {
  const b = document.createElement("button");
  b.textContent = label;
  b.addEventListener("click", onclick);
  return b;
}
// Icons for the file manager (inline SVG, so they need no files).
const ICONS = {
  folder: '<svg class="fm-ico" viewBox="0 0 20 20"><path d="M2 5.5A1.5 1.5 0 0 1 3.5 4h4l1.6 1.8h7.4A1.5 1.5 0 0 1 18 7.3v7.7a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 2 15z" fill="#e8b84a" stroke="#b8862a"/><path d="M2 8h16" stroke="#c99a35"/></svg>',
  folderOpen: '<svg class="fm-ico" viewBox="0 0 20 20"><path d="M2 5.5A1.5 1.5 0 0 1 3.5 4h4l1.6 1.8h7A1.5 1.5 0 0 1 17.6 7.3V9H5.2L2 16z" fill="#d9a63c" stroke="#b8862a"/><path d="M5 9h14l-3 7.5H2z" fill="#f2c95f" stroke="#b8862a" stroke-linejoin="round"/></svg>',
  text: '<svg class="fm-ico" viewBox="0 0 20 20"><path d="M4.5 2h7.5l4 4v11.5a.5.5 0 0 1-.5.5h-11a.5.5 0 0 1-.5-.5v-15a.5.5 0 0 1 .5-.5z" fill="#fff" stroke="#7d8a9c"/><path d="M12 2v4h4" fill="#e6eaf0" stroke="#7d8a9c"/><path d="M6.5 9h7M6.5 11.5h7M6.5 14h5" stroke="#4f7fd1" stroke-width="1.2"/></svg>',
  file: '<svg class="fm-ico" viewBox="0 0 20 20"><path d="M4.5 2h7.5l4 4v11.5a.5.5 0 0 1-.5.5h-11a.5.5 0 0 1-.5-.5v-15a.5.5 0 0 1 .5-.5z" fill="#eef1f5" stroke="#7d8a9c"/><path d="M12 2v4h4" fill="#dfe4eb" stroke="#7d8a9c"/><rect x="6.5" y="9" width="7" height="5.5" rx="1" fill="#9aa6b6"/></svg>',
  floppy: '<svg class="fm-ico" viewBox="0 0 20 20"><rect x="2.5" y="2.5" width="15" height="15" rx="1.5" fill="#3d4f6b" stroke="#24324a"/><rect x="5.5" y="2.5" width="9" height="5.5" fill="#cfd6e0"/><rect x="11.5" y="3.5" width="2" height="3.5" fill="#3d4f6b"/><rect x="5" y="11" width="10" height="6.5" fill="#f4f6f9"/></svg>',
  hd: '<svg class="fm-ico" viewBox="0 0 20 20"><rect x="2" y="5" width="16" height="10" rx="2" fill="#aeb8c6" stroke="#5d6878"/><circle cx="14.5" cy="12" r="1.2" fill="#3fbf4f"/><path d="M4.5 12h6" stroke="#5d6878" stroke-width="1.2"/></svg>',
};
const fl_expanded = new Set();  // folders opened in the tree
const fl_collapsed = new Set(); // folders closed in the tree, even on the way to the open folder
function icon(kind) {
  const span = document.createElement("span");
  span.className = "fm-item";
  span.innerHTML = ICONS[kind];
  return span;
}
function fselect(name) {
  fl.sel = name;
  for (const tr of $("files-table").rows) tr.classList.toggle("sel", tr.dataset.name === name);
  const e = fl.entries && fl.entries.find((x) => x.name === name);
  $("files-download").disabled = !e || e.dir;
  $("files-copy").disabled = !e;
}
function fgo(path) {
  // Show the folder you go to: open it and every folder above it.
  for (let p = path; p !== "/" && p; p = p.replace(/\/[^/]*$/, "") || "/") fl_collapsed.delete(p);
  fl.path = path;
  fl.sel = null;
  frefresh();
}
function frefresh() {
  if (!$("files-panel").open) return;
  const srcs = fsources();
  const sel = $("files-src");
  const key = skey;
  const keep = fl.src && srcs.find((s) => key(s) === key(fl.src));
  if (!keep) { fl.path = "/"; fl_expanded.clear(); fl_collapsed.clear(); }
  fl.src = keep || srcs[0] || null;
  sel.replaceChildren(...srcs.map((s) => new Option(s.label, key(s), false, fl.src && key(s) === key(fl.src))));
  sel.hidden = srcs.length === 0;
  $("files-edit").setAttribute("aria-pressed", String(fl.edit));
  $("files-edit").textContent = fl.edit ? "Done editing" : "Edit";
  const table = $("files-table");
  const tree = $("files-tree");
  table.replaceChildren();
  tree.replaceChildren();
  $("files-path").replaceChildren();
  $("files-empty").hidden = true;
  $("files-download").disabled = true;
  $("files-copy").disabled = true;
  $("files-up").disabled = !fl.src || fl.path === "/";
  $("files-save-img").disabled = $("files-close-img").disabled = !fl.src || fl.src.kind !== "img";
  if (fl.src && fl.src.offset < 0) {
    fl.entries = null;
    fstatus("This image has no ICON (QNX 2) filesystem: it may be blank, or another computer's disk.", true);
    return;
  }
  if (!fl.src) {
    fl.entries = null;
    fstatus(floppy || hardDisk
      ? "No ICON (QNX) files found. A diskette must be formatted for the ICON: New blank gives you one."
      : "Insert a diskette (New blank gives an empty one), add a hard disk, or use Open image… to work on a disk image from your computer.");
    return;
  }
  let view;
  try { view = JSON.parse(disk_view(fimage(fl.src), fl.src.offset, fl.path, JSON.stringify([...fl_expanded]))); }
  catch (e) { fl.path = "/"; fl.entries = null; fstatus(e.message, true); return; }
  fl.entries = view.entries;

  // Breadcrumbs: disk / folder / folder, each a link.
  const crumbs = $("files-path");
  const parts = fl.path.split("/").filter(Boolean);
  const link = (label, path) => {
    const a = document.createElement("a");
    a.textContent = label;
    a.addEventListener("click", () => fgo(path));
    return a;
  };
  crumbs.append(link(fl.src.kind === "fd" ? "diskette" : fl.src.kind === "img" ? imageOf(fl.src).name : "hard disk", "/"));
  parts.forEach((p, i) => {
    const sep = document.createElement("span");
    sep.className = "sep";
    sep.textContent = "/";
    crumbs.append(sep, link(p, "/" + parts.slice(0, i + 1).join("/")));
  });

  // Folder tree: the disk at the top, folders below, ▸ / ▾ to open and close.
  const addNode = (path, name, depth, deeper = true) => {
    const row = document.createElement("div");
    row.className = "fm-node" + (path === fl.path ? " current" : "");
    row.style.paddingLeft = `${4 + depth * 14}px`;
    const subs = view.tree[path];
    const open = path === "/" || (!fl_collapsed.has(path) && (fl_expanded.has(path) || fl.path === path || fl.path.startsWith(path + "/")));
    // A folder with no folders inside gets no ▸.
    const leaf = path !== "/" && (subs ? subs.length === 0 : !deeper);
    const twisty = document.createElement("span");
    twisty.className = "fm-twisty";
    twisty.textContent = leaf ? "" : open ? "▾" : "▸";
    twisty.title = leaf ? "" : open ? "Collapse" : "Expand";
    twisty.addEventListener("click", (ev) => {
      ev.stopPropagation();
      if (leaf) { fgo(path); return; }
      if (open && path !== "/") {
        fl_expanded.delete(path);
        fl_collapsed.add(path);
        if (fl.path.startsWith(path + "/")) fl.path = path; // don't hide the open folder
      } else {
        fl_collapsed.delete(path);
        fl_expanded.add(path);
      }
      frefresh();
    });
    const diskIcon = fl.src.kind === "fd" || (fl.src.kind === "img" && imageOf(fl.src).data.length <= 819200) ? "floppy" : "hd";
    const label = icon(path === "/" ? diskIcon : open ? "folderOpen" : "folder");
    label.append(name);
    row.append(twisty, label);
    row.addEventListener("click", () => fgo(path));
    tree.append(row);
    if (open && subs) for (const [n, d] of subs) addNode(path === "/" ? "/" + n : path + "/" + n, n, depth + 1, d);
  };
  addNode("/", fl.src.label, 0);

  // The list.
  const kb = (n) => `${Math.round(n / 1024)} KB`;
  const why = readOnlyReason();
  fstatus(`${view.entries.length} item${view.entries.length === 1 ? "" : "s"} · ${kb(view.free)} free of ${kb(view.total)}` + (why ? ` · read-only: ${why}` : ""));
  $("files-empty").hidden = view.entries.length > 0;
  for (const e of view.entries) {
    const tr = table.insertRow();
    tr.dataset.name = e.name;
    const name = tr.insertCell();
    const item = icon(e.dir ? "folder" : e.text ? "text" : "file");
    item.append(e.name);
    name.append(item);
    name.title = e.attrs;
    const size = tr.insertCell();
    size.className = "num";
    size.textContent = e.dir ? "" : e.size.toLocaleString();
    const date = tr.insertCell();
    date.className = "fm-date";
    date.textContent = e.date;
    const act = tr.insertCell();
    act.className = "fm-act";
    // The root bitmap is QNX's free-space map: never offered for editing or deleting.
    const system = e.name === "bitmap" && fl.path === "/";
    if (fl.edit && !system) {
      if (!e.dir && e.text) act.append(fbutton("Edit", () => fopen(e.name)));
      act.append(" ", fbutton("Delete", () => fdelete(e)));
    }
    tr.addEventListener("click", () => fselect(e.name));
    tr.addEventListener("dblclick", () => {
      if (e.dir) { fl_expanded.add(fl.path); fgo(fjoin(fl.path, e.name)); }
      else fdownload(e.name);
    });
  }
  if (fl.sel) fselect(fl.sel);
}
function fdownload(name) {
  try {
    const data = disk_read(fimage(fl.src), fl.src.offset, fjoin(fl.path, name), $("files-text").checked);
    const url = URL.createObjectURL(new Blob([data], { type: "application/octet-stream" }));
    const a = document.createElement("a");
    a.href = url;
    a.download = name;
    a.click();
    URL.revokeObjectURL(url);
  } catch (e) { fstatus(e.message, true); }
}
function fdelete(e) {
  const path = fjoin(fl.path, e.name);
  let what = path;
  if (e.dir) {
    try {
      const n = JSON.parse(disk_count(fimage(fl.src), fl.src.offset, path));
      const parts = [];
      if (n.files) parts.push(`${n.files} file${n.files === 1 ? "" : "s"}`);
      if (n.folders) parts.push(`${n.folders} folder${n.folders === 1 ? "" : "s"}`);
      what = `the folder ${path}` + (parts.length ? ` and the ${parts.join(" and ")} in it` : " (empty)");
    } catch (err) { fstatus(err.message, true); return; }
  }
  if (!confirm(`Delete ${what}? This can't be undone.`)) return;
  try {
    fedit((img) => disk_remove(img, fl.src.offset, path, localTime()));
    if (fl.open && (fl.open.path === path || fl.open.path.startsWith(path + "/"))) fclose(true);
    frefresh();
  } catch (err) { fstatus(err.message, true); }
}

// The text editor: the file is decoded in memory (ICON line ends, Telidon
// accents, code page 437, underline codes) and saved back in the same style.
const NEWLINE_NAMES = { rs: "ICON line ends (RS)", lf: "LF line ends", crlf: "CR LF line ends" };
const ACCENT_NAMES = { telidon: "ICON accents (Telidon)", cp437: "IBM PC accents (code page 437)" };
function fopen(name) {
  if (fl.open && fl.open.dirty && !confirm(`Discard your changes to ${fl.open.path}?`)) return;
  const path = fjoin(fl.path, name);
  let t;
  try { t = JSON.parse(text_open(fimage(fl.src), fl.src.offset, path)); }
  catch (e) { fstatus(e.message, true); return; }
  if (t.binary) { fstatus(`${name} isn't a text file (${t.size.toLocaleString()} bytes); use Download instead.`, true); return; }
  fl.open = { src: fl.src, path, dirty: false };
  $("ed-name").textContent = path;
  $("ed-text").value = t.text;
  $("ed-newline").value = t.newline;
  $("ed-accents").value = t.accents;
  $("files-editor").hidden = false;
  const why = readOnlyReason(fl.src);
  $("ed-text").readOnly = !!why;
  $("ed-save").disabled = !!why;
  let note = `${t.size.toLocaleString()} bytes · ${NEWLINE_NAMES[t.newline]} · ${ACCENT_NAMES[t.accents]}.`;
  if (!t.exact) note += " Careful: this file mixes conventions (for example two kinds of line end), so saving it will change bytes you didn't edit.";
  if (why) note += " Read-only: " + why;
  enote(note, !t.exact);
  $("ed-text").focus();
}
// A machine went on or off: the hard disk may have become read-only, or writable.
function filesPowerChanged() {
  frefresh();
  if (!fl.open) return;
  const why = readOnlyReason(fl.open.src);
  $("ed-text").readOnly = !!why;
  $("ed-save").disabled = !!why;
  if (why) enote("Read-only now: " + why + " Your changes stay here until you close the editor.", true);
}
function fclose(force = false) {
  if (!force && fl.open && fl.open.dirty && !confirm(`Discard your changes to ${fl.open.path}?`)) return;
  fl.open = null;
  $("files-editor").hidden = true;
}
$("ed-text").addEventListener("input", () => { if (fl.open) fl.open.dirty = true; });
$("ed-close").addEventListener("click", () => fclose());
$("ed-underline").addEventListener("click", () => {
  const ta = $("ed-text");
  if (ta.readOnly) return;
  const [a, b] = [ta.selectionStart, ta.selectionEnd];
  ta.setRangeText("⟦" + ta.value.slice(a, b) + "⟧", a, b, "end");
  if (fl.open) fl.open.dirty = true;
  ta.focus();
});
$("ed-save").addEventListener("click", () => {
  const o = fl.open;
  if (!o) return;
  const prev = fl.src;
  try {
    fl.src = o.src;
    fedit((img) => text_save(img, o.src.offset, o.path, $("ed-text").value, $("ed-newline").value, $("ed-accents").value, localTime()));
    o.dirty = false;
    enote(`Saved ${o.path} at ${new Date().toLocaleTimeString()}.`);
  } catch (e) {
    enote(e.message, true);
  } finally {
    fl.src = prev;
  }
  frefresh();
});
window.addEventListener("beforeunload", (e) => { if ((fl.open && fl.open.dirty) || images.some((i) => i.dirty)) e.preventDefault(); });

// Standalone images: open, download, close.
$("files-open").addEventListener("change", async (e) => {
  const file = e.target.files[0];
  e.target.value = "";
  if (!file) return;
  const img = { id: nextImageId++, name: file.name, data: normalize_hard_disk(new Uint8Array(await file.arrayBuffer())), dirty: false };
  images.push(img);
  const first = fsources().find((s) => s.kind === "img" && s.id === img.id);
  fl.src = first;
  fl.path = "/";
  $("files-panel").open = true;
  frefresh();
  if (first && first.offset >= 0) fstatus(`Opened ${img.name}. Changes stay in this page until you use Download image.`);
});
$("files-save-img").addEventListener("click", () => {
  const img = fl.src && fl.src.kind === "img" && imageOf(fl.src);
  if (!img) return;
  const url = URL.createObjectURL(new Blob([img.data], { type: "application/octet-stream" }));
  const a = document.createElement("a");
  a.href = url;
  a.download = img.name;
  a.click();
  URL.revokeObjectURL(url);
  img.dirty = false;
  frefresh();
});
$("files-close-img").addEventListener("click", () => {
  const img = fl.src && fl.src.kind === "img" && imageOf(fl.src);
  if (!img) return;
  if (img.dirty && !confirm(`${img.name} has changes you haven't downloaded. Close it anyway?`)) return;
  if (fl.open && fl.open.src.kind === "img" && fl.open.src.id === img.id) fclose(true);
  images.splice(images.indexOf(img), 1);
  fl.src = null;
  frefresh();
});

$("files-panel").addEventListener("toggle", frefresh);
$("files-refresh").addEventListener("click", frefresh);
$("files-edit").addEventListener("click", () => { fl.edit = !fl.edit; frefresh(); });
$("files-src").addEventListener("change", (e) => {
  fl.src = fsources().find((s) => skey(s) === e.target.value) || null;
  fl.path = "/";
  frefresh();
});
$("files-up").addEventListener("click", () => fgo(fl.path.replace(/\/[^/]*$/, "") || "/"));
$("files-download").addEventListener("click", () => { if (fl.sel) fdownload(fl.sel); });

// Copy to…: the selected entry into a folder on any disk in the menu,
// keeping permissions, owners and dates (the qnx2fs crate does the copy).
let copying = null; // { src, path, name, dir }
$("files-copy").addEventListener("click", () => {
  const e = fl.entries && fl.entries.find((x) => x.name === fl.sel);
  if (!e) return;
  copying = { src: fl.src, path: fjoin(fl.path, e.name), name: e.name, dir: e.dir };
  $("cp-name").textContent = copying.path + (e.dir ? "/" : "");
  const targets = fsources().filter((s) => s.offset >= 0);
  $("cp-disk").replaceChildren(...targets.map((s) => new Option(s.label, skey(s), false, skey(s) === skey(fl.src))));
  $("cp-dir").value = fl.path;
  $("cp-replace").checked = false;
  $("files-copybar").hidden = false;
  $("cp-dir").focus();
});
$("cp-disk").addEventListener("change", () => {
  if (copying) $("cp-dir").value = $("cp-disk").value === skey(copying.src) ? fl.path : "/";
});
$("cp-cancel").addEventListener("click", () => { copying = null; $("files-copybar").hidden = true; });
$("cp-go").addEventListener("click", () => {
  if (!copying) return;
  const dst = fsources().find((s) => skey(s) === $("cp-disk").value);
  if (!dst) { fstatus("That disk isn't there any more.", true); return; }
  const dir = "/" + $("cp-dir").value.split("/").filter(Boolean).join("/");
  try {
    const srcImage = fimage(copying.src);
    let what = copying.name;
    if (copying.dir) {
      const n = JSON.parse(disk_count(srcImage, copying.src.offset, copying.path));
      what = `${copying.name} (${n.files} file${n.files === 1 ? "" : "s"}, ${n.folders} folder${n.folders === 1 ? "" : "s"} inside)`;
    }
    fedit_on(dst, (img) => disk_copy(srcImage, copying.src.offset, copying.path, img, dst.offset, dir, $("cp-replace").checked, localTime()));
    $("files-copybar").hidden = true;
    copying = null;
    frefresh();
    fstatus(`Copied ${what} to ${dst.label}: ${dir}, with its permissions and dates.`);
  } catch (err) {
    fstatus(err.message.startsWith("already exists") ? `${err.message}. Tick "replace files that exist" to overwrite.` : err.message, true);
  }
});
$("files-mkdir").addEventListener("click", () => {
  if (!fl.src) return;
  const why = readOnlyReason();
  if (why) { fstatus(why, true); return; }
  const asked = prompt("Name of the new folder:");
  if (!asked) return;
  const name = qnx_name(asked);
  try {
    fedit((img) => disk_mkdir(img, fl.src.offset, fjoin(fl.path, name), localTime()));
    frefresh();
    if (name !== asked) fstatus(`Created ${name} (names must fit QNX).`);
  } catch (e) { fstatus(e.message, true); }
});
$("files-upload").addEventListener("change", async (e) => {
  const list = [...e.target.files];
  e.target.value = "";
  if (!fl.src || !list.length) return;
  const why = readOnlyReason();
  if (why) { fstatus(why, true); return; }
  const text = $("files-text").checked;
  const done = [], renamed = [];
  try {
    for (const f of list) {
      const name = qnx_name(f.name);
      const path = fjoin(fl.path, name);
      const data = new Uint8Array(await f.arrayBuffer());
      try {
        fedit((img) => disk_put(img, fl.src.offset, path, data, text, false, localTime()));
      } catch (err) {
        if (!err.message.startsWith("already exists") || !confirm(`${path} already exists. Replace it?`)) throw err;
        fedit((img) => disk_put(img, fl.src.offset, path, data, text, true, localTime()));
      }
      done.push(name);
      if (name !== f.name) renamed.push(`${f.name} → ${name}`);
    }
    frefresh();
    fstatus(`Copied ${done.length} file${done.length === 1 ? "" : "s"}.` + (renamed.length ? ` Renamed to fit QNX: ${renamed.join(", ")}.` : ""));
  } catch (err) {
    frefresh();
    fstatus((done.length ? `Copied ${done.join(", ")}; then: ` : "") + err.message, true);
  }
});

// ---- Type text: one line at a time into the focused machine, each followed
// by Enter, waiting until the previous line has been typed and has had a
// moment to run (a long paste would otherwise overrun QNX's type-ahead).
let typing = null; // { slot, lines, next, waitUntil }
$("type-go").addEventListener("click", () => {
  const lines = $("type-text").value.replace(/\r/g, "").split("\n").filter((l) => l.trim() !== "");
  if (!net || !net.is_on(focus)) { alert("Power on and click the machine to type into first."); return; }
  if (lines.length) typing = { slot: focus, lines, next: 0, waitUntil: 0 };
});
$("type-stop").addEventListener("click", () => { typing = null; $("type-status").textContent = "stopped"; });
function pumpTyping() {
  if (!typing) return;
  const t = typing;
  if (!net.is_on(t.slot)) { typing = null; $("type-status").textContent = "machine is off"; return; }
  if (net.keys_pending(t.slot) > 0 || performance.now() < t.waitUntil) return;
  if (t.next >= t.lines.length) { typing = null; $("type-status").textContent = `typed ${t.lines.length} line(s)`; return; }
  net.type_text(t.slot, t.lines[t.next++] + "\r");
  t.waitUntil = performance.now() + 400;
  $("type-status").textContent = `typing line ${t.next} of ${t.lines.length}…`;
}

// ---- Main loop
function frame() {
  requestAnimationFrame(frame);
  if (!net) return;
  net.run_frame();
  pumpTyping();
  pushAudio(net.audio(RATE));
  machines.forEach((m, slot) => {
    const on = net.is_on(slot);
    m.pwr.classList.toggle("on", on);
    // Coax activity: this node sent or received a packet since the last frame.
    const a = net.activity(slot);
    const busy = a !== m.activity;
    m.activity = a;
    if (m.tap) { m.tap.classList.toggle("busy", busy); m.drop.classList.toggle("busy", busy); }
    else $("cable").classList.toggle("busy", busy);
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
  $("packets").textContent = `${net.packets()} packets`;
}

setupMachine(FS);
// Nodes from last time (per browser); the old workstation count still works.
let kinds = null;
try { kinds = JSON.parse(local.get("icon-emu.nodes")); } catch {}
if (!Array.isArray(kinds)) kinds = Array(parseInt(local.get("icon-emu.workstations") || "1", 10) || 1).fill("icon");
kinds = kinds.filter((k) => KINDS[k]?.node).slice(0, 8);
if (kinds.length === 0) kinds = ["icon"];
kinds.forEach((kind, i) => addMachine(i + 1, kind));
setFocus(FS);
layoutBus();

// Build stamp: the last commit's date and hash, written by scripts/web-version.sh.
fetch("version.json", { cache: "no-store" })
  .then((r) => (r.ok ? r.json() : null))
  .then((v) => {
    if (!v?.commit) return;
    const a = document.createElement("a");
    a.href = `https://github.com/danifunker/icon-emu/commit/${v.commit}`;
    a.textContent = v.commit;
    $("build").append(`build ${v.date} · `, a, v.dirty ? " + local changes" : "");
  })
  .catch(() => {});

await init();
net = new Network();
while (net.slots() < machines.length) net.add_node(machines[net.slots()].kind);
// ROMs come from browser storage; a shared one can also come from a local
// build's rom/ folder (`make web` copies it there; it is never published).
for (const key of ["shared", ...Object.keys(KINDS)]) {
  try { roms[key] = (await idbGet("rom:" + key)) || null; } catch (e) { console.warn("IndexedDB unavailable", e); break; }
}
if (!roms.shared) {
  try { const old = await idbGet("rom"); if (old?.data) { roms.shared = old; idbPut("rom:shared", old); } } catch {}
}
let note = roms.shared ? "restored from browser storage" : "";
if (!roms.shared) {
  try {
    const res = await fetch("rom/icon1_rom_combined.bin");
    if (res.ok) {
      const data = new Uint8Array(await res.arrayBuffer());
      if (data.length === 0x8000) { roms.shared = { name: "rom/icon1_rom_combined.bin", data, builtin: true }; note = "from this build's rom/ folder"; }
    }
  } catch {}
}
net.set_ram_kb(ramKb);
applyRoms();
showRoms(note);
try {
  const savedHd = await idbGet("fileserver-hd");
  if (savedHd) { hardDisk = savedHd; showHd("restored from browser storage"); }
} catch (e) { console.warn("IndexedDB unavailable", e); }
requestAnimationFrame(frame);
