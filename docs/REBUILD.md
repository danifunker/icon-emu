# Rebuilding a LEXICON network from scratch

This is the end-to-end recipe: from an empty browser to a fileserver running ICON system software 4.10.01, with Help, QDOS and MS-DOS 3.2, and student accounts. It uses the York University (YUCoM) disk set, under `UNISYS_ICON/Disk_1/`. The disk images are not distributed with this project.

Each step is marked:
- ✅ done in the emulator;
- ⚠ the expected procedure, not yet run in the emulator.

More detail on the install itself is in [INSTALL.md](INSTALL.md). Day-to-day administration (users, paths, House, shutting down) is in [ADMIN.md](ADMIN.md).

## 0. What you need

- **The boot ROM:** the ICON's two 16 KB EPROMs (5053-1 even bytes, 5053-2 odd bytes, ISS 2) or the 32 KB interleaved image. The later **Rel 2.11** pair (U33 200-504-000 LOW, U36 200-503-000 HIGH) also works: it's recognised as "Rel 2.11". Its workstations need the 4.10.05 `ws_boot` (below). Load it with **Load ROM…** at the top of the page. The page shows "stock 5053 ISS 2" when it recognises the original dump.
- **The disk images below.** All paths are relative to `Disk_1/`.

| Purpose | Folder | File |
|---|---|---|
| Boot the empty fileserver | `System 4.01.00` | `ICON_SERIES_ICON_FILESERVER_BOOT_DISKETTE_4.01.00_JAN_1990_DISK_1_OF_1.img` |
| Serve the fixed 4.10 workstation kernel | `System 4.10.01` | `ICON_SERIES_ICON_NETCARD_FILESERVER_BOOT_DISK_4.10.01_EXCEPTION_REL._SEP_17_1991_DISK_1_OF_1.img` |
| `configure` | `System 4.10.01` | `ICON_SERIES_CONFIGURATION_DISKETTE_4.10.01_EXCEPTION_REL._SEP_17_1991_DISK_1_OF_1.img` |
| System software (5 disks) | `System 4.10.01` | `ICON_SERIES_SYSTEM_SW_DISKETTE_4.10.01_EXCEPTION_REL._SEP_17_1991_DISK_1_OF_5.img` … `5_OF_5.img` |
| On-line Help | `System 4.10.01` | `ICON_SERIES_HELP_DISKETTE_4.10.01_EXCEPTION_REL.SEP_17_1991.img` |
| QDOS | `QDOS` | `UNISYS_CANADA_INC._ICON_SERIES_QDOS_4.01.02_MAY_1990.img` |
| MS-DOS 3.2 (asked for during QDOS) | `QDOS` | `ICON_SERIES_MS-DOS_BOOT_DISKETTE_LEVEL_3.2_MAY_1989_DISK_1_OF_1.img` (360K) |

Disks to avoid or not needed yet:
- **Anything with "(bad sectors)" in the name**, i.e. most of `System 4.10.01 (Copy 2)`. Use the copies in `System 4.10.01`.
- **`ISSW_4.10.01_Home_brew_fileserver_boot.img`**. Despite its name it carries the 4.01 kernel and no workstation images.
- **The 4.10.05 patch disk** (`ICON_SERIES_SYSTEM_SW_UPGRADE_DISKETTE_4.10.05_PATCH_RELEASE_NOV_1_1991.img`). Optional with the 5053 ROM, but **required for workstations with the Rel 2.11 boot ROM**. Install it with section 2b below.
- **The `Programming` folder** (C, Logo, Watcom BASIC/Pascal, GKS) and the QDOS support disks. These are packages for `install_software` that haven't been tried yet ⚠.

The default administrator login is **`root` / `superuser`**.

## 1. Choose a hard disk

Under the fileserver:
- **New 10 MB** gives a blank 306×4×17 drive with one 10 MB volume (drive 1).
- **New 64 MB** gives a factory-prepared 1024×8×17 drive: two 32 MB volumes (drives 1 and 3) plus a small third partition. Use this one for a school-sized system.

The hard disk is saved in your browser automatically. **Download .img** gives you a file to keep.

## 2. Install ICON system software 4.10.01 ✅

1. Insert the **4.01 fileserver boot diskette**. Power on the fileserver and wait for `Login:`.
   - With a 64 MB disk this takes longer. Wait until it is fully up before changing floppies, or you get "Bad load file".
2. Insert the **4.10.01 Netcard fileserver boot disk**, without rebooting. This makes the fileserver hand out the 4.10 workstation kernel, which avoids a 4.01 bug where `configure` reports "old format disk: 16". The full explanation is in INSTALL.md.
3. Power on the workstation. It boots over the network as node 16 to `Login:`. Log in as `root` / `superuser`.
4. Insert the **Configuration diskette**. On the workstation, type `configure`.
5. Click the workstation screen to capture the mouse. The left mouse button is the ACTION key. Then:
   1. **OK** to "You will be configuring the NETWORK fileserver".
   2. **OK** to "Are all previously ATTACHED … switched on?".
   3. **Perform**, leaving the name blank (node 1).
6. What happens next depends on the disk:
   - **10 MB:** **OK** the "all files will be erased" warning. The disk is formatted; press a key when the title bar says *Press any key to continue*.
   - **64 MB:** it asks for System SW disk 1, then to re-insert the Configuration diskette. Then it offers to initialise the partitions; answering "no" is fine. Click **Perform**.
7. Insert **System SW disk 1 of 5** and click **OK**. For each "Please insert Diskette N", insert the next disk and press a key.
8. "Installation … complete": press a key, then **OK**.
9. **Eject** the floppy and power-cycle the fileserver. It now boots 4.10.01 from its hard disk. Power-cycle the workstation too.
10. **Download .img** now, as a clean base install to fall back to.

## 2b. Upgrade to 4.10.05 ✅ (needed for Rel 2.11 workstations)

The patch disk holds new boot images (`ws_boot`, `hd_boot`, `ws_fsys_boot`, `ws3_boot`, the ICON2 images), `dbase_admin`, `node_info`, the boot messages and the ICON2 hard-disk driver. Rel 2.11 workstations need its `ws_boot`: the ROM writes the boot server's node number into the image at offset 48h, which breaks the 4.10.01 `ws_boot`.

It's a System Software upgrade, so it goes in with **`configure`**, not `install_software` (`install_software` says "You must use the program configure"). `configure` in turn needs the fileserver booted from a boot diskette ("FATAL ERROR! You must boot the machine from the appropriate boot diskette"). It does **not** reformat an installed disk: it says "The fileserver's first hard drive will NOT be initialized".

1. Fileserver: your installed hard disk attached, the **4.01 fileserver boot diskette** inserted, **Power**. Wait for `Login:`.
2. Insert the **4.10.01 Netcard fileserver boot disk**. Power on a workstation and log in as root.
3. Insert the **Configuration diskette**. On the workstation, type `configure`.
4. **OK** on "You will be configuring the NETWORK fileserver".
5. "Please put the first diskette of the SYSTEM SOFTWARE package…": insert the **4.10.05 patch disk**, then **OK**.
6. "Please re-insert the Configuration diskette": insert it, then **OK**.
7. Check it says "The fileserver's first hard drive will NOT be initialized", then click **Perform**.
8. It asks for the SYSTEM SOFTWARE diskette again: insert the **4.10.05 patch disk**, then **OK**. It reports "The ICON Series System SW Upgrade Release 4.10.05 is now installed on [1]2:/" (drive 2 is the hard disk here, because the fileserver booted from the floppy).
9. Press a key, then **OK** on "You have completed the configuration procedure".
10. **Eject**, then power-cycle the fileserver (it boots from its hard disk) and then the workstations. Run `verify_net`, as `configure` asks.

## 3. Add On-line Help ✅

1. The fileserver boots from its hard disk with no floppy inserted. Log in on the workstation as root.
2. Insert the **Help diskette**. Type `install_software`.
3. Click the fileserver's row (network, node 1), then **Perform**, **OK**, **OK**. Press a key when asked, then **OK** and **Quit**.

## 4. Add QDOS and MS-DOS 3.2 ✅

1. Insert the **QDOS 4.01.02 diskette**. Run `install_software` and select node 1, then **Perform**.
2. If an earlier attempt left a `/dos/MSDOS.pf`, it asks whether you modified it. Answer **n** to delete it, or **y** to keep it renamed as `MSDOS.pf.old`.
3. "Please Enter the desired hard drive number": pick **1**, or **3** on a 64 MB disk. This is where the 360K `MSDOS.pf` pseudo-diskette goes.
4. "Insert MSDOS 3.2 boot diskette into drive [1]2": insert the **MS-DOS 3.2 boot diskette** and press Enter. It copies the whole disk into `/dos/MSDOS.pf`.
5. **Expect `GETC: Read Error` at the end**, followed by an "integrity cannot be guaranteed" box. This is harmless. The install script reads its last lines from the floppy, but the floppy is now the MS-DOS disk. Click **OK**.
6. Check the result with `dump [1]3:/dos/MSDOS.pf` (or `1:` if you chose drive 1). The first bytes must be `EB 34 90 4D 53 44 4F 53 33 2E 32` ("MSDOS3.2").
   - If you see `01 00 02 00 FF FF …` instead, the page was running an emulator from before the 360K floppy fix. Hard-reload the page and run the install again.

`qdos` on a workstation starts MS-DOS ⚠. On an original ICON it is monochrome and runs only well-behaved DOS programs.

## 5. Prepare for user accounts ✅

Run this once, as root:

```
verify_net
```

The 4.10.01 install leaves the network's hardware database empty, and `new_user` rejects every cluster ID until `verify_net` has filled it in. When it asks about a workstation that is switched off, answer **n**, which keeps the node and marks it "away for service". It then checks every node up to 255 and prints `Done.`

## 6. Add students ✅

Run `new_user` for each student. See ADMIN.md for every prompt. The answers are usually:
- a login name and a password typed twice;
- **n** (not privileged);
- the class's group number;
- Enter (cluster 1);
- **n**, **n**, **n** to keep the default home directory, `.login` entry program and empty `.login` file;
- **y** to add the user.

To start a student in House, put `house` in their `.login` (see ADMIN.md) ⚠.

## 7. Save it

Power off the workstations first and the fileserver last, waiting until the disk light has been off for a few seconds. Then use **Download .img**. Keep a copy after each stage above, so a mistake costs one stage rather than the whole build.
