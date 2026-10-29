# icon-emu

An emulator of the Unisys/CEMCORP **ICON**, the networked school computer used in Ontario in the 1980s, and its **LEXICON** fileserver. It runs in your web browser, or from the command line with no window.

It emulates the ICON's 80186 main board well enough to run the original boot ROM and the ICON's operating system (QNX 2, ICON system software 4.10). You can install the system onto a hard disk from the original diskettes, network-boot workstations from the fileserver, log in, and use the system as a school would have.

**Try it in your browser: <https://danifunker.github.io/icon-emu/>**

**No ROM or system software is included.** You need your own copy of the ICON boot ROM and the ICON system diskettes, as disk images. The York University Computer Museum's set is the one these instructions follow.

## Getting started

1. Open the page and click **Load ROM…**. Pick the 32 KB ROM image, or select both 16 KB EPROM files at once. The ROM stays in your browser; it is never uploaded.
2. Follow [the rebuild guide](docs/REBUILD.md) to install the system onto a hard disk and boot a workstation.
3. [The web page guide](docs/UI.md) explains every button, including **Files**, which copies files between your computer and the ICON's disks.

## Documentation

| Guide | What it covers |
|---|---|
| [Using the web page](docs/UI.md) | The controls: ROMs, memory, power, keyboard and trackball, diskettes, hard disks |
| [Rebuilding a network from scratch](docs/REBUILD.md) | Which diskettes to use, in which order, from an empty hard disk to a working network with student accounts |
| [Installing in detail](docs/INSTALL.md) | The install, step by step, for 10 MB and 64 MB hard disks, and adding packages |
| [Administrator notes](docs/ADMIN.md) | Drives and paths, user accounts, moving files in and out, speech, MS-DOS, shutting down |
| [Headless mode](docs/HEADLESS.md) | Running and scripting the emulator from the command line |

The hardware notes ([COM9026 network chip](docs/com9026.md), [keyboard, trackball and display](docs/input_video.md), [NVRAM, clock and disk](docs/nvram_rtc_disk_install.md)) are reference material for developers.

## Building it yourself

You need Rust, with the `wasm32-unknown-unknown` target and `wasm-bindgen-cli` for the web page.

```
make web        # build the web page into web/
make serve      # build it and serve it at http://localhost:8086
make cli        # build the command-line tools: target/release/icon-cli and icon-ci
make test       # run the tests
```

If you put your ROM at the path in the `Makefile`, `make web` copies it into the local page so you don't have to load it each time. It is never published.

## License

The emulator is released under the **MIT License**; see [LICENSE](LICENSE).

The 8086/80186 CPU core (`crates/icon-core/src/cpu/i86.rs`, `i186.rs`) and the TMS5220 speech chip (`crates/icon-core/src/devices/tms5220.rs`) are ports of MAME code. They remain under the **BSD 3-Clause License**; see [THIRD_PARTY.md](THIRD_PARTY.md). The project as a whole is therefore `MIT AND BSD-3-Clause`.

Neither license covers the ICON ROM, QNX, or any Unisys/CEMCORP software.
- `disks/hd64-factory.bin` is a blank, factory-prepared 64 MB disk template. It holds blocks written by the ICON's own `hdformat`/`fdisk` tools, including part of the QNX hard-disk loader, and is not covered by either license.
- The blank-diskette template (`crates/icon-core/src/devices/fd640_blank.bin`) is an empty QNX directory and bitmap, as `dinit` writes them.

## Credits

- **MAME**: the 8086/80186 CPU core is ported from MAME's `src/devices/cpu/i86`.
  - Core author: Carl. Earlier i86 work: Bryan McPhail, Oliver Bergmann, Fabrice Frances and David Hedley.
  - 80186 peripherals: Phill Harvey-Smith (rmnimbus driver), building on the Leland sound driver by Aaron Giles and Paul Leaman.
  - The TMS5220C speech synthesizer is ported from MAME's `tms5220.cpp`: Frank Palazzolo, Aaron Giles, Jonathan Gevaryahu, Raphael Nabet, Couriersud and Michael Zapf.
  - <https://github.com/mamedev/mame>
- **SingleStepTests** by Daniel Balsom (MIT): the 8086/80286 test suites used to check the CPU core. They are downloaded by `scripts/fetch_cpu_tests.py` and not included. <https://github.com/SingleStepTests>
- **York University Computer Museum (YUCoM)**: preserved and supplied the ICON ROM dumps, manuals and system diskettes this work is based on.
- **Alan's Hyperion emulator** inspired the look of the web page, with each machine drawn in HTML/CSS. <https://github.com/alanswx/Hyperion>
- **[qnx2fs](https://crates.io/crates/qnx2fs)** (MIT) reads and writes the ICON's QNX 2 disks for the Files panel. It is shared with RustyBackup.
- **Standard Microsystems (SMSC)**: the COM9026 datasheet, used to model the network controller.

ICON, LEXICON and Unisys are trademarks of their respective owners. QNX is a trademark of BlackBerry Limited (originally Quantum Software Systems). This project is not affiliated with any of them.
