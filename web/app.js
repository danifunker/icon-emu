import init, { Emu } from "./pkg/icon_web.js";

const canvas = document.getElementById("screen");
const ctx = canvas.getContext("2d");
const image = ctx.createImageData(640, 240);
const powerBtn = document.getElementById("power");
const led = document.getElementById("led");
const status = document.getElementById("status");

let rom = null;
let emu = null;

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
  powerBtn.classList.add("on");
  requestAnimationFrame(frame);
});

await init();
rom = new Uint8Array(await (await fetch("rom/icon1_rom_combined.bin")).arrayBuffer());
ctx.fillRect(0, 0, 640, 240);
status.textContent = "ready: press Power";
