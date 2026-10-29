# Using the web page

The page shows one ICON network: a **LEXICON fileserver** on the left, and one or more **ICON workstations** joined to it by the network cable (ICONET). Each machine is a separate emulated computer.

Everything you load stays in your browser. Nothing is uploaded.

## Boot ROM

Every machine needs the ICON boot ROM before it can be switched on. At the top of the page:

1. Click **Load ROM…**.
2. Pick the 32 KB ROM image, or select **both** 16 KB EPROM files at once (5053-1 and 5053-2).

The page names the ROM when it recognises it:
- **stock 5053 ISS 2** is the original ROM.
- **Rel 2.11** is the later ROM. Workstations with it need system software **4.10.05** on the fileserver (see [the rebuild guide](REBUILD.md), section 3).

The ROM is remembered for next time. **Forget** removes it.

To give the fileserver and the workstations different ROMs, untick **Use the same ROM for all devices**. A ROM row then appears for each kind of machine.

## Memory

**Memory** sets how much memory each machine has the next time it is switched on:
- **512 KB** is the standard ICON.
- **1 MB** adds the CEMCORP "1 MEG ICON UPGRADE BOARD". It only works with the **Rel 2.11** ROM. With the original ROM a 1 MB workstation stops before `Login:`. The note next to the menu warns you when the loaded ROM doesn't fit.

## Switching machines on and off

- **Power** switches a machine on or off. Switch the fileserver on first and wait for `Login:`, then the workstations.
- Workstations boot from the fileserver over the network. They become nodes 16, 17, 18 and so on.
- **Check the date on the login screen.** The ICON can only store the years 1980–1995, so its clock shows today's date and time **in 1994** (1992 on 29 February). Files it writes are dated that way; see [Dates](ADMIN.md#dates).
- **Add ICON node** adds another workstation. **Remove last** removes the last one, once it is switched off.

The lights:
- **power**: the machine is on.
- **disk** and **floppy** (fileserver): the hard disk or the diskette drive is being used.
- **trouble**: blinks error codes during the self-test.

The small line under each machine's name shows its state ("off", or its node number when running).

## Keyboard and trackball

- **Click a machine** to type on it. Its frame lights up.
- **Click its screen** to capture the mouse as well. Moving the mouse moves the trackball, and the two buttons are the two ACTION keys. Press **Ctrl+Alt** to let go (Esc also works).

Keys that differ from a PC keyboard:

| ICON key | Press |
|---|---|
| ACTION (left / right) | F11 / F12, or the mouse buttons while captured |
| HELP | Insert |
| RUBOUT | Delete |
| CURSOR LOCK | Scroll Lock |
| CLR | Home |

**Type text into the selected machine** (below the machines) types pasted lines for you, one at a time, each followed by Enter. Use it for long commands. Click the machine to type into first.

## Diskettes

The fileserver has the network's only diskette drive.

| Button | What it does |
|---|---|
| **Insert…** | Put a diskette image (`.img`) in the drive |
| **New blank** | Put in an empty 640K diskette, already formatted for the ICON, ready to copy files onto |
| **Eject** | Take the diskette out |
| **Download .img** | Save the diskette to your computer, with everything written to it. After an eject, it saves the diskette you just took out |

When the fileserver has started from its hard disk, the diskette is **drive 2** (`2:` on the fileserver, `[1]2:` from a workstation).

If a diskette is in the drive when the fileserver is switched on, the fileserver tries to start from it. A system boot diskette makes it start from the diskette; a blank or data diskette is skipped and it starts from the hard disk.

## Hard disk

| Button | What it does |
|---|---|
| **New 10 MB** | An empty 10 MB drive. `configure` formats it during the install |
| **New 64 MB** | An empty 64 MB drive, prepared as the factory shipped it (two 32 MB volumes) |
| **Load .img…** | Use a hard-disk image from your computer |
| **Download .img** | Save the hard disk to your computer |

The hard disk is saved in your browser every few seconds and when the fileserver is switched off, so it survives reloading the page. Download a copy after each important step anyway; browser storage can be cleared.

A new or loaded hard disk is used from the next time the fileserver is switched on.

## Files

**Files** (below the machines) copies files between your computer and the ICON's diskette or hard disk, without using the ICON itself.

It looks and works like a file manager, in its own light window:
- The menu in the title bar picks **Diskette** or **Hard disk**, or an image you opened.
- **Open image…** works on a diskette or hard-disk image from your computer **on its own**: no machine has to run, and no ROM is needed. Everything else in the window works the same on it, and it is never read-only, because no machine is using it. **Download image** saves it with your changes; **Close** puts it away (asking first if you haven't downloaded your changes). Opened images stay in this page only.
- The **tree** on the left shows the folders. **▸** opens a folder in the tree and **▾** closes it; clicking a name opens that folder in the list.
- The **path** above the list (*hard disk / help / editor*) is clickable, and **↑ Up** goes up one folder.
- In the **list**, click to select, double-click a folder to open it, and double-click a file to download it. Icons tell folders, text files and other files apart.
- **Download** saves the selected file to your computer. **Upload…** copies files from your computer into the open folder. **New folder** makes one. **↻** reads the disk again.
- **Copy to…** copies the selected file or folder (with everything in it) to a folder on any disk in the menu: the diskette, the hard disk, an opened image, or another folder on the same disk. Permissions, owners and dates are kept exactly, so a copied program still runs. Tick **replace files that exist** to overwrite. The usual rules apply to the disk you copy to.
- Uploading over a file, or saving it in the editor, keeps its permissions and owner.
- The status bar shows the number of items and the free space, and says when the disk is read-only.
- **Edit** switches on editing. Each file then also gets **Edit** (open it in the text editor) and **Delete**. Deleting a folder deletes everything in it, after telling you how many files and folders that is. **Done editing** switches back.
- **convert text** (leave it ticked) turns the ICON's text into ordinary text on your computer and back: line endings, French accents, and code page 437 characters. Binary files are never converted.
- The diskette can be changed at any time. The hard disk can only be changed while **every machine is off**; while any is on, it is read-only.

The text editor shows the file the way the ICON does: accents as letters, and underlined text between **⟦** and **⟧** (the ICON's underline codes). **Underline ⟦…⟧** wraps the selected text. **Save** writes it back the way the file was written: its line ends, and its style of accents. Characters the ICON can't store, such as emoji, are refused with their names. If a file mixes conventions, the editor warns you that saving will change bytes you didn't edit.

The [administrator notes](ADMIN.md#moving-files-in-and-out) have the details.

## Sound and version

- **sound** (top right) turns the speaker and speech on or off.
- Next to it, **build** shows the date and code version of the emulator you are running. Mention it when you report a problem.
