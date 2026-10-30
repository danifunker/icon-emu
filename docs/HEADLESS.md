# Headless mode (`icon-cli serve` + `icon-ci`)

Headless mode runs an emulated ICON network with no window: a LEXICON fileserver in slot 0 and workstations in slots 1 and up. You drive it with commands from the `icon-ci` client, or from any program, through a Unix socket.

Emulated time only advances while a command runs, so a script gives the same result every time. That makes it useful for automated tests, and for tools such as RustyBackup that need a real ICON system to check the disks they write.

## Build

```
make cli          # target/release/icon-cli and target/release/icon-ci
```

The boot ROM is not included. Give it with `--rom FILE`: the 32 KB image, which is the two 16 KB EPROMs interleaved (5053-1 holds the even bytes, 5053-2 the odd).

## Start the server

```
icon-cli serve [--socket /tmp/icon.sock] [--rom FILE] [--fs-rom FILE] [--ws-rom FILE] [--hd IMG] [--floppy IMG] [--workstations N] [--ram-kb 512|1024] [--nvram SLOT=FILE ...]
```

- The socket can also be set with `ICON_CI_SOCKET`. Keep the path short, because Unix socket paths are limited to about 100 bytes.
- `--rom` is the boot ROM for every machine. `--fs-rom` and `--ws-rom` give the fileserver or the workstations a different one.
- The server reads `--hd` into memory. Your file is never written unless you send `hd-save`.
- With no floppy, the fileserver boots from the hard disk. With a boot floppy, it boots from the floppy, and the hard-disk partitions become drives 2 and 3.
- `--ram-kb 1024` gives every machine the 1 MB upgrade card. It needs the Rel 2.11 ROM.

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
| `floppy-new` | Insert a blank 640K diskette, already formatted for QNX |
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
| `nvram SLOT` | The machine's NVRAM settings, as the ICON's `nv_config` reports them, and its 16 words |
| `nvram SLOT set NAME=VALUE…` | Change settings, e.g. `nvram 1 set node=20 loaders=network`. The ROM reads them at the next power-on |
| `nvram SLOT load FILE`, `save FILE`, `reset` | Use a 32-byte image, save one, or go back to the built-in settings |
| `quit` | Stop the server |

### Files on the fileserver's disks

These copy files between your computer and the ICON's disks directly, without typing commands into QNX. Useful in CI to load test data before booting, and to collect results afterwards.

| Command | What it does |
|---|---|
| `ls DISK [PATH]` | List a folder (default `/`) |
| `get DISK PATH HOSTFILE [--text]` | Copy a file to your computer |
| `put DISK HOSTFILE PATH [--text] [--replace]` | Copy a file onto the disk |
| `mkdir DISK PATH`, `rm DISK PATH` | Make a folder; delete a file, or a folder with everything in it |
| `cp DISK PATH TODISK DIR [--replace]` | Copy a file or folder into `DIR` on a disk, keeping permissions, owners and dates (e.g. `cp fd /util hd /util`) |

- `DISK` is `fd` (the diskette), `hd` (the hard disk's first partition), or `hd2` / `hd3` (its other partitions on a 64 MB disk).
- **Reading** works while the machine runs. It sees the disk as QNX has written it so far; something QNX still holds in memory may not be there yet.
- **Writing the diskette** works at any time. The server takes the diskette out, changes it and puts it back, like swapping it by hand. Don't do it while QNX is writing to it.
- **Writing the hard disk** only works while **every machine is off** (`power N off`, or before the first `power`). The fileserver caches the disk and would overwrite the change, so the server refuses while anything is on, and says which slots are.
- `--text` converts text both ways: UTF-8 with LF line ends on your side; RS (0x1E) line ends, Telidon accents (é is `0x19 0x42 e`), code page 437 and the underline codes `0x99` / `0x9A` on the ICON's. Characters the ICON can't store are refused. Binary files are left alone.
- File names must fit QNX: 1 to 16 of `A-Z a-z 0-9 _ . $`.
- Files are dated with the emulated clock: today's date in 1994, or 1992 on 29 February, at your local time (see the administrator notes, Dates). Files copied with `cp` keep their dates.
- `floppy-new` gives you a formatted diskette to `put` files on.

### Without a running machine: `icon-cli disk`

`icon-cli disk` does the same on a disk image file directly, with no server and no ROM. Changes are written back to the image.

```
icon-cli disk IMAGE [--part N] ls [PATH]
icon-cli disk IMAGE [--part N] cat PATH
icon-cli disk IMAGE [--part N] get PATH HOSTFILE [--text]
icon-cli disk IMAGE [--part N] put HOSTFILE PATH [--text] [--replace]
icon-cli disk IMAGE [--part N] mkdir PATH
icon-cli disk IMAGE [--part N] rm PATH
icon-cli disk IMAGE [--part N] check
icon-cli disk IMAGE [--part N] cp PATH TO_IMAGE DIR [--to-part N] [--replace]
icon-cli disk IMAGE [--part N] fix-watcom
```

`fix-watcom` patches Watcom Pascal 3.00's `wedit` and `pascal_m`, which show a blank screen on system 4.10 (see the administrator notes).

`cp` copies between two images (or within one), keeping permissions, owners and dates. `get` gives the saved file the ICON file's date.

`--part` picks a hard-disk partition (default the first). `cat` prints a text file decoded (accents, line ends). `check` checks the filesystem and exits with 1 if it finds errors. It is handy in CI to prepare a disk before `icon-cli serve --hd`, or to inspect one afterwards.

For example, to load data, run a program on it and fetch the result:

```
put hd input.txt /user/icon/input.txt --text
power 0
login
run 'sort 1:/user/icon/input.txt >1:/user/icon/sorted.txt'
get hd /user/icon/sorted.txt sorted.txt --text
```

### NVRAM

Each slot starts with built-in NVRAM settings (fileserver node 1; workstations node 15 + slot, booting from node 1). `--nvram SLOT=FILE` or `icon-ci nvram SLOT load|set` gives a slot its own NVRAM. It is used exactly as it is at power-on, so its node number, boot order and memory size apply. It keeps whatever the ICON writes to it across power cycles, for as long as the server runs. Save it with `nvram SLOT save FILE`.

An image file is 32 bytes: 16 words, each **low byte first**, as an EPROM programmer saves the ICON's NMC9306 chip. `icon-cli nvram` reads and edits one with no machine running:

```
icon-cli nvram FILE                       # show the settings
icon-cli nvram FILE set node=1 server=1 loaders=floppy,hd
icon-cli nvram FILE new fileserver|workstation [NAME=VALUE ...] [--force]
icon-cli nvram FILE hex [W0 ... W15]      # print or replace the raw words
icon-cli nvram FILE diff OTHER            # what differs from another image
icon-cli nvram --fields                   # every NAME and its values
```

The names are `loaders` (any of `floppy,network,hd,rom`, tried in that order), `board` (`fileserver` or `workstation`), `ram`, `local-floppy`, `serial-ports`, `rtc`, `keyboard`, `speech`, `node`, `server`, `boot-file`, `monitor`, `hw-serial`, `sw-serial`, `palette` and `crtc`. `wN=HEX` sets a raw word and `wN.B=0|1` sets one bit. [NVRAM, clock and disk](nvram_rtc_disk_install.md) §1 has the full map.

## Protocol

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
