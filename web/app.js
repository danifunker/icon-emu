import init, { Emu } from "./pkg/icon_web.js";

const canvas = document.getElementById("screen");
const ctx = canvas.getContext("2d");
const image = ctx.createImageData(640, 240);
const powerBtn = document.getElementById("power");
const led = document.getElementById("led");
const status = document.getElementById("status");

let rom = null;
let emu = null;
let floppy = null; // { name, data }

document.getElementById("floppy").addEventListener("change", async (e) => {
  const file = e.target.files[0];
  if (!file) return;
  floppy = { name: file.name, data: new Uint8Array(await file.arrayBuffer()) };
  document.getElementById("floppy-name").textContent = file.name;
  if (emu) emu.insert_floppy(floppy.data);
});

function frame() {
  if (!emu) return;
  emu.run_frame();
  image.data.set(emu.screen());
  ctx.putImageData(image, 0, 0);
  led.classList.toggle("on", emu.led());
  status.textContent = emu.status();
  requestAnimationFrame(frame);
}

powerBtn.addEventListener("click", () => {
  if (emu) {
    emu.free();
    emu = null;
    powerBtn.classList.remove("on");
    ctx.fillRect(0, 0, 640, 240);
    status.textContent = "off";
    led.classList.remove("on");
    return;
  }
  emu = new Emu(rom, "fs");
  if (floppy) emu.insert_floppy(floppy.data);
  powerBtn.classList.add("on");
  requestAnimationFrame(frame);
});

await init();
rom = new Uint8Array(await (await fetch("rom/icon1_rom_combined.bin")).arrayBuffer());
ctx.fillRect(0, 0, 640, 240);
status.textContent = "ready: press Power";
