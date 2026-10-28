# Web UI Plan

The goal is a browser page showing **two separate machines side by side, linked by an ICONET coax cable**: an **ICON workstation** and a **LEXICON fileserver**. Each runs its own emulated 80186 board in the same WebAssembly module, and the two are joined by an emulated ARCNET segment. This follows the style of Alan's [Hyperion emulator](https://github.com/alanswx/Hyperion), where the machine itself is drawn in HTML/CSS, but the drawings match the ICON hardware. Reference photos are in `icon_icon2_research/web_archive/_hardware_photos/`.

## The machines

### LEXICON fileserver (built first: it's what the install needs)
- A low box with these features:
  - **5¼" floppy drive** with a turn-down latch and an activity LED;
  - **red hard-disk LED**;
  - **yellow trouble LED** (it blinks the self-test codes, driven by I/O FA88);
  - **power switch with lamp**.
- **Console monitor:** an optional small CRT. The real LEXICON drives its video only during boot, for troubleshooting. Show it in a collapsible panel.
- **Power button** (cold boot) and a **reset** (front-panel or keyboard reset).
- **Floppy controls:**
  - pick a bundled release diskette, or **Load .img**;
  - eject;
  - a write-protect toggle;
  - drag-and-drop a file onto the drive to mount it.
- **Hard disk controls:**
  - **New hard disk**: creates a blank, unformatted 10 MB (306/4/17) or 64 MB drive. Formatting and installing happen inside the emulated OS (`configure` / `dinit`), as they did for real;
  - **Load .img**;
  - **Download .img**: saves the current disk to your computer;
  - **Autosave** to browser storage, so the disk survives a page reload.

### ICON workstation (added once the fileserver boots)
- The pedestal unit, with:
  - the **CRT** (amber or colour) on top;
  - a **built-in keyboard** with a **trackball**, and the CUR LOCK / CAP LOCK lights;
  - the **amber power toggle** on the pedestal;
  - the volume knob and headphone jack (speech output later).
- **Power button** and **RESET** (the ICON's RESET + CTRL + SHIFT combination).
- Keyboard input goes to whichever machine has focus. The trackball maps to the mouse (pointer lock while the screen is clicked).
- A diskless station needs no disk controls. An optional "solo ICON" mode later adds the pedestal floppy.

### Between them
- A drawn **coax cable** with BNC terminators. Its activity flickers when packets pass.
- Per-machine **NVRAM** settings (node ID, boot server, boot options), editable in a settings drawer. These replace the real `attach_net` factory setup.

## How saving works in a browser

The emulator keeps each disk image in memory as a byte array. To persist it:
1. **Autosave:** the page writes changed disk images to the browser's private storage (the Origin Private File System, or IndexedDB) every few seconds and when the page is hidden. On reload, the last disk comes back automatically. This storage belongs to this site in this browser only.
2. **Download .img:** creates a file from the in-memory image (a `Blob` download), so you can keep it, share it, or open it with RustyBackup. That's how you'd take a freshly installed fileserver disk away.
3. **Load .img:** picks a file from your computer and replaces the disk, e.g. to restore a download or start from a supplied image.

NVRAM contents (16 words per machine) are saved the same way, since `attach_net` and `configure` write to them.

Later, we'll ship ready-made starting images (an installed 4.10.01 fileserver disk) as bundled downloads.

## Build order

| Phase | UI deliverable |
|---|---|
| 1 | None. A headless CLI runs the ROM self-test |
| 2 | The fileserver panel: power, console screen, floppy drive, new/load/download hard disk, autosave |
| 3 | The second machine (ICON workstation) and the coax link. The workstation network-boots to `Login:` |
| 4 | Keyboard and trackball input, for logging in and driving `configure` |
| 5 | Full install, then boot from the hard disk. Ship a starting image |
