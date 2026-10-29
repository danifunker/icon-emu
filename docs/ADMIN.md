# Running an ICON network: administrator notes

These notes cover running ICON system software 4.10 once it is installed (see [the rebuild guide](REBUILD.md)). The administrator's login is **`root`**, password **`superuser`**.

Items marked ⚠ come from the manuals and haven't been tried in the emulator yet. Everything else has.

## Paths and drives

QNX 2 has no `/home`. Paths name a node and a drive:

```
[1]1:/user/alice/.login
 │ │ └ path on that drive
 │ └ drive 1
 └ node 1 (the fileserver)
```

- `[1]` can be left out on a one-fileserver network: `1:/user/alice`.
- `cd 1:/user`, `pwd`, `files` (with block counts and dates), `ls`, `cat FILE`, `edit FILE`.

| Drive | What it is |
|---|---|
| 1 | Hard disk, first volume: the system, `/user` home directories |
| 2 | The fileserver's floppy, when it has booted from its hard disk |
| 3 | Second 32 MB volume of a 64 MB disk |

| Directory | Contents |
|---|---|
| `/user/<login>` | Home directories, each with a `.login` |
| `/cmds`, `/util` | Commands: QNX, and Unisys/CEMCORP tools (`new_user`, `house`, `edit`, …) |
| `/config` | `net.config`, the user and hardware databases, `sys.init`, printer setup |
| `/dos` | QDOS pseudo-disks: `MSDOS.pf` (MS-DOS 3.2 boot), `work.ph` (student work disk) |
| `/sys`, `/boot` | System tasks and network boot images |

## Moving files in and out

### With the Files panel (easiest)

Open **Files** below the machines on the web page. It shows the files on the diskette or the hard disk, and copies them to and from your computer directly.

1. Pick **Diskette** or **Hard disk** in the menu. Click a folder to open it, **Up** to go back.
2. **Download** copies a file to your computer. **Upload…** copies files from your computer into the folder you're in. **New folder** makes a folder. **Copy to…** copies a file or folder to another disk or folder, keeping its permissions, owner and dates, so programs stay runnable (for example Logo from the Logo diskette image to the hard disk).
3. Click **Edit** to edit or delete. Each file gets **Edit**, which opens it in a text editor on the page, and **Delete**. Deleting a folder deletes everything in it; the page first says how many files and folders that is. Click **Done editing** when you're finished.
4. Keep **convert text** ticked. ICON text differs from ordinary text in three ways, and the panel converts all of them, both ways:
   - lines end with an RS character (0x1E) instead of a newline;
   - French accents are stored as Telidon sequences (é is `0x19 0x42 e`), as the ICON's editor and keyboard write them;
   - bytes 128 to 255 are the IBM PC character set (code page 437), except `0x99` and `0x9A`, which switch underline on and off.

   On your computer you get ordinary UTF-8 text. Binary files are left alone.

The text editor shows accents as letters, and underlined text between **⟦** and **⟧**. Saving keeps the file's own line ends and accent style; you can change them with the two menus. Characters the ICON can't store (emoji, most non-Latin scripts) are refused, and the editor names them.

Rules:
- **The diskette** can be changed at any time. The page takes it out, makes the change and puts it back, like swapping diskettes by hand. Don't do it while the ICON is writing to the diskette.
- **The hard disk** can only be changed while **every machine is off**. The fileserver keeps parts of the disk in memory and would write over the change, and a running workstation may be using it. While anything is on, the panel and the editor are read-only, showing the disk as last saved.
- **Names** must fit QNX: up to 16 letters, digits, `_`, `.` or `$`. Other characters become `_`, and long names are shortened. The panel says when it renames something.
- Files you upload or save are dated with the emulated ICON clock: today's date in 1994, as files the ICON writes itself (see [Dates](#dates)). Files you **copy** between disks keep their original dates.
- A new file uploaded from your computer gets QNX's default permissions, which don't include "execute". If it is a program, mark it runnable on the ICON with `chattr 1:/path/name a=+e pg=+e po=+e`, or copy it from an ICON disk with **Copy to…** instead. Replacing an existing file keeps that file's permissions.

To give a student a file, upload it straight into their home folder on the hard disk (`/user/<login>`) with every machine off. Or put it on a diskette and have them `copy [1]2:/name name`.

### With diskette images

Diskette images also work with other tools, such as [RustyBackup](https://github.com/danifunker/rusty-backup)'s `rb-cli`.

1. Under the fileserver, click **New blank**. This inserts an empty 640K diskette that is already formatted.
2. Copy files onto it. From a workstation the diskette is `[1]2:`. Give the copy a file name, not just the drive:
   ```
   copy 1:/user/alice/story [1]2:/story
   files [1]2:/
   ```
3. Click the diskette's **Download .img** to save it to your computer.
4. Read it, or add files to it, with `rb-cli`:
   ```
   rb-cli ls blank-640k.img /
   rb-cli get --rs-to-lf blank-640k.img /story story.txt
   ```
5. To bring files in, **Insert…** the diskette image, then `copy [1]2:/name 1:/user/alice/name`.

To prepare a blank diskette yourself instead of using **New blank**, insert any empty 640K image and run `dinit 2` on the fileserver. Press Enter when it asks you to place the disk in the drive.

## User accounts

**Once after install,** run `verify_net`. Without it, `new_user` rejects every cluster ID with "Invalid cluster id". If it asks about switched-off workstations, answer **n**.

**Add a user** with `new_user`. The prompts:

| Prompt | Usual answer |
|---|---|
| Enter new login name | up to 16 characters, e.g. `alice` (Ctrl-C quits) |
| Type new password / Retype password | up to 20 characters, or Enter for none |
| Should the user be a Privileged User? | `n` for students |
| Please enter new group number | the class's group, e.g. `1`. Students in one group share a group number; the member number is assigned for you |
| Type user's cluster id | Enter. This is the fileserver the user's workstation boots from: node 1 |
| Should the user's home directory be different? | `n` (`[1]1:/user/<login>`) |
| Should the entry program be different? | `n` (`.login`); or `y` and `/util/house` to make House the login program ⚠ |
| Should the user's `.login` file be different? | `n` (an empty `.login` is created) |
| Should this user be added? | `y` |

- **Remove a user:** `delete_user` ⚠.
- **Change a password:** `new_password` ⚠.
- **User IDs** are group.member. Students use groups 1–254. A classroom administrator is member 255 of a group. The site administrator is 255.255.

## Start-up for a user: `.login`

`.login` is a shell script that runs each time the user logs in. To start them in House and log them off when they leave it:

```
house
logoff
```

Write it from root, then check it:

```
echo house >1:/user/alice/.login
echo logoff >>1:/user/alice/.login     (append; or use: edit 1:/user/alice/.login)
cat 1:/user/alice/.login
```

`echo … >` and `>>` work. Logging off when House exits hasn't been tried yet ⚠.

House has its own QNX shell window, so this keeps students out of the prompt *after* House but not inside it. Setting `/util/house` as the entry program in `new_user` avoids the shell altogether ⚠.

## User interfaces

- **The shell:** the `$` prompt. This is the default.
- **House** (`house`, which is `/util/house` running `/sys/ui`): the doors-and-notebooks desktop from system software 3.00 onwards. Rooms and doors are directories, notebooks are files. It's built on the Cemcorp Window Manager.
- **ICON_LOOK** is the later graphical login and desktop (about 1991). 4.10.01's `sys.init` supports it, but the package is **not** in the YorkU set.
- **Full-screen programs** you can launch from either: `edit`, `logo` (and the French version), the languages, and `qdos` (MS-DOS).

## Network

- The fileserver is always **node 1**. Workstations boot from it over ICONET as nodes 16, 17, …
- `/config/net.config` lists the nodes. `verify_net` checks it against the network and fills in the hardware database.
- If the network hangs, the fileserver may have run out of virtual circuits. `kill_vcs`, or reboot the fileserver and then every workstation ⚠.

## Dates

The ICON stores dates with a 4-bit year, so it can only hold **1980 to 1995**. The emulator therefore runs the ICON's clock on **today's month, day and time, in 1994**. On 29 February, which 1994 doesn't have, it uses **1992**. 1994 sorts after all the ICON system software (the last release is from 1991), and a machine left running past New Year's Eve moves into 1995, which the ICON can still store.

The ICON's clock has no time zone; the emulator gives it **your local time**, both on the web page and in headless mode. Dates copied out to your computer (`icon-cli disk get`) are read the same way, so a file the ICON dated 5:04 pm shows 5:04 pm on your computer.

**Check the date when a machine starts.** The login screen shows the date the ICON is using, for example `October 29 1994  4:35 pm`. Everything QNX writes, and every file you upload or save in the Files panel, gets that date. Files copied between ICON disks keep their original dates. The same rule is used by RustyBackup and the `qnx2fs` crate when they write modern files to ICON disks.

## Sending a message to everyone

`apb` ("all-points bulletin") sends a one-line message, up to 63 characters, to every screen on the network:

```
apb Lunch is ready - please log off
```

Each user sees it the next time they press Enter at the shell prompt, as `** APB (16:36): Lunch is ready - please log off **`. A second `apb` before then replaces the first. Users in a full-screen program such as House or the editor see it when they come back to the shell.

## Turning it off

There is no shutdown command. The manual's order:

1. Workstations first: `logoff` (or Ctrl-D), then power off.
2. Cluster or storage fileservers, if any.
3. The network fileserver (node 1) last, once nothing is running and the **disk light has been off for a few seconds**. It keeps a RAM disk cache.

Power on in reverse order: the fileserver first, then the workstations once it shows `Login:`.

After an unclean stop, run `chkfsys 1` (and `chkfsys 3` on a 64 MB disk). It should end with "Rebuilt bitmap agrees with existing bitmap".

In the browser, the hard disk is saved to browser storage every 5 seconds and again when the fileserver is switched off. **Download .img** makes a copy you can keep.

## Speech

ICON workstations have a TI TMS5220C speech chip; the emulator plays it through the page's sound. Fileservers have none. The Speech Administrator (`/sys/speech`) starts automatically on a workstation.

- `say FILE …` speaks phrase files.
- **The phrase file format** is a series of 12-byte frames: energy, pitch, k1–k10, each a parameter index (energy 0–15, where 15 is a stop frame; pitch 0–63, where 0 is unvoiced). `decode_phrase <FILE` lists a file as one frame per line, and `encode_phrase <TEXT >FILE` turns such a listing back into a file. `convert_phrase` converts old TMS5220 "codepack" bitstreams.
- **No phrase files ship with 4.10.01** or anywhere in the YorkU disk set. To try speech, make one:

```
echo 10 49 20 13 3 6 11 11 12 5 3 3 >1:/tmp/a.txt
echo 14 44 24 19 8 2 5 8 9 6 3 4 >>1:/tmp/a.txt
(repeat those two lines a few times for a longer sound)
encode_phrase <1:/tmp/a.txt >1:/tmp/a.sp
say 1:/tmp/a.sp
```

Those two frames are the voiced sound the boot ROM's speech self-test plays. Each frame lasts 25 ms.

**Making real words** from a recording. This runs on your own computer, in a copy of the emulator's source code, and needs Python with numpy, and sox:

```
say -v Daniel -o hello.aiff hello            # any recording works; macOS's say is handy
sox hello.aiff -r 8000 -c 1 -b 16 hello.wav  # 8 kHz mono 16-bit
python3 scripts/lpc_encode.py hello.wav > hello.txt                  # the frame listing
target/release/icon-cli lpc-render hello.txt hello-tms.wav           # preview through the emulated chip
python3 scripts/lpc_encode.py hello.wav --paste hello > hello.paste.txt
```

Open **Type text into the selected machine** on the web page, paste `hello.paste.txt`, click a logged-in workstation, then **Type it**. It writes `1:/tmp/hello.txt` line by line with `echo`, runs `encode_phrase` and `say`.

`lpc_encode.py` is a plain LPC-10 analysis (pre-emphasis, autocorrelation, Levinson-Durbin, autocorrelation pitch), with each parameter rounded to the nearest entry in the chip's tables. `--gain` sets the loudness; the default 0.5 avoids clipping the chip's 8-bit output. `--pitch-shift` raises or lowers the voice. The ICON's own `say` produces the same audio as `lpc-render`.

## QDOS (MS-DOS)

- Pseudo-disks are ordinary QNX files: `.pf` files are 160–360K floppy images, `.ph` files are hard-disk images up to 32 MB.
- `install_dos_pgm` installs DOS applications onto pseudo-disks ⚠.
- On an original ICON, QDOS is **monochrome** and runs only programs that go through the BIOS. CGA graphics and the trackball as a mouse need the ICON2.
