# ICON / LEXICON fileserver: NVRAM, RTC, disk hardware and installer (RE notes for the emulator)

Scope: what a Rust/WASM emulator needs in order to POST the 5053 ROM, boot the 4.x fileserver kernels, and run the 4.10.01 install. Every claim carries a file and offset. Tags: **CONFIRMED** means read directly from code or data. **INFERRED** means my interpretation.

Closes or advances: FW-4 (NVRAM map), HW-4 (`machine_test` codes), FS-7 and FS-8 (hard-disk layout), NET-1 (reply layout, partly), NET-3 (`net.config`), HW-5 (`hw_dbase`, header only), FW-6 (trackball counters, partly), and HW-1 (the ICON2 CPU class).

---

## 0. How the evidence was obtained (reproducible)

Paths below are relative to `scratchpad/`. Work files are in `scratchpad/re/` (`bin/` holds the copied binaries, `x/` holds the split, decompressed and disassembled outputs, `tools/` holds the scripts).

* **QNX 2 load-file format (CONFIRMED, every `/cmds`, `/sys` and `/util` binary):**
  * 14-byte header: `u8 01`, `u8 flags` (`0x80` = large model, multi-record), `u16 code_size`, `u16 data_size (incl. bss)`, `u16 init_data_end`, `u16 0`, `u16 stack (0x3E8)`, `u16 0`.
  * Then records `02 <seg> <u16 offset> <u16 len> <bytes…>`. Seg 0 = code, seg 1 = data, seg 8 = a second code segment (seen in `configure` and `install_software`). A 4-byte `03 01 <u16>` record can come first.
  * The data segment's first 0x80 bytes are the OS-filled prefix (`/lib/magic.h`). `[DS:0x65]` = `My_nid` (INFERRED from use).
  * Splitter: `re/tools/qload.py`. Annotator: `re/tools/annot.py`, which names `int 5Ch/70h/72h` stubs and resolves string immediates.
* **System calls:** `int 70h` and `int 72h` are QNX library calls. **`int 5Ch` is the "machine services" interface implemented in the kernel.** All NVRAM, RTC and machine-type access by user programs goes through `int 5Ch`. No user program touches the NVRAM ports directly.
* **Kernel `/boot/hd_boot` (4.10.01, MD5 fd57feab…, identical on SSW disk 2 and the home-brew boot disk):**
  * 0x40-byte header, then a 0x140-byte copy-and-decompress stub at file 0x40.
  * The body is **0x9D-RLE compressed from file 0x180**: `9D 00` = literal 9D, `9D 01` = end, `9D n` = n zero bytes. Decompress it with `tools/unrle.py bin/hd_boot 0x180` (gives 170 784 bytes).
  * The stub loads the image at **0070:0000** and enters at **009C:0000**.
  * The "task admin" code runs with **CS = 009C**, so image offset = 0x2C0 + IP. All `CS:IP` addresses below are in segment 009C of this image (`re/x/t9c.asm`).
  * `/boot/fd_boot` (4.01, 169 424 bytes) has the same format, **uncompressed** (the image starts at file 0x40).
* **ROM:** `firmware/ICON1/icon1_rom_combined.bin`, CS = F88F, file offset = IP + 0x8F0 (`re/x/rom.asm`).

---

## 1. NVRAM (93C06-class Microwire EEPROM, 16 × 16 bits)

### 1.1 Wire protocol (CONFIRMED: kernel `009C:67D8` read and `009C:66CF` write; ROM `F88F:27BA` read and `F88F:2837` write)

| Signal | Port / bit | How it is driven |
|---|---|---|
| Device select | **PPI-A port B (F983) bits 7..5** | NVRAM is selected with PB7..5 = `100` (`in; and 1Fh; or 80h; out`). It is released by setting PB5 (`or 20h`). |
| SK (clock) | PPI-A **PC1** | Bit set/reset through the control port F987: `out F987,03` = SK high, `out F987,02` = SK low |
| DI (to EEPROM) | PPI-A **PC0** | `out F987,01` / `out F987,00` |
| DO (from EEPROM) | PPI-A **PC6**, read on **F985** | `in al,F985 ; and al,40h` |

* **Decoder (INFERRED from all users of PB7..5):** PB5 is an active-low global enable and PB7..6 pick the device.
  * `00` = trackball counter X (selected with `and 1Fh; or 00h`), `01` = trackball counter Y (`or 40h`). Kernel `009C:6B78`, a timer-2 ISR, reads `+0x80` and `+0x82` of the sysblock.
  * `10` = NVRAM.
  * `11` = baud-rate counter (ROM `F88F:0E0C` selects `or C0h` for the baud test).
  * Emulator rule: **NVRAM CS = (PB7=1 ∧ PB6=0 ∧ PB5=0)**.
* **Frame format:**
  * The host clocks one 0 bit, then the **start bit 1**, then 8 bits MSB-first from `bh`: opcode (2 bits) + 6-bit address (A5 and A4 are always 0).
  * **READ** = `bh = 80h|a`. Then 16 data bits, MSB first: each loop iteration pulses SK and then samples PC6 (`009C:67E0`–`6800`). So DO must present a dummy 0 after A0 and then D15..D0 on successive SK pulses.
  * **WRITE** sequence (`009C:66CF`):
    * `EWEN` (`bh=30h`), release PB5, delay `loop 4000h`;
    * `ERASE` (`bh=C0h|a`), release, delay;
    * `WRITE` (`bh=40h|a`) + 16 data bits, release.
  * The host never polls DO for ready and never sends EWDS. An emulator may complete erase and write instantly.
* **Write-key interlock (CONFIRMED, `009C:66DF`):** the kernel writes only if sysblock `+0x56 == 0xADB0`, and it clears the key after each write. User programs set the key with `int 5Ch` fn **0x49(0xADB0)** before every write fn. Examples: `set_boot_type` code `0x16D`; `node_info` code `0x24C`.

```
009C:67D8 nv_read(al=word)            ; called by every NVRAM reader
  mov bh,80h / or bh,al / call cmd     ; cmd: PB7..5=100, DI=0,clk, DI=1,clk, 8 bits of bh
  mov cx,10h
L: call clk ; rol di,1 ; in al,F985 ; and al,40h ; jz → di&=FFFE else di|=1 ; loop L
  in al,F983 ; or al,20h ; out F983,al  ; deselect
```

### 1.2 Who reads and writes which word

There are `int 5Ch` handlers in `hd_boot`. The dispatcher is at `009C:5C94`: for `ax` = fn, it does `call [cs:5CB9 + 2*(fn-0Ah)]`, with fn valid from 0Ah to 6Dh.

| fn | Handler | Action (CONFIRMED) | Callers seen |
|---|---|---|---|
| 2C | 66B1 | write word *n* (n ≤ 15), needs key | — |
| 2D | 67CD | read word *n* (no range check) | nv_config, machine_test, kernel init |
| 33 | 685A | `w12 & 0x000F` (**boot-file selector**) | nv_config, set_boot_type |
| 34 | 6863 | `w10 & 0xFF` (**boot / cluster server node**) | cluster_id, nv_config, node_info |
| 35 | 686C | `w10 >> 8` (**own node ID**) | nv_config |
| 36 | 6876 | `w12 = (w12 & FFF0) \| n` | set_boot_type |
| 37 | 68CF | `w10 hi = n` | node_info (spawned by attach_net / configure) |
| 38 | 68A0 | `w10 lo = n` | node_info |
| 39 | 68FE | `w0 >> 12` (loader-disable nibble) | nv_config |
| 3A | 690A | `w0 = (w0 & 0FFF) \| n<<12`, **rejects n > 15** | node_info passes F9h/FCh/F8h/FEh, so it is a **no-op on ICON** (those values are evidently meant for Netcard CMOS) |
| 3B | 6938 | `(w3<<16) \| w4` (software serial no.) | nv_config |
| 3C | 6951 | `(w1<<16) \| w2` (hardware serial no.) | get_serial_no (+h → w1, +l → w2), nv_config, node_info |
| 3E / 3F | 696B / 6972 | read / write w0 | set_boot_type, nv_config |
| 40 / 41 | 6985 / 6991 | `(w13>>2)&3` = monitor type; set it | nv_config |
| 42 / 43 | 69BF / 69D3 | `8 >> ((w13>>6)&3)`; set (1→C0h, 2→80h) | none found (unknown option) |
| 44 / 45 | 6A09 / 6A1F | read w11; write w11 = both bytes `(b<<4)\|a` | — |
| 48 | 6A57 | **always returns 0** on ICON | check_machine, machine_test |
| 4D | 6A5B | `(w0>>2)&1` | nv_config, set_boot_type |
| 4A / 4B | 60F1 / 6226 | RTC read / set (§2), returns −1 if `w0 bit5` = 1 | read_rtc, set_rtc |

At boot, kernel init (`009C:4F6A`–`5690`) copies words into the sysblock (far pointer at 0000:0180):
* `+0x48 = w10`, `+0x6C = w11`, `+0x6E = w13 lo`, `+0x6F = w13 hi` (`009C:51E9`–`5222`);
* memory top from `w0 & 0x803` (`5052`);
* monitor class from w13 and w8 (`57D9`–`5838`).

### 1.3 The 16-word map

| Word | Bits | Meaning | Status / evidence |
|---|---|---|---|
| **0** | 0–1 | RAM: with bit 11 = 1: 3 = 128 KB, 0 = 256 KB, 1 = 384 KB, 2 = 512 KB. With bit 11 = 0 and bits = 1: 1 MB board | CONFIRMED. ROM `F88F:090C` tests (bits+1) extra banks, none if 3. nv_config switch table data 0x421: {0x803→128, 0x800→256, 0x801→384, 0x802→512, 0x001→1024}. Kernel sets the `machine_test +m` flags 10h/20h/30h/40h/80h (`009C:5857`) |
| | 2 | **0 = fileserver board with WD1002-05** (the ROM floppy loader goes through the WD1002, and the ROM tests the WD1002). **1 = workstation** (on-board 1797) | CONFIRMED: ROM `F88F:11AE`, nv_config `0x4B6` ("Fileserver" if bit 2 = 0), set_boot_type `0x1DE` |
| | 3 | 1 = no on-board 1797 floppy (ROM skips test 46–4A); nv_config prints "Local diskette drive" = ¬bit 3 | CONFIRMED: ROM `F88F:11B9`; nv_config `0x5E1` |
| | 4 | 1 = no serial / baud-rate hardware (ROM skips tests 2B/2C and 38–3F); nv_config "Serial port" = ¬bit 4 | CONFIRMED: ROM `F88F:0E01`, `F88F:10C7`; nv_config `0x5EC` |
| | 5 | 1 = **no real-time clock** (fn 4A/4B return −1, and read_rtc prints "Real time clock not installed") | CONFIRMED: kernel `009C:60F8`, `622D` |
| | 6 | Selects a variant of the ROM baud-rate counter test | CONFIRMED use (ROM `F88F:0E11`); meaning unknown |
| | 7 | 1 = no keyboard (the ROM tolerates a missing keyboard interrupt); nv_config "Keyboard" = ¬bit 7 on fileservers | CONFIRMED: ROM `F88F:0949`; nv_config `0x5F7` |
| | 8 | 1 = no TMS5220 speech chip (ROM skips tests 4B–4E) | CONFIRMED: ROM `F88F:129E` |
| | 9–10 | not referenced by anything examined | — |
| | 11 | 0 = 1 MB memory board (see bits 0–1) | CONFIRMED (see above) |
| | 12–15 | Loader disables: 12 = network, 13 = floppy, 14 = hard disk, 15 = ROM slot (1 = disabled) | CONFIRMED: ROM (ICON_firmware §4); nv_config `0x11D2` prints the order floppy → server → hard disk |
| **1–2** | | Hardware serial number, 32-bit `(w1<<16)\|w2`. Bits 31–28 = a, 27–24 = b, 23–20 = year−80, 19–16 = month, 14–10 = day, 9–0 = sequence. Printed as `%d%02d-%02d%02d%02d-%03d` (b, a, yy, mm, dd, seq) | CONFIRMED: nv_config `0x1CD`–`0x266` |
| **3–4** | | Software serial number, same packing | CONFIRMED: nv_config `0x270` |
| **5–9** | | CRTC R0–R9. Low byte = even register | CONFIRMED: ROM `F88F:08B2` |
| | w8 hi | R7 (vsync position). 3Dh = "ICON Colour" monitor, otherwise "Roland CC-141" | CONFIRMED: nv_config `0x8DC`, kernel `009C:57F4` |
| **10** | | hi = own node ID, lo = boot / cluster server node | CONFIRMED (fn 34/35/37/38). The kernel, `machine_test +n` and `+f` compare hi with lo |
| **11** | | Four nibbles to F900/F902/F904/F906 (palette or attribute map). The ROM writes nibbles low first. The kernel `009C:5D93` swaps the order when PPI-A PB2 is set | CONFIRMED |
| **12** | 0–3 | Boot-file selector for the netboot request (`/config/boot_config` type): 1 = ws_boot, 2 = ws_fsys_boot, 0x10 / 0x11 = ICON2 variants (set_boot_type) | CONFIRMED: set_boot_type `0x112`–`0x14D` |
| **13** | 2–3 | Monitor: 1 = Colour RGB, 2 = composite colour (ICON or Roland per w8), 3 = mono | CONFIRMED: nv_config `0x8BF`–`0x940` |
| | 0–3 | If equal to 0xE (and bits 2–3 = 3): "ICON Monochrome", otherwise "Roland MB-121A" | CONFIRMED |
| | 6–7 | fn 42/43: value v → 8>>v. Purpose unknown | CONFIRMED accessor |
| | lo / hi byte | Copied to sysblock +6E / +6F | CONFIRMED |
| **14–15** | | Never referenced (factory value FFFF) | CONFIRMED: no accessor in the int 5Ch table |

**There is no checksum or signature.**
* check_machine's "NVRAM malfunction" path runs only for machine types 3–5 (ICON2 and Netcard). Its switch table at data 0x10B maps type 1 → exit 0.
* On ICON, fn 48 always returns 0 (`009C:6A57 mov ax,0 / ret`). CONFIRMED.

### 1.4 What the ROM needs (errors 11–13)

* **Error 11:** at ROM `F88F:0864`, if **all 16 words read FFFF** → "Missing or failed NVRAM". Any non-FFFF word passes.
* **Error 12:** raised only in **factory mode**. That mode is entered when PPI-A port A (F981) bits 0–4 read 0 (`F88F:077B`). In it the ROM writes 0, n·0101h and FFFF to all words and verifies. Normal boots must return **non-zero straps (e.g. 1Fh)**.
  * After the test, the ROM waits (blinking PC3) until the straps become non-zero, then writes defaults (`F88F:07E8`–`0855`): w0 = two reads of port FAA0 (INFERRED: a test fixture), w5..9 = **2838 442C 0041 3D3C 0300**, w10 = FE01 if bit 2 of that read is set (else 0101), w11 = F4F4, w12 = 0001. w1–4 and w13–15 stay FFFF.
* **Error 13 ("no sync"):** at `F88F:08DD`, after loading the CRTC from w5–w9, the ROM polls **PPI-A port C bit 7 (F985 bit 7)**. It needs **two transitions** within a 0x2000-iteration loop each (about 30–40 ms at 8 MHz). The same check, run with the ROM's built-in CRTC values, is **error 0F** (`F88F:06AF`).
  * INFERRED: PC7 is the video sync (CRTC VSYNC through the gate array, "GA13").
  * Emulator: toggle PC7 at the frame rate whenever R0–R9 are sane. The defaults give 66 rows × 4 lines = 264 lines, about 60 Hz at 15.7 kHz.

### 1.5 Known-good images (16 big-endian words, listed w0 … w15)

**(a) ICON fileserver, node 1, boots floppy then hard disk, 512 KB, RTC present, no keyboard, no speech chip:**

* w0 = 9000 (floppy and HD enabled, network and ROM disabled) | 0800 | 0100 (no speech) | 0080 (no keyboard) | 0008 (no 1797) | 0002 (512 KB) = **998A**
  * With a keyboard attached, use 990A. For a 1 MB board, use **9189**.
* w1/w2 = 5173 3C01 (serial "105-870315-001"); w3/w4 the same.

```
998A 5173 3C01 5173 3C01 2838 442C 0041 3D3C 0300 0101 F4F4 0000 FFFF FFFF FFFF
```

**(b) ICON workstation, node 16, boots from server 1 (ws_boot), 512 KB, keyboard, no RTC, no pedestal floppy:**

* w0 = E000 (network only) | 0800 | 0020 (no RTC) | 0008 | 0004 (workstation) | 0002 = **E82E**.
  * Add 0100 (→ **E92E**) if the emulator has no TMS5220. Clear bit 3 (E826) if the 1797 pedestal floppy is emulated.
* w12 = 0001; use 0002 for ws_fsys_boot.

```
E82E 5173 3C02 5173 3C02 2838 442C 0041 3D3C 0300 1001 F4F4 0001 FFFF FFFF FFFF
```

Notes:
* The fileserver role comes from: node ∈ {1, 245}, or (node ≠ 254 and w0 bit 2 = 0) (`machine_test` `0x77A`, `set_boot_type` `0x1AB`).
* `machine_test +f` returns 1 (cluster / network fileserver) when w10.lo == own node, and 2 (storage fileserver) otherwise (`0x1C3`–`0x1F3`).
* +f also needs sysblock `+0x4A ≥ 0xF0`. That value comes from the boot loader's parameter block (open item §6).
* Node 245 / 254 are the factory defaults.

---

## 2. Fileserver real-time clock (`/sys/read_rtc`, `/sys/set_rtc`)

* The user programs call `int 5Ch` fn **4A** (read) and **4B** (set) with a pointer to an 8-byte struct (read_rtc: struct at DS:113h; set_rtc: DS:123h).
* The kernel handlers (`009C:60F1`, `009C:6226`) drive a **4-bit-register clock chip with 13 registers (0–12)**.

### 2.1 Ports (CONFIRMED)

| Port | Use |
|---|---|
| **FAC3** (PPI-B port B) | Out: bits 0–3 = register address; bits 4–7 = write data |
| **FA80** | Out bit 0: 1 during a write cycle, 0 otherwise. **In: bits 0–3 = data read from the RTC** |
| **FA82** | Out: 0 = READ asserted while FA80 is sampled, then 1 |
| **FA84** | Out: WRITE strobe pulse 1 → 0 |
| **FA86** | Out: HOLD = 1 for the whole access, 0 at the end. Delays of `loop 50h` between steps |

The FA80–FA8E even ports behave like single-bit latches; FA88 is the beeper/LED (INFERRED: a 74LS259-type addressable latch).

```
009C:61E7 rtc_get(reg):  out FA80,0 ; out FAC3,reg ; out FA82,0 ; in al,FA80 ; out FA82,1 ; and ax,0Fh
009C:6357 rtc_put(reg,v): out FA80,1 ; out FAC3,(v<<4)|reg ; out FA84,1 ; out FA84,0 ; out FA80,0
```

### 2.2 Registers and struct

The registers match the **OKI MSM5832** layout (the chip name is INFERRED; the register semantics are CONFIRMED from the code):

| Reg | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Field | S1 | S10 | MI1 | MI10 | H1 | H10 (bits 0–1 tens, **bit 2 PM, bit 3 24 h**) | day of week | D1 | D10 (bits 0–1 tens, **bit 2 leap**) | MO1 | MO10 | Y1 | Y10 |

Struct (CONFIRMED, `009C:6106`–`61D7`):
* `[0]` = weekday; `[1]` = day; `[2]` = month; `[3]` = 2-digit year; `[4]` = hour; `[5]` = minute; `[6]` = second.
* `[7]` = flags: bit 3 = "binary" (always set on read), bit 2 = leap, bits 0–1 = H10 bits 2–3 (bit 0 PM, bit 1 24 h).

Year conventions:
* set_rtc derives the year as `(qnx_date >> 12) + 80` (set_rtc `0xE3`).
* read_rtc uses `year + 1980` only for its days-in-month check (`0x1EE`).
* So the register holds the **two-digit year (e.g. 91)**. read_rtc rejects month ∉ 1..12, hour (12 h form) ∉ 1..12, and min / sec > 59 ("invalid date in real-time clock").
* For an emulator: serve BCD digits per register from the host clock, with 24 h mode; set H10 bit 3 on writes.

---

## 3. Disk hardware as the 4.x software drives it

### 3.1 WD1002-05 task file (base **FAA0**, even addresses) — CONFIRMED in `/drivers/hd` and `/drivers/fd`

* Registers: +0 data (PIO, `rep insb` / `outsb`, 512 bytes, **no DMA**); +2 error / precomp; +4 sector count (always 1); +6 sector; +8 cyl lo; +A cyl hi; +C SDH; +E status / command.
* Commands used: **20h read**, **30h write**, **10h restore**, **70h seek** (mount), **50h format track** (hdformat `0x6A4`).
* Before each command: wait (status & C0h) == 40h (not busy, ready), with 2–3 retries of 65 535 polls.
* After the command the driver waits for the interrupt (int 70h fn 6), then polls BUSY (bit 7) to clear.
  * Status bit 5 → write fault; bit 0 → read error register `& D7h`; error bit 7 → bad-block code.
* After each operation the driver writes **SDH = BFh** (deselect). HDos_ldr writes FFh.
* **Hard-disk SDH = A0h | drive<<3 | head**: 512-byte sectors, drive 0..2.
* **Floppy SDH = 38h | unit<<1 | side** (WD1002 drive 3 = its WD2797):
  * sector numbers start at 1;
  * **cyl-hi bit 0 = 1 flags 40-track or double-step media** (driver `0x829`/`0x845`);
  * the driver header defaults are 1280 blocks, 16 spc, 8 spt, first sector 1, port FAA0 (`/drivers/fd` code `0x14`);
  * the real geometry comes from block 1 of the floppy.
* `/drivers/hd` CHS (`0xA9C`), with 1-based block b: `cyl = (b−1) / spc`, `head = rem / spt`, `sector = rem % spt + first_sector`.
* **Interrupts (CONFIRMED, kernel `009C:0251`):**
  * **80186 INT0** (vector 0Ch): the handler `009C:02C8` reads PPI-A port A. **PA5 → disk (int 65h)**, **PA7 → keyboard (int 64h)**. EOI 0Ch.
  * **INT2** (vector 0Eh): `009C:0306`. **PC5 → ARCNET (int 66h)**, **PC4 → speech (int 67h)**, **PPI-B PC0 → printer (int 61h)**.
  * I0CON and I2CON are set level-triggered (`FF38`/`FF3C` `|= 10h`); the mask register FF28 is `&= FFAFh`.
  * Drivers hook int 65h. The default handler reads FAAE and FAA0.
  * Emulator: assert INT0 while (WD1002 INTRQ or kbd) is pending, and reflect INTRQ on PA5. The INTRQ clears on a status read.
* **Controller presence (kernel `009C:56D4`):**
  * if FAA2..FAAE all read FFh → no controller;
  * else sysblock +5E |= 5 (`machine_test +F1` → 5);
  * if the FAA8 sector register reads back 00/55/AA → `|= 40h` (`+H1` → 1).

### 3.2 Hard-disk physical layout (FS-7; FS-8 answered)

Sources (all CONFIRMED): hdformat `0x499`–`0x606` writes it, `/drivers/hd` `0x380`–`0x690` parses it, HDos_ldr `0x1BC`–`0x3AC` parses it, version_test validates it.

**Physical block 1** (C0 H0 S0; block numbers are 1-based):

| Offset | Field |
|---|---|
| +02 | u16 loader start block (= 2 + BBT length) — the ROM also reads this |
| +04 | u16 loader block count |
| **+10..+2F** | **Geometry record**: `+10 = 02`, `+11 = 01`, `+12 = 00`, `+13` u8 heads, `+14` u32 total blocks, `+18` u32 sectors per cylinder (= heads × spt), `+1C` u16 spt (11h), `+1E` u16 first sector (0), `+20` u32 bias (0), `+24` u32 first block after the BBT, `+2F = 03` |
| **+30..+3F** | **BBT record**: `+30 = 02`, `+31` u16 0, `+33` u16 replacement count r, `+35` u32 BBT start block (= 2), `+39` u16 BBT length in blocks = ceil(8r/512), `+3F = 03` |
| +1BE | 4 × 16-byte partition entries, PC style: +0 = 80h active, +8 u32 start, +C u32 size (written by `fdisk`) |
| +1FE | **AA55** |

* version_test calls a disk "new format" only if AA55 is present, both records carry their 02/03 markers, and heads × spt == spc.
* Physical blocks 2.. hold the **BBT**: 64 entries per block, each `{u32 bad, u32 replacement}`. bad = 0 means a free spare. Maximum 320 entries.
* Then a **16-block (8 KB) loader area** (`hdformat l=/sys/HDos_ldr`).
* Then **r replacement blocks**. The default is r = total / 2048 (hdformat `0x8CC`).
* Then the partitions. `+park` takes the innermost cylinder (heads × 17 blocks) off the total.
* `/drivers/hd` stores the BBT location (`[si+1A..20]`) but does not remap. The remap must happen above the driver (INFERRED: fsys or driver_lib). A raw-image reader must apply the BBT itself.

**Older formats the driver still accepts:**
* `+1E0 = 02` / `+1FF = 03`: total @1E4, spc @1E8, spt @1EC, first sector @1EE, fs start @1F0+1.
* Word `+1EE == 1234h`: start @1F2, total @1F4, spc @1F6, spt u8 @1F8, first sector u8 @1F9.
* Neither has partitions.
* "Old format" in the manuals = a QNX fs over the whole 20 808-block disk; 4.x reformats it (`init_hd1`).

**Geometry assumptions:**
* hdformat defaults (data 0x4DF): **4 heads, 306 cylinders, 17 spt, 4-way interleave, ECC on**, i.e. 20 808 blocks = 10 MB. CONFIRMED (strings + data).
* HDos_ldr's fallback when no geometry record exists is also 20 808 / 68 / 17 (`0x1C0`).
* The 64 MB drive's geometry is **not built into any 4.x code**. `init_hd1` and `init_hd2` call `hdformat 1 -p l=/sys/HDos_ldr` without `c=`/`h=` only when version_test says the disk is **not** new-format.
* So 64 MB disks must already carry a geometry record written at the factory (INFERRED). For an emulator 64 MB image, write a record such as Micropolis 1335 = 1024 cyl × 8 heads × 17 spt, total 139 264 blocks, with 2 × 32 MB partitions via fdisk.

`/sys/init_hd1` and `init_hd2` (identical, 1442 bytes, "Release 4.02.04, Greg Rigole") take arguments #1–#4 = drive numbers per partition and #5 = scratch drive. The sequence is:

```
version_test 1 → if old: [copy bad_blocks]; hdformat 1 -p l=/sys/HDos_ldr; mount disk #1 /drivers/hd p=1;
  bbt 1 +u <list; fdisk 1 +p
per partition n: dcheck #n -p -v b=list; bbt 1 +u p=n <list; mount disk #n d=#n p=n; dinit #n +h -p
```

---

## 4. What the installer does (4.10.01)

### 4.1 Packages

* `/package` on disk 1 of each set holds one line, read by configure with `"%s%u%lu%lu%s%s"`:
  * `ICON Series System SW Release 4.10.01; QNX  5  4731  3593  4.10.00 4.02.00` = title; script prefix; number of disks; block sizes; version; minimum version to upgrade from.
  * Help disk: `…On_line Help Version 4.10.00; HELP 1 650 91 07 25`.
  * 4.10.05 patch: `…Upgrade Release 4.10.05; UPGRADE 1 4731 4731 4.10.05 4.10.01`.
* Each disk *n* carries a shell script `<prefix>_<n>` (`QNX_1`…`QNX_5`, `HELP_1`, `UPGRADE_1`). It runs with #1 = source prefix (floppy) and #2 = target prefix (hard disk).
  * The scripts unlock the target directories (`chattr a=+w`), delete obsolete files, and run `backup #1 #2 +a +d +o -v p=^bitmap p=^QNX_n -p`.
  * QNX_1 seeds `hw_dbase.*` and `user_dbase.*` only if they are missing.
  * QNX_5 relocks `/boot`, `/cmds`, `/fonts`, `/icons` and `/config` (`a=-w`).
* Cluster copies follow `config/cluster.cpy`, `cluster.del` and `network.cpy` (lists of paths; `?a = b` lines copy a template). All CONFIRMED (text).
* `convert_4.02` (disk 5) runs `verify_net -p`, then `new_user f=[1]1:/config/pass +o c=1`, for upgrades from 4.02.

### 4.2 Flow for a new network (CONFIRMED where cited; ordering from the configure strings plus the manuals)

1. **Network fileserver** (node 1 in NVRAM, see §1.5a): boot from the **ICON Fileserver Boot Diskette**. The kernel is `fd_boot`, and the floppy is drive 1. Its `sys.init` (4.01 disk):
   * `machine_test +f` ≠ 0 → `add_cache`;
   * `mount disk 2 /drivers/hd p=1` and `mount disk 3 d=2 p=2`;
   * a 20 KB ramdisk on 8; `read_rtc`; **`bootrec &`** (the netboot server).
2. **A workstation** netboots from it (selector 1 → `1:/boot/ws_boot` from the boot floppy). Log in as `root` / `superuser`.
3. Swap the floppy in node 1 for the **Configuration diskette** and run `configure` on the workstation.
   * configure:
     * checks for `[1]1:/config/setup_disk`, a 1-byte marker file on the Configuration diskette; without it configure reports "The diskette is not a valid Configuration diskette";
     * runs `[1]verify_net -p`;
     * probes nodes with `[n] [1]/sys/machine_test +H1` and `/sys/version_test`;
     * initializes the disk by running **`[n] [1]/sys/init_hdN …` on the fileserver** (the `[node]` prefix runs the command on that node; hdformat accesses FAA0 directly, so it must run on fileserver hardware);
     * mounts scratch ramdisk 7 (`mount ramdisk 7 [1]/drivers/ramdisk s=20k`, `dinit 7`).
   * Then: "Please put the first diskette of the SYSTEM SOFTWARE package in the diskette drive of node 1". configure reads `/package` and runs `QNX_1..5` per disk ("Installing System Software on node %d, from hard drive on node 1" for cluster fileservers). It asks for the Configuration diskette again, and writes `net.config`, `hw_dbase` and node NVRAM through `[1]1:/sys/node_info`.
4. Reboot node 1 from the hard disk (drive lever open): ROM HD loader → `HDos_ldr` (blocks after the BBT) → `1:/boot/hd_boot`.
   * The 4.10 `sys.init` (SSW disk 1) then runs `poll`, `mount_localdisks`, `mount disk 2 /drivers/fd`, `read_rtc`, `bootrec &`, `dbase_admin &` and `def_server 1 1 +l`.
5. Reboot the workstation, run `verify_net`, then `attach_net [n=] [c=]` on each new workstation, and reboot each one.

### 4.3 Minimum emulated hardware per step

* **Fileserver (every step):**
  * 80186 with timers, chip selects and DMA unused by the drivers;
  * PPI-A (straps ≠ 0, NVRAM, PC7 sync);
  * CRTC registers (POST only);
  * WD1002-05 with HD drive 0 and floppy drive 3 (PIO + INT0 via PA5);
  * COM9026 + 2 KB RAM; the RTC (§2); the 8274 serial chip (POST 38–3F, unless w0 bit 4 = 1).
  * Keyboard, speech and trackball are not needed (w0 bits 7 / 8).
  * **The fileserver console is not used by the documented flow.**
* **Workstation (steps 2–5): required.** configure is a graphical ICON_LOOK application:
  * it loads `/sys/tball_mgr`, `GRAPHICS.slib`, `driver_lib` and `EGA.slib` (strings in configure data 0x31F7…);
  * it runs commands remotely on node 1;
  * so an emulated ICON workstation (video, keyboard, network, NVRAM; netboot) plus ARCNET between two emulated nodes is the minimum.
* **Keyboard vs trackball:**
  * configure shows "Use Scroll Lock to select, Print Screen for help." only when `[0x3BDE]` is non-zero (code `0x10D9`). That flag comes from a GRAPHICS / driver_lib slib call (`0x1385`).
  * The `.net` help says "press **Scroll Lock** over the arrow… Use the cursor keys". The ICON help says "place the pointer… press **Action**", with arrow keys for scrolling (CUR LOCK on).
  * Dialogs always accept **Enter = OK / Esc = Cancel** (help/configure/default).
  * **INFERRED:** on an ICON, choosing options needs the trackball pointer unless driver_lib reports the keyboard-pointer mode (likely when no trackball is present). Emulate the trackball counters (PB7..6 = 00/01 Microwire counters, polled by a timer-2 ISR at `009C:6B78`) to be safe.

### 4.4 Important finding: the 4.10.01 "home-brew" fileserver boot disk is incomplete

* `ISSW_4.10.01_Home_brew_fileserver_boot` carries the **4.01** `fd_boot` (string `4.01_boot` at file 0x154; MD5 identical to the 4.01 disk).
* Its `boot_config` points to `1:/boot/ws_boot`, `ws_fsys_boot`, `ws2_high_boot` and others, **none of which are on the disk** (only fd_boot, hd_boot and ws2_boot are).
* Its `sys.init` mounts no hard disk.
* So it **cannot netboot any workstation**. Use the **4.01.00 ICON Fileserver Boot Diskette** (it has ws_boot, and its boot_config is consistent), or copy `ws_boot` from SSW disk 3.
* (FW-11 still applies: the 5053 ROM cannot read either 800 K boot disk past block 8.)

---

## 5. `boot_config`, `net.config`, `hw_dbase`

* **`/config/boot_config`** (CONFIRMED): text, 0x1E line ends. Each line is `<type>` + spaces or tab + `<path>`. bootsnd reads it (`c=` overrides it) and reports "Boot file type out of range".
* **Netboot reply (NET-1)** (CONFIRMED in bootsnd `0x44E`–`0x4A3`, `0x55D`–`0x631`):
  * bootsnd builds `FF×6, 'b', 'b', type, u16 block#, u16 flag, u16 load_seg, u16 load_off, u16 count, u16 arg0, u16 arg1, u16 IP, u16 CS, u16 0, data`.
  * It sends this to the network administrator with int 70h Send, msg code 2, broadcast.
  * The ROM sees it **shifted by one byte** (its "+7 'b' … +9 selector"), so the network layer prepends one byte.
  * Per-type tables at data 0x35D (count), 0x3DD (arg0), 0x41D (arg1), 0x45D (IP), 0x49D (CS).
  * The **first packet** loads 16 bytes to **0060:0000**: +4 = `server_nid`, +6 = `server_tid`, +8 = `poller_nid`, from int 70h fn 0Eh indices 0Dh, 0Eh and 11h, the word indices of `struct system_info` in `/lib/task.h`.
* **`net.config`** (NET-3) (CONFIRMED, attach_net `0x2FD0`–`0x30D4`, configure the same):
  * one line per node, `%u;%u;%s;%s;%s[;name=%s]\n` = node ; cluster node ; class ; kind ; serial [; fileserver name ≤ 16 chars];
  * class/kind words from the table at data 0x16A4: `inactive fileserver workstation icon icon2 netcard286 netcard386 network cluster storage network2 cluster2 storage2 network286 cluster286 storage286 network386 cluster386 storage386 icon3`;
  * the in-memory entry is 28 bytes (+2 cluster, +4 class idx, +5 kind idx, +6 u32 serial, +0xA name);
  * a lock file `net.config.lock`, with a `.tmp` rewrite.
  * Example (INFERRED): `1;1;fileserver;network;<serial>;name=MAIN`.
* **`hw_dbase.*`** (HW-5, partly): a "cem_dba" keyed database (error strings in attach_net), served by `/sys/dbase_admin`.
  * The shipped empty `.dat` header is `0000 0000 0100 0000 4600 0C00`: INFERRED record length 46h = 70 bytes, header 12 bytes.
  * `.kds` (64 bytes) starts with `3E00 'BD' … 0300` = 3 keys, held in `.kya`, `.kyb` and `.kyc`.
  * Field layout not decoded.

---

## 6. Still unknown

1. **Bits 6, 9, 10 of w0 and bits 0–1 / 4–7 of w13.** Their accessors exist but no examined program gives them a name.
2. **The kernel's boot parameter block.** es:bx at kernel entry; sysblock +2 = parameter segment, +6 = kernel DS; `[param:0x38]` → sysblock +4A, which `machine_test +f/+b` need ≥ F0h. This must be decoded from HDos_ldr (its exit at `0xBE0` retf) and from the ROM netboot (`arg0`/`arg1`) before HLE boot is possible.
3. **The exact PB7..6 decoder and the trackball / baud counter chips.** Their 6-bit commands are FC, A4, 84+16, 94, 80+16, 90, D4 (init) and 9C, 98, 88 (read) / 8C (read). The part is unidentified.
4. **Whether `node_info`'s rejected fn 3A writes matter on ICON** (loader nibble never updated by attach_net).
5. **The hw_dbase record fields and the meaning of hdformat's +11/+12 geometry bytes.**
6. **The real 64 MB drive geometry** (needs an image or the factory label).
7. **The meaning of `int 5Ch` fn 12h (returns 2) and fn 4Ch (+b).**
