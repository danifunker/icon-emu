import init, { Emu, blank_hard_disk_10mb } from "./pkg/icon_web.js";

const canvas = document.getElementById("screen");
const ctx = canvas.getContext("2d");
const image = ctx.createImageData(640, 240);
const powerBtn = document.getElementById("power");
const led = document.getElementById("led");
const hdLed = document.getElementById("hd-led");
const fdLed = document.getElementById("fd-led");
const status = document.getElementById("status");
const soundBox = document.getElementById("sound");

let rom = null;
let emu = null;
let floppy = null; // { name, data }
let hardDisk = null; // { name, data } - kept across power cycles

// ---- Hard disk persistence (IndexedDB, this browser only)
const hdName = document.getElementById("hd-name");
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
  const db = await idb();
  return new Promise((res, rej) => {
    const req = db.transaction("disks").objectStore("disks").get(key);
    req.onsuccess = () => res(req.result); req.onerror = () => rej(req.error);
  });
}
function showHd(note) {
  hdName.textContent = hardDisk
    ? `${hardDisk.name} (${(hardDisk.data.length / 1048576).toFixed(1)} MB)${note ? " · " + note : ""}`
    : "no hard disk";
}
async function saveHd() {
  if (emu && emu.take_hard_disk_dirty()) hardDisk.data = emu.hard_disk_image();
  if (hardDisk) {
    await idbPut("fileserver-hd", hardDisk);
    showHd("saved " + new Date().toLocaleTimeString());
  }
}
setInterval(() => { saveHd().catch(console.error); }, 5000);
document.addEventListener("visibilitychange", () => { if (document.hidden) saveHd(); });

document.getElementById("hd-new").addEventListener("click", () => {
  if (hardDisk && !confirm("Replace the current hard disk with a blank 10 MB disk?")) return;
  hardDisk = { name: "lexicon-10mb.img", data: blank_hard_disk_10mb() };
  idbPut("fileserver-hd", hardDisk);
  showHd(emu ? "attached at next power-on" : "");
});
document.getElementById("hd-load").addEventListener("change", async (e) => {
  const file = e.target.files[0];
  if (!file) return;
  hardDisk = { name: file.name, data: new Uint8Array(await file.arrayBuffer()) };
  idbPut("fileserver-hd", hardDisk);
  showHd(emu ? "attached at next power-on" : "");
  e.target.value = "";
});
document.getElementById("hd-download").addEventListener("click", () => {
  if (emu && emu.take_hard_disk_dirty()) hardDisk.data = emu.hard_disk_image();
  if (!hardDisk) return;
  const url = URL.createObjectURL(new Blob([hardDisk.data], { type: "application/octet-stream" }));
  const a = document.createElement("a");
  a.href = url;
  a.download = hardDisk.name;
  a.click();
  URL.revokeObjectURL(url);
});

// ---- Keyboard: browser KeyboardEvent.code -> ICON key number (manual Table D-1).
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
// Arrow keys: the keypad cursor keys with Shift (the CURSOR LOCK column).
const ARROWS = { ArrowUp: 0x5c, ArrowLeft: 0x5f, ArrowDown: 0x60, ArrowRight: 0x61 };
// Lock keys report their state: make = locked, break = unlocked.
const LOCKS = { CapsLock: 0x3f, ScrollLock: 0x2f /* CURSOR LOCK */ };
const lockState = { 0x3f: false, 0x2f: false };
const held = new Set();

function keyEvent(e, down) {
  if (!emu || e.metaKey) return; // leave browser shortcuts alone
  const code = e.code;
  if (code in LOCKS) {
    if (down && !e.repeat) {
      const k = LOCKS[code];
      lockState[k] = !lockState[k];
      emu.key(lockState[k] ? k : k | 0x80);
    }
    e.preventDefault();
    return;
  }
  if (code in ARROWS) {
    e.preventDefault();
    if (e.repeat) return;
    const k = ARROWS[code];
    if (down) { emu.key(0x30); emu.key(k); held.add(k); }
    else { emu.key(k | 0x80); emu.key(0x30 | 0x80); held.delete(k); }
    return;
  }
  const k = KEYS[code];
  if (k === undefined) return;
  e.preventDefault();
  if (e.repeat) return; // the ICON keyboard makes its own repeat ticks
  if (down) { emu.key(k); held.add(k); }
  else { emu.key(k | 0x80); held.delete(k); }
}
window.addEventListener("keydown", (e) => keyEvent(e, true));
window.addEventListener("keyup", (e) => keyEvent(e, false));
window.addEventListener("blur", () => {
  if (emu) for (const k of held) emu.key(k | 0x80);
  held.clear();
});
// While a key is held the keyboard sends 7Fh repeat ticks (~20 per second).
setInterval(() => { if (emu && held.size) emu.key(0x7f); }, 50);

// ---- Sound: the speaker (PPI A PC3) rendered by the core, played via Web Audio.
const RATE = 48000;
let audio = null;
let audioTime = 0;
function startAudio() {
  if (audio || !soundBox.checked) return;
  audio = new AudioContext({ sampleRate: RATE });
  audioTime = audio.currentTime + 0.05;
}
function pushAudio(samples) {
  if (!audio || !soundBox.checked || samples.length === 0) return;
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
soundBox.addEventListener("change", () => { if (soundBox.checked) startAudio(); });

// ---- Main loop
function frame() {
  if (!emu) return;
  emu.run_frame();
  pushAudio(emu.audio(RATE));
  image.data.set(emu.screen());
  ctx.putImageData(image, 0, 0);
  led.classList.toggle("on", emu.led());
  const act = emu.disk_activity();
  hdLed.classList.toggle("on", (act & 1) !== 0);
  fdLed.classList.toggle("on", (act & 2) !== 0);
  status.textContent = emu.status();
  requestAnimationFrame(frame);
}

document.getElementById("floppy").addEventListener("change", async (e) => {
  const file = e.target.files[0];
  if (!file) return;
  floppy = { name: file.name, data: new Uint8Array(await file.arrayBuffer()) };
  document.getElementById("floppy-name").textContent = file.name;
  if (emu) emu.insert_floppy(floppy.data);
  e.target.value = "";
});

powerBtn.addEventListener("click", () => {
  startAudio();
  if (emu) {
    if (hardDisk) { hardDisk.data = emu.hard_disk_image(); saveHd(); }
    emu.free();
    emu = null;
    held.clear();
    powerBtn.classList.remove("on");
    ctx.fillRect(0, 0, 640, 240);
    status.textContent = "off";
    for (const l of [led, hdLed, fdLed]) l.classList.remove("on");
    return;
  }
  emu = new Emu(rom, "fs", Date.now() / 1000 - new Date().getTimezoneOffset() * 60);
  if (floppy) emu.insert_floppy(floppy.data);
  if (hardDisk) emu.attach_hard_disk(hardDisk.data);
  powerBtn.classList.add("on");
  powerBtn.blur(); // keep Space/Enter going to the emulator
  requestAnimationFrame(frame);
});

await init();
try {
  const saved = await idbGet("fileserver-hd");
  if (saved) { hardDisk = saved; showHd("restored from browser storage"); }
} catch (e) { console.warn("IndexedDB unavailable", e); }
rom = new Uint8Array(await (await fetch("rom/icon1_rom_combined.bin")).arrayBuffer());
ctx.fillRect(0, 0, 640, 240);
status.textContent = "ready: load the fileserver boot diskette, then press Power";
