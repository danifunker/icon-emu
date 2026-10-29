# Headless mode (`icon-cli serve` + `icon-ci`)

This works like iris's `--ci` mode. One process runs the emulated ICONET segment with no window: a LEXICON fileserver in slot 0 and workstations in slots 1 and up. Other programs drive it through a Unix control socket. Emulated time advances only while a command runs, so a scripted session gives the same result every time. That makes it suitable for regression tests, and for tools such as RustyBackup that need real QNX 2 to check what they wrote to a disk.

## Build

```
make cli          # target/release/icon-cli and target/release/icon-ci
```

The boot ROM is not in this repo. `serve` reads `--rom FILE`, and defaults to `~/repos/icon_icon2_research/firmware/ICON1/icon1_rom_combined.bin`. That file is the two 16 KB EPROMs interleaved: 5053-1 holds the even bytes, 5053-2 the odd. SHA-1 `5d7f7b17…`.

## Start the server

```
icon-cli serve [--socket /tmp/icon.sock] [--rom FILE] [--fs-rom FILE] [--ws-rom FILE] [--hd IMG] [--floppy IMG] [--workstations N]
```

- The socket can also be set with `ICON_CI_SOCKET`. Keep the path short, because Unix socket paths are limited to about 100 bytes.
- `--rom` is the boot ROM for every machine. `--fs-rom` and `--ws-rom` give the fileserver or the workstations a different one.
- The server reads `--hd` into memory. Your file is never written unless you send `hd-save`.
- With no floppy, the fileserver boots from the hard disk. With a floppy, it boots from the floppy, and the hard-disk partitions become drives 2 and 3.

## Drive it

```
icon-ci [--socket PATH] COMMAND ...
icon-ci script FILE [--keep-going]     # one command per line, '#' comments
```

| Command | What it does |
|---|---|
| `ping`, `status` | Liveness check; slots, which are powered, packets, emulated seconds |
| `hd-load IMG`, `hd-new [10mb\|64mb]` | Disk used at the next fileserver power-on (`hd-new 64mb` is a blank drive; use `disks/hd64-factory.bin` + `hd-load` for a prepared one) |
| `floppy IMG`, `eject` | Insert or eject the fileserver's diskette (takes effect immediately if it is on) |
| `power SLOT [off] [--no-wait]` | Power a node on and wait for `Login:` (or turn it off) |
| `add-workstation` | Add a workstation slot; it becomes node 15 + slot |
| `login [SLOT] [USER] [PASS]` | Default `root` / `superuser`; waits for the shell prompt |
| `run [--slot N] [--timeout S] COMMAND…` | Type a shell command, wait for the next prompt, print what it output |
| `type SLOT 'TEXT'` | Raw typing (`\r` = Enter, `\e` = Esc) |
| `key SLOT HEX` | Raw ICON key code (bit 7 = release), e.g. `45` = left ACTION |
| `mouse SLOT DX DY`, `click SLOT left\|right` | Trackball |
| `wait-text SLOT TEXT [TIMEOUT]` | Wait until the text appears on that screen |
| `idle SECONDS` | Let emulated time pass |
| `screen SLOT`, `transcript SLOT` | Decoded console: the current screen, or everything since power-on (scrolling merged) |
| `screenshot SLOT FILE.ppm` | Screen image (640×240 PPM) |
| `hd-save IMG`, `floppy-save IMG` | Write the fileserver's disks, including everything QNX wrote |
| `quit` | Stop the server |

The protocol is one JSON object per line: `{"cmd":"run","args":{"slot":0,"command":"query"}}`. Each request gets one reply line, `{"ok":true,"data":…}` or `{"ok":false,"error":"…"}`. Any language can talk to the socket directly. Errors that are timeouts include the screen as decoded at that moment.

## Example

```
icon-cli serve --socket /tmp/icon.sock --hd disks/lexicon-4.10.01-10mb.img &
icon-ci script scripts/ci-smoke.txt
```

`scripts/ci-smoke.txt` boots the fileserver and a workstation and runs a few commands on each. It takes about 40 s of wall time.

From Rust, the same API is available in-process as `icon_core::session::Rig` (several nodes) and `Session` (just a fileserver). `icon-cli session --hd IMG --cmd "chkfsys 1"` runs one-shot commands with no server.

## How the console is read

Text is decoded from the frame buffer, not from a serial line:

- Each 8×10 cell is matched against the running kernel's font table, which is found in RAM. The ROM font is the fallback.
- Rows are located one at a time, because the workstation login screen puts its banner and its prompt on different line grids. The 4.10 workstation kernel also draws glyphs one pixel to the left.
- `?` marks a cell that couldn't be decoded, such as the cursor or graphics. A `_` may also be the cursor.
- `run` output therefore matches the real output exactly for plain text. Very fast scrolling can still drop or garble a line now and then, so tests should check short, deterministic commands (`query`, `date`, `files DIR`, `chkfsys N`) rather than long listings.
