# Running an ICON network: administrator notes

These notes cover running ICON system software 4.10.01 once it is installed (see [REBUILD.md](REBUILD.md)). The default administrator is **`root` / `superuser`**.

Items marked ✅ were confirmed in the emulator. Items marked ⚠ come from the manuals and haven't been tried yet.

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

## User accounts

**Once after install,** run `verify_net` ✅. Without it, `new_user` rejects every cluster ID with "Invalid cluster id". If it asks about switched-off workstations, answer **n**.

**Add a user** with `new_user` ✅. The prompts:

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

`echo … >` is confirmed ✅. `>>` and the House-then-logoff behaviour are not yet ⚠.

House has its own QNX shell window, so this keeps students out of the prompt *after* House but not inside it. Setting `/util/house` as the entry program in `new_user` avoids the shell altogether ⚠.

## User interfaces

- **The shell:** the `$` prompt. This is the default.
- **House** (`house`, which is `/util/house` running `/sys/ui`) ✅: the doors-and-notebooks desktop from system software 3.00 onwards. Rooms and doors are directories, notebooks are files. It's built on the Cemcorp Window Manager.
- **ICON_LOOK** is the later graphical login and desktop (about 1991). 4.10.01's `sys.init` supports it, but the package is **not** in the YorkU set.
- **Full-screen programs** you can launch from either: `edit`, `logo` (and the French version), the languages, and `qdos` (MS-DOS).

## Network

- The fileserver is always **node 1**. Workstations boot from it over ICONET as nodes 16, 17, …
- `/config/net.config` lists the nodes. `verify_net` checks it against the network and fills in the hardware database ✅.
- If the network hangs, the fileserver may have run out of virtual circuits. `kill_vcs`, or reboot the fileserver and then every workstation ⚠.

## Turning it off

There is no shutdown command. The manual's order:

1. Workstations first: `logoff` (or Ctrl-D), then power off.
2. Cluster or storage fileservers, if any.
3. The network fileserver (node 1) last, once nothing is running and the **disk light has been off for a few seconds**. It keeps a RAM disk cache.

Power on in reverse order: the fileserver first, then the workstations once it shows `Login:`.

After an unclean stop, run `chkfsys 1` (and `chkfsys 3` on a 64 MB disk). It should end with "Rebuilt bitmap agrees with existing bitmap".

In the browser, the hard disk is saved to browser storage every 5 seconds and again when the fileserver powers off. **Download .img** makes a copy you can keep.

## QDOS (MS-DOS)

- Pseudo-disks are ordinary QNX files: `.pf` files are 160–360K floppy images, `.ph` files are hard-disk images up to 32 MB.
- `install_dos_pgm` installs DOS applications onto pseudo-disks ⚠.
- On an original ICON, QDOS is **monochrome** and runs only programs that go through the BIOS. CGA graphics and the trackball as a mouse need the ICON2.
