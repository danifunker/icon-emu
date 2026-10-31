# The printer: parallel port, Panasonic KX-P1091, PDF

The emulated fileserver has a 9-pin dot-matrix printer on its parallel port, drawn as a **Panasonic KX-P1091**. System software 3.00 supported two printers, the Epson FX-80 and the Panasonic KX-P1091 (4.10 adds some Unisys models), and both speak Epson's ESC/P, which the emulator interprets. What the ICON sends is captured byte for byte and turned into a PDF in one of two looks.

## What schools used

The documentation names only the supported models. Accounts from the time fill in the rest:
- A teacher who ran an ICON lab: "We also had a shared printer, a Panasonic 1091. … the LexICON had a parallel printer port and a software print queue. Print jobs would go and sit in the queue and be printed on a first in, first out basis" (Doug Peterson, *Whatever happened to …*, 2022).
- Students: "the one dot-matrix printer in the network room, then tear the holes off the continuous feed paper"; "a server called Lexicon … and a standard dot matrix printer" (r/canada, 2017).
- Jason Eckert: the LEXICON "sat on a desk in the corner of the room next to a shared printer", which everyone printed to over ICONET.

So: one 9-pin dot matrix per lab, usually a Panasonic KX-P1091 or Epson FX-80, on tractor-feed paper, shared by the whole class through the fileserver's queue. Copies of these pages are in the research repository's `web_archive/`.

## The two looks


- **Authentic**: 9.5-inch continuous tractor-feed paper (sprocket holes, perforations, plain or green bar), every character printed as 9-pin dots, ribbon ink that varies along the ribbon and from pin to pin, and the small offset of bidirectional printing. A worn ribbon prints lighter and less evenly.
- **Clean**: Letter pages with the text in Courier, selectable and searchable. Accents the ICON prints by overstriking (`'`, backspace, `e`) become one letter (`é`).

Screen dumps (`/sys/print_screen`) come out sideways as bit images, as they did on paper.

## Using it

Set the printer up once, as `root` on the fileserver:

```
$ attach_printer n=1 d=3 t=p1091
```

(or just `attach_printer` and answer: device 3 is `$plpr`, the parallel port; type `p1091`, or `fx80` for the Epson setup, which works the same). If it answers `Node 1 not found in network`, the hardware database doesn't list the fileserver yet: run `verify_net` (answer `n` to removing any node), then `attach_printer` again.

Then, from any machine:

| Command | Prints |
|---|---|
| `print FILE` | A text file, with a header (date, time, name, page) |
| `print -h FILE` | Without the header. Options: `+b` banner page, `+e` emphasized, `w=132` condensed, `s=8` eight lines per inch |
| `/sys/print_screen` | The screen, as a bit image |

A job is finished when the printer has had nothing for 3 emulated seconds.

- **Web page**: the printer sits under the fileserver. Paper feeds out of it while it prints; finished printouts land in the tray below, kept in the browser. Click one for the PDF. **Look**, **Paper** and **Ribbon** apply to every printout, old ones included. **ON LINE** takes the printer off line: the ICON then waits, as with a real printer.
- **Headless**: `icon-ci printouts`, `icon-ci printout N FILE.pdf [--style clean|authentic] [--green-bar] [--worn]`, `icon-ci printout-raw N FILE` (the bytes), `icon-ci printer SLOT on|off`. Offline: `icon-cli printout JOB.bin OUT.pdf [same options]`.

Workstations have no printer (their `list` and `slist` then wait); `icon-ci printer SLOT on` gives one a printer.

## The hardware (traced in the emulator)

Kernel code at 1E7F2 (send a byte) and 1E897 (INT 61h) in the 4.10.01 `hd_boot`:

| Signal | Where |
|---|---|
| Data | **PPI B port B** (FAC3). PPI B is set to `8C`: port B is a **mode 1 strobed output** |
| STROBE | **PPI A port C bit 2**: `out F987,04` then `out F987,05` (low pulse) |
| BUSY | **PPI B port C bit 6** (FAC5), high = busy. The kernel polls it after each byte and from its timer |
| ACK | Sets the 8255's INTR_B (**PPI B PC0**) when INTE_B (PC2, `out FAC7,05`) is on. INTR_B is wired to **INT2**, whose handler polls FAC5 bit 0 and calls **INT 61h**, which writes port B back to clear it |

The ROM's test 1e ("Line printer interrupt present on PPI B (pc0)") checks that PC0 is clear with INTE off. At start-up the ICON strobes out one NUL; the emulator drops jobs of only NULs.

## What the ICON sends

From `print` (4.10.01) with `/config/print/PAT.fx80`:

1. `ESC : 0 0 0` copies the ROM characters to RAM, `ESC % 1 0` selects them, and 13 `ESC & 0 c c` downloads replace some codes: lowered capitals for accented capitals (C1 A, C5 E, C9 I, CF O, D5 U), accent pieces (5E circumflex, C0 cedilla, DE diaeresis), and « » ÷ × ß (BC, BE, AD, AA, C2). Each is an attribute byte (bit 7 clear = use the lower 8 pins) and 11 half-dot columns.
2. Text: `DC4 DC2` (pica, 80 columns) or `DC4 SI` (condensed, 132), `ESC 2` / `ESC 0` line spacing, `ESC E/F`, `ESC G/H`, CR LF, FF.
3. Accents are overstruck: `'` BS `e` = é, `` ` `` BS `a` = à, `^` BS `o` = ô, `C0` BS `c` = ç, `` ` `` BS `C1` = À.

`print_screen` with the Epson setup sends `ESC A 8`, then per band `ESC * 4 E0 01` (480 columns at 80 dpi, top pin = MSB) and CR LF, and ends with `FF ESC @` (`/config/snap.init.fx80`). The Panasonic setup (`snap.init.p1091`) sends `ESC 1` and 640-column bands with no CR LF: each band fills the 8-inch line, and the printer feeds by itself.

The two text setups differ only in the pitch codes: the Panasonic file sends `DC2` / `SI` alone where the Epson one sends `DC4 DC2` / `DC4 SI`.

## The printer model (`crates/icon-printer`)

- `escp.rs` interprets ESC/P as an FX-80: pica, elite, condensed, expanded (SO, `ESC W`), emphasized, double-strike, italic (also codes A0–FE), underline, super/subscript, `ESC !`, the international sets (`ESC R`), line spacing (`ESC 0 1 2 3 A J j`), page length (`ESC C`), skip-over-perforation, margins, tabs, download characters (`ESC : % &`), bit images (`ESC K L Y Z * ^ ?`), `ESC @`, DC1/DC3. Positions are in 1/240 inch across and 1/216 inch down; a page is 66 lines of 1/6 inch unless the program sets otherwise.
- The ROM characters are Adafruit's classic 5×8 font (BSD, see THIRD_PARTY.md) on the even half-dot columns, standing in for the FX-80's ROM.
- `render.rs` writes the PDF with `pdf-writer`: paper as vector graphics, ink as one image per page (an ink-coloured image with a soft mask, cropped to the dots), text in the PDF base font Courier for the clean look.
