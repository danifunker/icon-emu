# icon-emu

An emulator of the Unisys/CEMCORP **ICON** school computer and its **LEXICON** fileserver (Ontario, 1980s). It is written in Rust and runs in the browser through WebAssembly, or headless from the command line.

It emulates the 80186 board: video, keyboard, trackball, speaker, NVRAM, battery clock, WD1002 hard-disk/floppy controller, and the ARCNET (ICONET) network. That is enough to boot the original boot ROM and QNX 2 (ICON system software 4.10.01) from the original diskettes, install the system to a hard disk, and network-boot workstations from the fileserver.

**Try it in your browser: <https://danifunker.github.io/icon-emu/>** (you'll need your own ROM; see below).

**No ROM or system software is included.** You need your own dump of the ICON boot ROM (EPROMs 5053-1 and 5053-2) and your own disk images.

## Quick start

```
make web        # build the WebAssembly page into web/ (copies a local ROM into web/rom/ if you have one)
make serve      # http://localhost:8086
make cli        # target/release/icon-cli and icon-ci
```

- In the page, click **Load ROM…** and pick the 32 KB ROM image, or both 16 KB EPROM dumps at once. The ROM is kept only in your browser. Untick **Use the same ROM for all devices** to give the LEXICON and the ICONs different ROMs.
- **Add ICON node** adds more workstations to the network.
- Click a screen to capture the keyboard and mouse; **Ctrl+Alt** releases them.
- Then add a hard disk, insert the boot diskette and power on the fileserver.
- [docs/INSTALL.md](docs/INSTALL.md) walks through a full 4.10.01 install.
- [docs/HEADLESS.md](docs/HEADLESS.md) covers headless and scripted use (`icon-cli serve` with the `icon-ci` client).

## Documentation

| Document | Contents |
|---|---|
| [docs/REBUILD.md](docs/REBUILD.md) | Step-by-step rebuild of a whole network: which disks, in which order (4.10.01, Help, QDOS, users) |
| [docs/ADMIN.md](docs/ADMIN.md) | Running it: paths and drives, user accounts, `.login`, House, shutting down |
| [docs/INSTALL.md](docs/INSTALL.md) | Installing ICON 4.10.01 onto 10 MB and 64 MB hard disks in detail, adding packages |
| [docs/HEADLESS.md](docs/HEADLESS.md) | Control-socket mode for tests and tools |
| [docs/UI.md](docs/UI.md) | The web page |
| [docs/com9026.md](docs/com9026.md), [docs/input_video.md](docs/input_video.md), [docs/nvram_rtc_disk_install.md](docs/nvram_rtc_disk_install.md) | Hardware reverse-engineering notes |

## License

The emulator is released under the **MIT License**; see [LICENSE](LICENSE).

The 8086/80186 CPU core (`crates/icon-core/src/cpu/i86.rs`, `i186.rs`) and the TMS5220 speech chip (`crates/icon-core/src/devices/tms5220.rs`) are ports of MAME code. They remain under the **BSD 3-Clause License**; see [THIRD_PARTY.md](THIRD_PARTY.md). The project as a whole is therefore `MIT AND BSD-3-Clause`.

Neither license covers the ICON ROM, QNX, or any Unisys/CEMCORP software. `disks/hd64-factory.bin` is a blank, factory-prepared 64 MB disk template. It holds blocks written by the ICON's own `hdformat`/`fdisk` tools, including part of the QNX hard-disk loader, and is not covered by either license.

## Credits

- **MAME**: the 8086/80186 CPU core is ported from MAME's `src/devices/cpu/i86`.
  - Core author: Carl. Earlier i86 work: Bryan McPhail, Oliver Bergmann, Fabrice Frances and David Hedley.
  - 80186 peripherals: Phill Harvey-Smith (rmnimbus driver), building on the Leland sound driver by Aaron Giles and Paul Leaman.
  - The TMS5220C speech synthesizer is ported from MAME's `tms5220.cpp`: Frank Palazzolo, Aaron Giles, Jonathan Gevaryahu, Raphael Nabet, Couriersud and Michael Zapf.
  - <https://github.com/mamedev/mame>
- **SingleStepTests** by Daniel Balsom (MIT): the 8086/80286 test suites used to check the CPU core. They are downloaded by `scripts/fetch_cpu_tests.py` and not included. <https://github.com/SingleStepTests>
- **York University Computer Museum (YUCoM)**: preserved and supplied the ICON ROM dumps, manuals and system diskettes this work is based on.
- **Alan's Hyperion emulator** inspired the look of the web page, with each machine drawn in HTML/CSS. <https://github.com/alanswx/Hyperion>
- **Standard Microsystems (SMSC)**: the COM9026 datasheet, used to model the ARCNET controller.

ICON, LEXICON and Unisys are trademarks of their respective owners. QNX is a trademark of BlackBerry Limited (originally Quantum Software Systems). This project is not affiliated with any of them.
