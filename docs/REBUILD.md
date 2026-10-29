# Rebuilding an ICON network from scratch

This guide takes you from an empty browser to a working school network: a fileserver running ICON system software 4.10, with on-line Help, MS-DOS, and student accounts. It takes about an hour, most of it waiting for diskettes to copy.

It uses the York University Computer Museum (YUCoM) disk set. The disk images are not included with the emulator.

If you haven't used the page before, read [Using the web page](UI.md) first. For more background on the install, see [Installing in detail](INSTALL.md). Day-to-day running (users, paths, shutting down) is in the [administrator notes](ADMIN.md).

Steps marked ⚠ are taken from the manuals and haven't been tried in the emulator yet. Everything else has.

## What you need

**The boot ROM.** Either ROM works:
- the original **5053 ISS 2** (two 16 KB EPROMs, or one 32 KB image);
- the later **Rel 2.11**. Its workstations need the 4.10.05 upgrade (step 3).

**These diskette images.** The folder names are those of the YUCoM set.

| Use | Folder | File name starts with |
|---|---|---|
| Starting the empty fileserver | `System 4.01.00` | `ICON_SERIES_ICON_FILESERVER_BOOT_DISKETTE_4.01.00` |
| Netcard boot disk (see step 2) | `System 4.10.01` | `ICON_SERIES_ICON_NETCARD_FILESERVER_BOOT_DISK_4.10.01` |
| Configuration | `System 4.10.01` | `ICON_SERIES_CONFIGURATION_DISKETTE_4.10.01` |
| System software, 5 diskettes | `System 4.10.01` | `ICON_SERIES_SYSTEM_SW_DISKETTE_4.10.01_..._DISK_1_OF_5` to `5_OF_5` |
| 4.10.05 upgrade (Rel 2.11 ROM only) | `System 4.10.01` | `ICON_SERIES_SYSTEM_SW_UPGRADE_DISKETTE_4.10.05` |
| On-line Help | `System 4.10.01` | `ICON_SERIES_HELP_DISKETTE_4.10.01` |
| QDOS | `QDOS` | `UNISYS_CANADA_INC._ICON_SERIES_QDOS_4.01.02` |
| MS-DOS 3.2 | `QDOS` | `ICON_SERIES_MS-DOS_BOOT_DISKETTE_LEVEL_3.2` |

Avoid:
- any image with **"(bad sectors)"** in its name. Most of the `System 4.10.01 (Copy 2)` folder is like that; use the copies in `System 4.10.01` instead.
- **`ISSW_4.10.01_Home_brew_fileserver_boot.img`**. Despite its name, it holds the older 4.01 system and can't start workstations.

The administrator's login is **`root`**, password **`superuser`**.

## 1. Choose a hard disk

Under the fileserver, click:
- **New 10 MB** for a small system, or
- **New 64 MB** for a school-sized one. It has two 32 MB volumes (drives 1 and 3).

## 2. Install the system software

1. Insert the **4.01 fileserver boot diskette** and click the fileserver's **Power**. Wait for `Login:` on its screen.
   With a 64 MB disk this takes longer. Wait until `Login:` appears before changing diskettes, or you get "Bad load file".
2. Insert the **4.10.01 Netcard boot disk**. Don't restart anything.
   This makes the fileserver send the newer 4.10 system to the workstation. The 4.01 system has a fault that makes `configure` fail with "old format disk: 16".
3. Click the workstation's **Power**. It starts over the network and shows `Login:`. Log in as `root` / `superuser`.
4. Insert the **Configuration diskette**. On the workstation, type `configure` and press Enter.
5. Click the workstation's screen to capture the mouse. The left mouse button is ACTION. Then:
   1. **OK** to "You will be configuring the NETWORK fileserver".
   2. **OK** to "Are all previously ATTACHED … switched on?".
   3. **Perform**, leaving the name blank.
6. What happens next depends on the disk:
   - **10 MB:** click **OK** on the warning that all files will be erased. The disk is formatted. When the title bar says *Press any key to continue*, press a key.
   - **64 MB:** it asks for System Software diskette 1, then for the Configuration diskette again. It then offers to initialise the volumes; answering **no** is fine. Click **Perform**.
7. Insert **System Software diskette 1 of 5** and click **OK**. Each time it says "Please insert Diskette N", insert the next one and press a key. Each takes a minute or so.
8. At "Installation … complete", press a key, then click **OK**.
9. Click **Eject**. Switch the fileserver off and on again: it now starts from its hard disk. Then switch the workstation off and on.
10. Click the hard disk's **Download .img** to keep a clean copy.

## 3. Upgrade to 4.10.05 (needed with the Rel 2.11 ROM)

Skip this with the original 5053 ROM; it's optional there. Workstations with the Rel 2.11 ROM crash after loading the 4.10.01 system, and the 4.10.05 upgrade fixes that.

The upgrade is installed with `configure`, which only runs when the fileserver has started from a boot diskette. It does **not** erase the hard disk.

1. Insert the **4.01 fileserver boot diskette** and switch the fileserver on, with your installed hard disk attached. Wait for `Login:`.
2. Insert the **4.10.01 Netcard boot disk**. Switch a workstation on and log in as `root`.
3. Insert the **Configuration diskette**. On the workstation, type `configure`.
4. **OK** on "You will be configuring the NETWORK fileserver".
5. When it asks for the first System Software diskette, insert the **4.10.05 upgrade diskette** and click **OK**.
6. When it asks for the Configuration diskette again, insert it and click **OK**.
7. Check that it says "The fileserver's first hard drive will NOT be initialized", then click **Perform**.
8. When it asks for the System Software diskette again, insert the **4.10.05 upgrade diskette** and click **OK**. It reports that Release 4.10.05 is installed.
9. Press a key, then click **OK** on "You have completed the configuration procedure".
10. **Eject**. Switch the fileserver off and on, then the workstations. Run `verify_net` (step 6) as `configure` asks.

## 4. Add on-line Help

1. Start the fileserver from its hard disk, with no diskette in the drive. Log in on the workstation as `root`.
2. Insert the **Help diskette** and type `install_software`.
3. Click the fileserver's line (network, node 1), then **Perform**, **OK**, **OK**. Press a key when asked, then click **OK** and **Quit**.

## 5. Add QDOS and MS-DOS 3.2

QDOS runs MS-DOS on the ICON.

1. Insert the **QDOS diskette**. Run `install_software`, click node 1, then **Perform**.
2. If you installed QDOS before, it asks whether you changed `MSDOS.pf`. Answer **n** to replace it.
3. When it asks for the hard drive number, type **1** (or **3** on a 64 MB disk).
4. When it asks for the MS-DOS 3.2 boot diskette, insert the **MS-DOS 3.2 diskette** and press Enter. It copies the whole diskette.
5. At the end you'll see **`GETC: Read Error`** and a box saying integrity can't be guaranteed. This is harmless: the install script tries to read its last lines from the QDOS diskette, which is no longer in the drive. Click **OK**.

Type `qdos` on a workstation to start MS-DOS ⚠. On an original ICON it is black and white and runs only simple DOS programs.

## 6. Prepare for user accounts

Run this once, as `root`:

```
verify_net
```

Until you do, `new_user` refuses every cluster number. If `verify_net` asks about a workstation that is switched off, answer **n**. It finishes with `Done.`

## 7. Add students

Run `new_user` once for each student. The usual answers:
- a login name, then a password twice;
- **n** (not privileged);
- the class's group number;
- Enter (cluster 1);
- **n**, **n**, **n** to keep the usual home directory and start-up files;
- **y** to add the user.

The [administrator notes](ADMIN.md#user-accounts) explain each question, and how to start students in the House desktop ⚠.

## 8. Save it

Switch the workstations off first and the fileserver last, once its disk light has been off for a few seconds. Then click **Download .img**.

Keep a copy after each step above. Then a mistake costs you one step rather than the whole build.
