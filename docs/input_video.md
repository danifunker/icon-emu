# ICON (80186) keyboard, trackball and display: reverse-engineering notes

Status: working notes for the Rust/WASM emulator. Every claim carries **[C]** (CONFIRMED: read directly from code or data) or **[I]** (INFERRED). "Manual" means the Developer's Toolkit Vol 2, Appendix D (`txt/...Developer's Toolkit Operations Guide Volume 2...txt`, lines ~10617–10970).
All relative paths (`re/`, `tools/`, `disks/`, `txt/`) are under the session scratchpad `/private/tmp/claude-501/-Users-dani-repos-icon-icon2-research/070eb9df-879d-4f22-a9ce-20d79298baf5/scratchpad/`.


## 0. Sources and how to reproduce

| Name | What | Offset convention |
|---|---|---|
| ROM | `firmware/ICON1/icon1_rom_combined.bin`. Code CS=F88F | **file = IP + 0x8F0**. All ROM addresses below are IPs |
| `ws_boot` 4.10.01 | `disks/System_4.10.01__..._DISK_3_OF_5/boot/ws_boot` (86 427 B). **Compressed** | see below |
| `ws410.bin` | Decompressed boot image: `re/ws410.bin` (117 088 B) | "img" offsets |
| `icon_emul` | `disks/..._DISK_4_OF_5/sys/icon_emul` (the ICON2's ICON emulator) | Plain addresses are `re/icon_emul.asm` offsets (= file − 0x10). Jump-table and trap-table *values* are IPs (= file − 0x14 = asm − 4) |

**Decompressing ws_boot [C].** The file has a 0x40-byte header, then a stub (file 0x40) that moves the image up by 0x20000 and expands it. The stub is at img 0x50–0x8F. Compressed data starts at **file 0x180**. The only escape byte is 0x9D:
- `9D 00` → a literal 0x9D
- `9D 01` → end of data
- `9D n` → n zero bytes
- any other byte → copied as is

Script: `tools/unrle.py ws_boot 0x180 ws410.bin`. The 4.01 `ws_boot` is not compressed: its image starts at file 0x40.

**Boot-image task table [C]** (img 0x138 + 0x40·n). Each entry gives the task name, its CS paragraph and its code size in paragraphs. The real CS base is para·16 + 0x100: the jump tables and the code at `cs:0x755d` only make sense with that 0x100.

| Task | Code base (img) | Data base (img) |
|---|---|---|
| task admin (kernel) | **0x2C0** | 0x8660 |
| fsys admin | 0xB710 | |
| **dev admin** (console, keyboard, serial) | **0xCC90** | **0x136C0** |
| idle / net admin / shared lib | 0x170F0 / 0x17990 / 0x1A770 | |

Below, `kern:XXXX` means img = XXXX + 0x2C0, `dev:XXXX` means img = XXXX + 0xCC90, and `devds:XXXX` means img = XXXX + 0x136C0. Disassemblies: `re/kern.asm`, `re/dev.asm`, `re/romm.asm`, `re/icon_emul.asm`, `re/win.asm`, `re/tball.asm`.

---

## 1. Interrupt wiring

80186 vectors set up by the kernel at kern:0251 (img 0x511) [C]:

```
kern:0253  mov word [0x30],0x2c8   ; type 0Ch INT0 -> kern:02C8
kern:025D  mov word [0x34],0x395   ; type 0Dh INT1 -> IRET (dev admin later hooks it for the 8274 serial chip)
kern:0267  mov word [0x38],0x306   ; type 0Eh INT2 -> kern:0306
kern:0271  mov word [0x3c],0x395   ; type 0Fh INT3 -> IRET (unused)
kern:027B  [0x190]=0387 (INT 64h kbd) [0x194]=0354 (65h disk) [0x198]=0361 (66h net)
           [0x19c]=037E (67h speech) [0x184]=038E (61h printer)
kern:02AD  FF38 |= 0x10 ; FF3C |= 0x10   ; INT0 and INT2 level-triggered
kern:02BD  FF28 &= 0xFFAF                ; unmask INT0 and INT2
```

**INT0 is shared. Its dispatcher polls PPI A port A (F981)** [C], kern:02C8:

```
in  F981 ; test al,0x20 ; jz +   ; PA5 -> INT 65h (disk). If the handler returns non-zero, reschedule
in  F981 ; test al,0x80 ; jz +   ; PA7 -> INT 64h (KEYBOARD)
out FF22,000Ch                   ; specific EOI for INT0
```

**INT2 is also shared** (kern:0306) [C]:
- PPI A port C (F985) PC5 → INT 66h (network COM9026).
- PC4 → INT 67h (TMS5220; the default handler reads FA01).
- PPI B port C (FAC5) PC0 → INT 61h (printer).
- EOI is `FF22=000Eh`.

| Source | Pin | CPU line | SW vector | Clear / ack |
|---|---|---|---|---|
| **Keyboard** | PPI A **PA7** | **INT0** | 64h | any write to **F920** (default handler kern:0387: `out F920,al`) [C] |
| Disk | PPI A PA5 | INT0 | 65h | read FAAE, then FAA0 [C] |
| Network | PPI A PC5 | INT2 | 66h | F983 bit1 low, F880 [C] |
| Speech | PPI A PC4 | INT2 | 67h | read FA01 [C] |
| Printer | PPI B PC0 | INT2 | 61h | write FAC3 [C] |
| Serial 8274 (FAE5/FAE7) | | INT1 | 0Dh → dev:6184 | dev:2CCF unmasks bit 5 of FF28 [C] |
| **Trackball polling** | (none) | **Timer 2** (type 13h) | 13h → kern:81CB | non-specific EOI `FF22=8000h` [C] |

The trackball raises **no** hardware interrupt [C]. It is polled by 80186 timer 2 (§3).

---

## 2. Keyboard

### 2.1 Hardware interface [C]

- **Data:** a parallel 8-bit read at **I/O F920**. The value on the bus is the **complement** of the key code: the ISR does `in al,F920 / not al`.
- **Interrupt:** PPI A **PA7** (F981 bit 7) goes high while a byte is pending. It is wired to **INT0** through the dispatcher above.
- **Acknowledge:** **write any byte to F920.** This clears PA7 and the interrupt. The dev-admin ISR writes back the complemented byte. The ROM writes back the raw byte.

The dev admin's real ISR (installed at INT 64h by dev:2C60, `[0x190]=5954`) is at dev:5954, img 0x125E4:

```
dev:5954  push es/ds/dx ; ds = dev data
dev:5966  mov dx,0xf920
dev:5969  in  al,dx
dev:596A  not al                 ; code = ~port
dev:596C  xor ah,ah
dev:596E  mov [0x30fa],ax        ; raw code
dev:5971  out dx,al              ; ack / clear PA7
dev:5972  mov dx,0xff28 ; save IMR ; out 0xffff (mask all) ; call 0x5991 (decode) ; restore IMR
dev:5990  iret                   ; EOI is sent by the kernel INT0 dispatcher
```

### 2.2 Byte protocol (code = ~port)

| Code | Meaning | Evidence |
|---|---|---|
| **0x01–0x66** | **Make** of key number 1–102. The key number equals the decimal "Key Number" column of manual Table D-1 (#001 Esc … #102) | [C] decoder dev:5C7C (`and ax,7Fh` key, `and ax,80h` break); tables are 0x67 long (devds:3470 = 0x0066) |
| **0x81–0xE6** | **Break** of the same key (bit 7 set) | [C] |
| 0x7F | Auto-repeat **tick**. While a key that may repeat is held, a repeat counter (devds:30FC) is loaded with 10 on make. Each 0x7F decrements it; when it reaches 1, every further 0x7F re-emits the last key. Releasing that key zeroes the counter | [C] dev:5C91–5CB9 and dev:5DD2 ff. **[I]** The keyboard MCU itself sends 0x7F (raw port 0x80) periodically while any key is held. The rate is unknown |
| 0xFF | Ignored (raw port 0x00) | [C] dev:5C83 |

The keyboard reports **lock keys as make and break**. Caps Lock (0x3F) and Cursor Lock (0x2F) are plain level modifiers in the driver: make sets a state bit and break clears it [C] (type-0 entries, dev:5D3B).

**[I]** So the 8035 keyboard MCU implements the toggling and the LEDs, and sends a make when a lock turns on and a break when it turns off. This is backed by the power-up self-test below.

**The host never sends commands or LED data to the keyboard [C].** The only writes to F920 anywhere in ws_boot are the two acks above, and the ROM writes only acks and a 0 at init (ROM:0A25).

**Power-up self-test (ROM:0941–09CC)** [C]:
- ROM:0953 waits for PA7 (error 17).
- It reads F920 and writes it back.
- It accepts only the **raw** values `D0h, 50h, C0h, 40h`. Complemented, these are **2F, AF, 3F, BF**: Cursor Lock make/break and Caps Lock make/break. **[I]** The keyboard therefore announces its lock state after reset.
- Anything else is retried up to 256 times, then error 19 ("invalid keycode").
- After a valid code, PA7 must fall within 16 polls (error 18).
- If NVRAM word 0 bit 7 is set ("keyboard option"), keyboard errors are suppressed (ROM:0949).

**Emulator recipe:**
- On key down, latch `~keynum`; on key up, latch `~(0x80|keynum)`.
- Raise PA7 (→ INT0); drop PA7 when F920 is written.
- While a key is held, send `~0x7F` = 0x80 every ~50 ms (rate is a guess).
- At reset, send `~0x2F|0x80` and `~0x3F|0x80` (locks off): raw 0x50 and 0x40.
- RESET key #016 (code 0x10) has no table entry. **[I]** It is a hardware reset or NMI line rather than a scancode.

### 2.3 Driver state machine [C]

Decoder dev:5C7C. Modifier table at devds:3105, 12 entries of 4 bytes (key, type, mask, code); the count is at devds:3472.

| Key | Type | Effect |
|---|---|---|
| 21h Control | 0 | state bit 0x02 |
| 2Fh Cursor Lock | 0 | bit 0x10 (keypad gives cursor codes) |
| 30h, 3Bh Shift | 0 | bit 0x01 (counted, two keys) |
| 3Fh Caps Lock | 0 | bit 0x04 (affects keys with flag 0x04) |
| 41h Alt | 2 | down: bit 0x08. Released alone: start **compose** (2 hex digits or 2 chars → accent table at devds:3338) |
| 01h Esc with Ctrl+Shift | 1 | code 0Ah → dev:5A9A `int 73h` (debugger) |
| 02h `1` with Ctrl+Shift | 1 | code 0Bh → event 16h (switch debugger screen) |
| 09h `8` with Ctrl+Shift | 1 | code 09h → event 12h (screen snap, `/cmds/prtsc`) |
| 42h Rubout with Ctrl | 1 | code 00h → keyboard break (dev:5A74) |
| 3Dh, 44h Help (unmodified) | 1 | code 08h → event 17h (Help port signal) |

Per-key flags (devds:3135, indexed by key) [C]:
- 0x08: auto-repeat.
- 0x04: Caps Lock applies.
- 0x02: Cursor Lock applies.
- 0x80: always send an up-code on release.
- 0x01: send an up-code if `FUNC_UP_CODES` (0x2000) is set.
- 0x40 / 0x20 / 0x10: the code is a "function code" in the normal / shift / ctrl table (prefixed by 0xFF under EXPAND_FUNCTION).

Translation tables, each 0x67 bytes indexed by key; 0xFF means no code [C]:

| Table (devds) | img | Use |
|---|---|---|
| 3203 / 326A / 32D1 | 168C3 / 1692A / 16991 | **QNX mode** Normal / Shift / Ctrl. Matches manual Table D-1 |
| 319C | 1685C | QNX **Alt/Up** column (with Alt held, and release codes) |
| 34DF / 35AD / 367B | 16B9F / 16C6D / 16D3B | ICON (Telidon) mode Normal / Shift / Ctrl. Accents become `19h` sequences |
| 3546 / 3614 (/367B) | | Editor mode (tty option 0x4000 `USED_BY_CEMCORP`) |
| 3478 | 16B38 | ICON/editor-mode up-codes (A0–AD for keys 45h–52h) |

### 2.4 Key code table (QNX mode), extracted from ws_boot 4.10.01 [C]

Key names come from manual Table D-1 [C, manual]. Code values are printed as hex; `-` means no code. Generated by `tools/kbmd.py`.

| code (hex) | key # | key | flags | Normal | Shift | Ctrl | Alt/Up | notes |
|---|---|---|---|---|---|---|---|---|
| 01 | 001 | Esc | 08 | 1b | 1b | 1b | - | Ctrl+Shift+Esc -> debugger |
| 02 | 002 | 1 ! | 08 | `1` | `!` | - | - | Ctrl+Shift+1 -> debug screen |
| 03 | 003 | 2 @ | 08 | `2` | `@` | 00 | - |  |
| 04 | 004 | 3 # | 08 | `3` | `#` | - | - |  |
| 05 | 005 | 4 $ | 08 | `4` | `$` | - | - |  |
| 06 | 006 | 5 % | 08 | `5` | `%` | - | - |  |
| 07 | 007 | 6 ^ | 08 | `6` | `^` | 1e | - |  |
| 08 | 008 | 7 & | 08 | `7` | `&` | - | - |  |
| 09 | 009 | 8 * | 08 | `8` | `*` | - | - | Ctrl+Shift+8 -> screen snap |
| 0a | 010 | 9 ( | 08 | `9` | `(` | - | - |  |
| 0b | 011 | 0 ) | 08 | `0` | `)` | - | - |  |
| 0c | 012 | - _ | 08 | `-` | `_` | 1f | - |  |
| 0d | 013 | = + | 08 | `=` | `+` | - | - |  |
| 0e | 014 | ` ~ | 08 | 60 | `~` | - | - |  |
| 0f | 015 | Backspace | 08 | 08 | 08 | 7f | - |  |
| 10 | 016 | RESET (no code in tables) | 00 | - | - | - | - |  |
| 11 | 017 | Tab | 38 | 09 | 80 | 9f | - |  |
| 12 | 018 | Q | 0c | `q` | `Q` | 11 | - |  |
| 13 | 019 | W | 0c | `w` | `W` | 17 | - |  |
| 14 | 020 | E | 0c | `e` | `E` | 05 | - |  |
| 15 | 021 | R | 0c | `r` | `R` | 12 | - |  |
| 16 | 022 | T | 0c | `t` | `T` | 14 | - |  |
| 17 | 023 | Y | 0c | `y` | `Y` | 19 | - |  |
| 18 | 024 | U | 0c | `u` | `U` | 15 | - |  |
| 19 | 025 | I | 0c | `i` | `I` | 09 | - |  |
| 1a | 026 | O | 0c | `o` | `O` | 0f | - |  |
| 1b | 027 | P | 0c | `p` | `P` | 10 | - |  |
| 1c | 028 | e-acute | 0c | 82 | 90 | - | - |  |
| 1d | 029 | c-cedilla | 0c | 87 | 80 | - | - |  |
| 1e | 030 | umlaut/circumflex (dead) | 68 | db | dc | - | - |  |
| 1f | 031 | [ { | 08 | `[` | `{` | 1b | - |  |
| 20 | 032 | ] } | 08 | `]` | `}` | 1d | - |  |
| 21 | 033 | Control | 00 | - | - | - | - | modifier: Ctrl (state bit 0x02) |
| 22 | 034 | A | 0c | `a` | `A` | 01 | - |  |
| 23 | 035 | S | 0c | `s` | `S` | 13 | - |  |
| 24 | 036 | D | 0c | `d` | `D` | 04 | - |  |
| 25 | 037 | F | 0c | `f` | `F` | 06 | - |  |
| 26 | 038 | G | 0c | `g` | `G` | 07 | - |  |
| 27 | 039 | H | 0c | `h` | `H` | 08 | - |  |
| 28 | 040 | J | 0c | `j` | `J` | 0a | - |  |
| 29 | 041 | K | 0c | `k` | `K` | 0b | - |  |
| 2a | 042 | L | 0c | `l` | `L` | 0c | - |  |
| 2b | 043 | ; : | 08 | `;` | `:` | - | - |  |
| 2c | 044 | ' " | 08 | `'` | `"` | - | - |  |
| 2d | 045 | Enter | 08 | 0d | 0d | 0a | - |  |
| 2e | 046 | grave (dead) | 48 | dd | e1 | - | - |  |
| 2f | 047 | CURSOR LOCK | 00 | - | - | - | - | modifier: Cursor Lock (bit 0x10) |
| 30 | 048 | Shift (left) | 00 | - | - | - | - | modifier: Shift (bit 0x01) |
| 31 | 049 | Z | 0c | `z` | `Z` | 1a | - |  |
| 32 | 050 | X | 0c | `x` | `X` | 18 | - |  |
| 33 | 051 | C | 0c | `c` | `C` | 03 | - |  |
| 34 | 052 | V | 0c | `v` | `V` | 16 | - |  |
| 35 | 053 | B | 0c | `b` | `B` | 02 | - |  |
| 36 | 054 | N | 0c | `n` | `N` | 0e | - |  |
| 37 | 055 | M | 0c | `m` | `M` | 0d | - |  |
| 38 | 056 | , < | 08 | `,` | `<` | - | - |  |
| 39 | 057 | . > | 08 | `.` | `>` | - | - |  |
| 3a | 058 | / ? | 08 | `/` | `?` | - | - |  |
| 3b | 059 | Shift (right) | 00 | - | - | - | - | modifier: Shift (bit 0x01) |
| 3c | 060 | angle quotes | 08 | af | ae | - | - |  |
| 3d | 061 | Help | 70 | e8 | e8 | e8 | - | unmodified -> Help event |
| 3e | 062 | Pause | 80 | 13 | 13 | 13 | 11 |  |
| 3f | 063 | CAPS LOCK | 00 | - | - | - | - | modifier: Caps Lock (bit 0x04) |
| 40 | 064 | Space | 08 | 20 | 20 | 20 | - |  |
| 41 | 065 | Alt / Compose (unlabelled) | 00 | - | - | - | - | Alt (bit 0x08) / compose |
| 42 | 066 | Rubout | 08 | 7f | 7f | 7f | - | Ctrl+Rubout -> break |
| 43 | 067 | \ \| | 08 | `\` | 7c | 1c | - |  |
| 44 | 068 | Help | 70 | e8 | e8 | e8 | - | unmodified -> Help event |
| 45 | 069 | ACTION (left) | f0 | fb | fb | fb | fc |  |
| 46 | 070 | unlabelled (trackball area) | f0 | f1 | f1 | f1 | f2 |  |
| 47 | 071 | F1 | 71 | 81 | 8b | 95 | d1 |  |
| 48 | 072 | F2 | 71 | 82 | 8c | 96 | d2 |  |
| 49 | 073 | F3 | 71 | 83 | 8d | 97 | d3 |  |
| 4a | 074 | F4 | 71 | 84 | 8e | 98 | d4 |  |
| 4b | 075 | F5 | 71 | 85 | 8f | 99 | d5 |  |
| 4c | 076 | F6 | 71 | 86 | 90 | 9a | d6 |  |
| 4d | 077 | F7 | 71 | 87 | 91 | 9b | d7 |  |
| 4e | 078 | F8 | 71 | 88 | 92 | 9c | d8 |  |
| 4f | 079 | F9 | 71 | 89 | 93 | 9d | d9 |  |
| 50 | 080 | F10 | 71 | 8a | 94 | 9e | da |  |
| 51 | 081 | unlabelled (trackball area) | f0 | f3 | f3 | f3 | f4 |  |
| 52 | 082 | ACTION (right) | f0 | fd | fd | fd | fe |  |
| 53 | 083 | CLR [Home] | 3a | 0c | e3 | e3 | e3 |  |
| 54 | 084 | Divide | 3a | f6 | e4 | e4 | e4 |  |
| 55 | 085 | Multiply | 3a | fa | e5 | e5 | e5 |  |
| 56 | 086 | + | 3a | `+` | e6 | b7 | c7 |  |
| 57 | 087 | kp 7 | 3a | `7` | a0 | b0 | c0 |  |
| 58 | 088 | kp 8 | 3a | `8` | a5 | b5 | c5 |  |
| 59 | 089 | kp 9 | 3a | `9` | a2 | b2 | c2 |  |
| 5a | 090 | kp - | 3a | `-` | a3 | b3 | c3 |  |
| 5b | 091 | kp 4 | 3a | `4` | a8 | b8 | c8 |  |
| 5c | 092 | kp 5 [Up] | 3a | `5` | a1 | b1 | c1 |  |
| 5d | 093 | kp 6 | 3a | `6` | aa | ba | ca |  |
| 5e | 094 | kp , | 3a | `,` | e7 | e7 | e7 |  |
| 5f | 095 | kp 1 [Left] | 3a | `1` | a4 | b4 | c4 |  |
| 60 | 096 | kp 2 [Down] | 3a | `2` | a9 | b9 | c9 |  |
| 61 | 097 | kp 3 [Right] | 3a | `3` | a6 | b6 | c6 |  |
| 62 | 098 | kp Enter | 3a | 0d | a7 | b7 | c7 |  |
| 63 | 099 | kp 0 [Ins] | 3a | `0` | ab | bb | cb |  |
| 64 | 100 | kp 0 [Ins] (2nd switch) | 3a | `0` | ab | bb | cb |  |
| 65 | 101 | kp . [Del] | 3a | `.` | ac | bc | cc |  |
| 66 | 102 | kp Enter (2nd switch) | 3a | 0d | a7 | b7 | c7 |  |

Notes:
- The keypad rows (53h–66h) produce the Shift column when Cursor Lock is on (flag 0x02).
- Codes D1–DA in the Alt/Up column are F31–F40; A0–AC are cursor keys (`tcap_keys.h`). 81–9E are F1–F30.
- **ACTION keys are keyboard keys.** Left ACTION = 45h (#069) gives FBh; right ACTION = 52h (#082) gives FDh. The two unlabelled keys by the trackball are 46h → F1h and 51h → F3h. Each one's release (up) code is +1 (FC, FE, F2, F4) [C table, manual D-6; the toolkit example `ACTION_PRESS 0xfd / ACTION2_PRESS 0xfb`, TK2 line ~1132].

---

## 3. Trackball

### 3.1 Hardware: two serial counter chips on the PPI A Microwire bus [C, from code]

**The trackball is not in the keyboard stream and has no interrupt.** Its counts sit in two Microwire up/down counter chips that share the serial bus with the NVRAM 93C06:

| Signal | PPI A pin | Port / operation |
|---|---|---|
| DI (CPU → chip) | **PC0** | F987 BSR: `00h` / `01h` |
| SK (clock) | **PC1** | F987 BSR: `03h` (high), then `02h` (low) |
| DO (chip → CPU) | **PC6** | F985 bit 6 |
| Device select | **port B bits 7..6** | F983 |
| Strobe / enable (active low) | **port B bit 5** | F983. Set to 1 to deselect |

Port B bits 7..5 select the device [C]:
- `100` = NVRAM (kern:7DBE and kern:7E44 `and al,1Fh / or al,80h`)
- `000` = counter chip "sel 0" (**Y axis**)
- `010` = counter chip "sel 0x40" (**X axis**)
- `xx1` = idle
- [I] `11x` is probably the "baud-rate counter" chip of ROM errors 2B/2C.

**[I]** The chips are National **COP452L**-type dual 16-bit counters with a Microwire interface (the ROM error text reads "Trackball X counter A/B", "Y counter A/B", "Baud rate counter A/B"). The ICON2's software emulation of them (§5) fixes the command semantics regardless of the part number.

**Command set** (6-bit instruction, MSB first, leading 1, from the kernel bit-banger kern:817D/8299 and the icon_emul decoder) [C]:

| Bits | Hex (6-bit) | Meaning (icon_emul handler) |
|---|---|---|
| 100000 + 16 data bits | 20h | load register A |
| 100001 + 16 | 21h | load register B |
| 100010 → 16 bits out | 22h | read register A |
| 100011 → 16 bits out | 23h | read register B |
| 100100 | 24h | counter A ← register A |
| 100101 | 25h | counter B ← register B |
| 100110 | 26h | register A ← counter A (latch) |
| 100111 | 27h | register B ← counter B (latch) |
| 111111, 101001, 110101 | 3Fh, 29h, 35h | sent at start-up. **[I]** Reset and mode/start commands; icon_emul ignores them |

Each instruction is framed by a strobe pulse: kern:82BD sets bit 5, then clears it.

### 3.2 Driver (kernel `int 5Ch` functions) [C]

`int 5Ch` is the kernel hardware API (vector 0x170 → kern:72E7). It dispatches functions 0Ah–6Dh through the table at kern:730C. The trackball calls (used by `tball_mgr` and `icon_emul`) are:

| Function | Address | Action |
|---|---|---|
| 2Eh start_trackball | kern:80BB | Initialise both chips (Y first, at kern:80D1/80DC; sequence kern:8165), zero the positions, hook **INT 13h (timer 2)**, program timer 2: `FF66=4000h`, `FF60=0`, **`FF62=AEA8h`**, `FF66=E001h` |
| 2Fh stop | kern:82D2 | Restore INT 13h, disable timer 2 |
| 30h | kern:8323 | return X (`sys+0x82`) |
| 31h | kern:833C | return Y (`sys+0x80`) |
| 32h | kern:8355 | `*arg1 = X`, `*arg2 = Y` |

Timer 2 counts CPU/4 = 1.79 MHz. Divided by 44 712 this gives **≈ 40 Hz polling** [C values, I clock].

The poll ISR at kern:81CB:

```
F983 bits7..5 = 000  ; Y chip
call 8236            ; cmd 27h, 26h ; cmd 22h -> read16 -> di ; cmd 23h -> read16 -> bx ; di = A - B
neg  di              ; Y is negated
cmp di,[sys+0x80] -> store, changed++
F983 bits7..5 = 010  ; X chip
call 8236
cmp di,[sys+0x82] -> store, changed++
if changed: mov ax,0115h / int 71h   ; signal the trackball event to tasks
```

So position = (counter A − counter B), read raw as an **absolute** 16-bit value each poll. X = A−B of chip 0x40; Y = −(A−B) of chip 0x00 [C].

The axis assignment is confirmed by icon_emul: chip 0x40 is driven from `int 5Ch` function 30h (X) and chip 0x00 from function 31h (Y) [C].

**Emulator recipe (follows icon_emul exactly) [C]:**
- Keep two 16-bit counters per axis.
- On mouse motion of d counts:
  - if d > 0, `B -= d`
  - else `A -= |d|`
- So A − B grows with +X. For Y, +d is ICON2 "down", which makes the kernel's Y = −(A−B) grow upward. Graphics origin is bottom-left.
- icon_emul halves the host counts (`shr ax,1`) before applying them.

### 3.3 Buttons [C]

There are no separate trackball buttons in the ICON code. The **ACTION keys and the two unlabelled keys** are ordinary keyboard keys 45h/52h/46h/51h (§2.4). Window manager and toolkit code look for FBh/FDh in stdin.

---

## 4. Display

### 4.1 Frame buffer: location, layout and pages

- **System RAM, linear, 80 bytes per scan line, 240 lines, 19 200 bytes per page** [C]. The kernel clears a page with `mov cx,2580h / rep stosw` (kern:73C0). Mode set stores `sys+64h = width·bpp/8 = 80`, `sys+66h` = width, `sys+68h = 240`, `sys+6Ah` = bpp (kern:75B5).
- **Row order: top-down in memory (offset 0 = top-left)** [I, strong]. Evidence:
  1. The ROM diagnostic box (ROM:151E) draws, in increasing offset order:
     - top border at 0x1279 (line 59)
     - title text in lines 60–69
     - separator at 0x1599 (line 69)
     - message text at 0x18BA (lines 70–79)
     - bottom border at 0x37F9 (line 179)
     
     That is a coherent top-to-bottom layout.
  2. The font (ROM file 0x2CF4; ws_boot img ≈0x15C73 + 10·ch) stores glyphs **bottom row first**: 3 descender rows, then 7 glyph rows. The ROM copies byte *i* to `start − i·80` (`movsb / dec di / sub di,50h`), i.e. upward. Only top-down memory plus bottom-first glyphs renders 'L' and 'J' upright.
  3. The kernel stores **`sys+60h = 4AB0h` = 239·80**, the offset of the *bottom* line. This is the bottom-left graphics origin; drawing walks upward by −80 (kern image 0x50E0).
- **Bit order: LSB = leftmost pixel** [I, strong].
  - The ROM box's left edge byte is `03h` and its right edge byte is `C0h` (ROM:157A).
  - 'L' and 'J' read correctly only with LSB on the left.
- **Page base addresses** [C, kernel image 0x50D9–0x5128]: `sys+62h` (page 1, used by `$con`) is chosen from NVRAM word 0 bits 0–1 (extra 128 KB banks). The selector is the key table at kernel ds:00FE (img 0x875E).

  | Memory option | Page 1 base |
  |---|---|
  | 0 | **3ABA0** |
  | 1 | 5ABA0 |
  | 2 | 7ABA0 |
  | 3 | 1ABA0 |
  | other | 5ABA0 |

  The ROM writes its screen to 3ABA0, 5ABA0 and 7ABA0 because it does not know which applies (ROM:1523 loop, `add es,2000h`).
- **Double buffering is not done through the CRTC.**
  - PPI A **port B bit 0 (F983.0)** selects the page: **1 = xABA0** (page 1), **0 = x2BA0** (32 KB lower; page 2, the debugger screen at `sys+54h`) [C kern:75C0–7642; icon_emul 0x46F–0x4BC uses `sys+62h` for bit0=1 and `sys+54h` for bit0=0].
  - The OS **never writes the 6845** (no F801/F803 access anywhere in ws_boot) [C].
  - **[I]** Video fetch address = (bank chosen by the memory configuration) + (F983.0 ? 0xABA0 : 0x2BA0) + line·80 + byte. For the emulator, take the base from `sys+62h`/`54h` semantics: page 1 = configured base, page 2 = base − 0x8000.

### 4.2 Modes [C]

| F983 bit 2 | Mode | Pixels per byte | Palette use |
|---|---|---|---|
| 1 | **640×240×1** | 8 (LSB = left [I]) | pixel 0 → **F900**, pixel 1 → **F906** [I, see below] |
| 0 | **320×240×2** | 4 | value v (0–3) → register F900 + 2·v [C kern:73E6, 2bpp path]. Bit order within the byte [I]: probably bits 1:0 = leftmost |

- Set by kern:75B5 (`int 5Ch` function 0Fh `(bpp, width, height, seg)`). Also set by `win_mgr` set_monitor at file 0x4E22 (`in F983 | 4` for 1 bpp, `& FBh` for 2 bpp) [C].
- After a reset, PPI outputs are 0, which means 2 bpp, page 2. The ROM writes `F983 = F7h` (640 mode, page 1, Microwire idle) at ROM:05FB [C].

### 4.3 Palette registers F900/F902/F904/F906

Four write-only 4-bit colour registers [C].
- **Kernel load from NVRAM word 11** (kern:73E6):
  - 2 bpp: F900..F906 get nibbles 0..3.
  - 1 bpp: F900 = F902 = nibble 0, F904 = F906 = nibble 1.
- **ROM default** when the NVRAM is bad (ROM:067C, AX=F4F4h): F900=4, F902=F, F904=4, F906=F. That is a red screen with white text in 1 bpp mode.
- **ICON2 emulation:** in 1 bpp mode it swaps F902 and F906 into its 4-entry table (icon_emul 0xB54/0xBA8).
- Only the reading "**1 bpp pixel p uses colour index 3·p**" (pixel 0 → F900, pixel 1 → F906) is consistent with all three writers [I].

**Colour value encoding = CGA RGBI** [C, two independent tables]:
- Kernel `int 5Ch` function 11h (get colour, kern:765A, table at kern:76CD), with full scale 7D00h:
  - 1 = (0, 0, 3E80)
  - 2 = (0, 3E80, 0)
  - 4 = (3E80, 0, 0)
  - 7 = 4E20 grey
  - 8 = 2710 grey
  - F = 7D00 white
- icon_emul's ICON→ICON2 colour table (file 0xBFE), as 12-bit RGB:
  - `0:000 1:00A 2:090 3:0BA 4:B00 5:909 6:990 7:999 8:333 9:00F A:0F0 B:0FF C:F00 D:F0F E:FF0 F:FFF`
  
So **bit0 = Blue, bit1 = Green, bit2 = Red, bit3 = Intensity**: 0–7 are ~60 % levels, 8 is dark grey, 9–F are full. Use the icon_emul table for rendering.

**Monochrome:**
- `int 5Ch` function 0Eh (set colour, kern:748B) maps a 2-bit-per-component RGB to a nibble through the 64-byte table at kern:755D. It uses the high nibble (a grey level) when `sys+6Eh` bit 2 is set [C].
- FA8A (latch) is 0 for B/W and 1 for colour monitors [C: win_mgr 0x4EC7/0x4EFB; dev:2C93 writes `~sys+6Eh & 1`]. **[I]** It selects the composite/mono vs RGB output.

### 4.4 6845 CRTC [C]

The ROM loads R0–R9 from NVRAM words 5–9: low byte → R(2k), high byte → R(2k+1) (ROM:08B2). When the NVRAM is missing, it uses the default table at ROM:0671 (file 0xF61):

| R0 | R1 | R2 | R3 | R4 | R5 | R6 | R7 | R8 | R9 |
|---|---|---|---|---|---|---|---|---|---|
| 38h | 28h | 2Ch | 44h | 41h | 00 | 3Ch | 3Dh | 00 | 03 |

Reading these: 40 character clocks of 16 px (2 bytes) per line; 60 rows × 4 scan lines = **240 lines**; 66 × 4 = 264 total lines.
- **[I]** With a 14.318 MHz dot clock (twice the CPU clock): 15.70 kHz line rate, **59.5 Hz frame rate**.
- R12/R13 (start address) are only exercised by the register test (ROM:061F) and stay at 0.
- **Vertical sync** (or a display-timing signal) is readable at **PPI A PC7 (F985 bit 7)**: ROM error 0F "No sync transitions" polls it at ROM:06AF. icon_emul fakes it by toggling bit 7 on every read (icon_emul 0x4C6) [C].

### 4.5 Text mode

**There is no hardware text mode** [C]. `$con` draws an 8×10 font in software into the 640×240×1 page, giving **80×24 cells**.
- Glyph format: 10 bytes per char. Byte 0 is the bottom scan line: rows 0–2 are descender space, rows 3–9 run bottom to top. LSB is the left pixel.
- Font in ws_boot img ≈ 0x15C73 + 10·code (e.g. 'H' at 0x15F43). The ROM's copy is at file 0x2CF4 + 10·(code − 20h).
- "graph drivers 2.40" (`/sys/GRAPHICS.slib` = `/sys/driver_lib`) has **no port I/O at all** [C]. All drawing is plain RAM writes.

### 4.6 Speaker (bell) [C]

PPI A **PC3** is toggled by bit-set/reset on F987 (07h/06h, 150 cycles with a delay loop) at dev:33C3. icon_emul maps PC3 writes to the ICON2 sound port F7E0 (0x56C/0x577).

---

## 5. What the ICON2 emulator (`icon_emul`) reveals

`icon_emul` (4505 B) runs on an ICON2. It traps the ICON's I/O ports through ICON2 trap hardware: registers F720–F728, the trap interrupt on vector 2 (NMI, `[0:0008]`), and the trap table at code 0x2D2 [C].

| Range | Handler | What it emulates |
|---|---|---|
| F981–F987 | 0x3D3 | PPI A |
| F900–F906 | 0xB1A | palette |
| FF20–FFFF | 0x39F | 80186 peripheral block, forwarded to port − 0x900 |
| F920 | 0xA5B | keyboard; data comes from **F7F2**, writes ack F7F2 |

INT 64h is hooked too. Facts confirmed for the ICON:
- **Microwire bus** on PC0/PC1/PC6 with device selects F983 bits 7..5. It emulates both the NVRAM (93C06 READ `10aaaaaa` and WRITE `01aaaaaa`, 16 words) and the two counter chips (table in §3.1, decoder at 0x5E4).
- **F983 bit0** = display page flip, **bit2** = 1 bpp/2 bpp mode, **bits 5–7** = Microwire select. Bits 1, 3 and 4 are not emulated.
- **F985 bit7** toggles (sync). **F985 bit6** = Microwire DO.
- **PC3** = speaker.
- **Palette nibbles are RGBI** (table above).
- **[I] ICON2 CPU:** the ICON2 has an 80186 peripheral control block relocated to **F600h**. FFxx accesses are forwarded to FFxx − 0x900, which suggests an 80186-family CPU. This answers part of HW-1.
- **ICON2 video registers:** F700 (mode, low 3 bits), F708 (display base = segment − 2; a 4-word palette header sits at seg−2:0000), F7E0 (sound).

---

## 6. Open questions

1. **Keyboard typematic tick rate and power-up sequence:** the exact bytes and timing the 8035 MCU sends. The 8035's program is on an external EPROM that hasn't been dumped. The protocol above is what the host accepts.
2. **2 bpp pixel bit order** within a byte, and which bit of the 2-bit value is the MSB. No table was found in GRAPHICS.slib. Check against a real screenshot or the ICON2 F700 mode semantics.
3. **Hardware video address generation** (6845 MA/RA → DRAM) and why the pages sit at bank + ABA0h / 2BA0h. Only the software convention is known.
4. **F983 bits 1, 3 and 4; PC2; FA8A exact function.** Bit 1 is pulsed for a COM9026 reset; bit 3 is cleared at init.
5. **The "baud-rate counter" chip** select code. Probably F983 bits 7..6 = 11.
6. **The 3Fh/29h/35h counter set-up commands.** COP452 mode bits; they don't matter for emulation if counters free-run.
7. **RESET key #016:** whether it is wired to the CPU RESET or NMI line.
