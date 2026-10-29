# Installing ICON system software 4.10.01 in detail

This is the detailed version of step 2 of [the rebuild guide](REBUILD.md). It covers 10 MB and 64 MB hard disks, adding software packages, and the reasons behind the unusual steps.

At the end, the fileserver starts ICON system software 4.10.01 from its own hard disk, and a workstation starts over the network from it as node 16.

## Diskettes

All from the YUCoM set:

| Step | Folder | File name starts with |
|---|---|---|
| Start the fileserver | `System 4.01.00` | `ICON_SERIES_ICON_FILESERVER_BOOT_DISKETTE_4.01.00` |
| Netcard boot disk | `System 4.10.01` | `ICON_SERIES_ICON_NETCARD_FILESERVER_BOOT_DISK_4.10.01` |
| `configure` | `System 4.10.01` | `ICON_SERIES_CONFIGURATION_DISKETTE_4.10.01` |
| System software | `System 4.10.01` | `ICON_SERIES_SYSTEM_SW_DISKETTE_4.10.01_..._DISK_1_OF_5` to `5_OF_5` |

The set has no 4.10.01 fileserver boot diskette, so the fileserver starts from the 4.01 one. Everything installed on the hard disk is 4.10.01.

## Installing onto a 10 MB disk

1. Under the fileserver, click **New 10 MB**. Insert the **4.01 fileserver boot diskette** and click **Power**. Wait for `Login:`.
2. Insert the **4.10.01 Netcard boot disk** into the fileserver. Don't restart it. (Why: see [Background](#background).)
3. Click the workstation's **Power**. It starts "SSW Release 4.10.01" over the network and shows `Login:`. Log in as `root`, password `superuser`.
4. Insert the **Configuration diskette** into the fileserver. On the workstation, type `configure`.
5. Click the workstation's screen to capture the mouse, then use the pointer and the left button (ACTION):
   1. **OK** on "You will be configuring the NETWORK fileserver".
   2. **OK** on "Are all previously ATTACHED fileservers and workstations switched on?".
   3. **Perform**. Leave the name blank.
   4. **OK** on the warning that all files will be erased.
6. "Performing the initializations" formats the hard disk. When the title bar says *Press any key to continue*, press a key.
7. When it asks for the first System Software diskette, insert **diskette 1 of 5** and click **OK**. Each time it asks for the next diskette, insert it and press a key. Each takes about a minute and a quarter.
8. "The installation of ISSSW 4.10 … is now complete": press a key. Then click **OK** on "You have completed the configuration procedure".
9. **Eject** the diskette. Switch the fileserver off and on: it starts 4.10.01 from its hard disk. Then switch the workstation off and on.
10. Click **Download .img** to keep the disk.

## Installing onto a 64 MB disk

`configure` can't prepare a blank 64 MB drive by itself: these drives came prepared from the factory. **New 64 MB** gives you such a factory-prepared drive.

The install then differs from the 10 MB one in three places:
1. After starting the fileserver from the 4.01 diskette, wait until it is **fully** up (`Login:`) before inserting the Netcard disk. It takes longer with a 64 MB disk, and changing diskettes too early gives "Bad load file".
2. After **Perform** there is no erase warning. `configure` asks for System Software diskette 1, then for the Configuration diskette again.
3. It then offers to initialise the first and second volumes. **no** is fine, because they are already empty. Click **Perform**.

The rest is the same: diskettes 1 to 5, eject, restart.

## Adding software packages

Packages other than the system software (Help, QDOS, programming languages and so on) are installed with `install_software`.

1. Start the fileserver from its hard disk, with **no diskette** in the drive.
2. Switch the workstation on and log in as `root`.
3. Insert the package's first diskette into the fileserver.
4. On the workstation, type `install_software`. With the mouse captured:
   1. Click the fileserver's line (**network, node 1**) so it is highlighted.
   2. Click **Perform**, then **OK** on "Please put the first diskette of the package in the diskette drive of node 1".
   3. **OK** on "Are you sure you want to install … on node 1?".
   4. When the title bar says *Press any key to continue*, press a key. Click **OK** on the success message, then **Quit**.
5. Eject the diskette and click **Download .img**.

## The House desktop

House is the ICON's graphical desktop of rooms, doors and notebooks. After logging in, type `house`. Press Ctrl-D in its shell window to come back. The [administrator notes](ADMIN.md#start-up-for-a-user-login) show how to start a student in House automatically.

The later **ICON_LOOK** desktop isn't in the YUCoM disk set.

## Installing without the browser

The same install can run unattended from the command line (see [Headless mode](HEADLESS.md)). Set `ICON_DISKS` to the folder holding the YUCoM disk set (the one with `System 4.01.00` in it), then:

```
export ICON_DISKS="/path/to/Disk_1"
head -c 10653696 /dev/zero > blank-10mb.img
target/release/icon-cli --preset fs \
  --floppy "$ICON_DISKS/System 4.01.00/ICON_SERIES_ICON_FILESERVER_BOOT_DISKETTE_4.01.00_JAN_1990_DISK_1_OF_1.img" \
  --hd blank-10mb.img --script scripts/install-4.10.01.txt --seconds 1561 --hd-out lexicon-4.10.01-10mb.img
```

It takes about 40 seconds. Other scripts in `scripts/`:
- `install-help.txt` adds Help to an installed disk;
- `prep-64mb.txt` prepares a blank 64 MB disk the way the factory did;
- `install-4.10.01-64mb.txt` installs onto it.

## Background

**Why the Netcard boot disk goes in before the workstation starts.** The workstation system on the 4.01 diskette has a fault. It reads the workstation's settings memory (NVRAM) without pausing the trackball, which shares the same wires. When the two collide, the workstation thinks it is a storage fileserver, and `configure` stops with "The following nodes have an old format disk: 16". The 4.10 system fixes this. With the 4.10.01 Netcard boot disk in the drive, the fileserver sends workstations the 4.10 system instead.

**How a 64 MB drive was prepared at the factory.** These commands, from the Configuration diskette, write the drive's geometry, bad-block table, boot loader and three volumes. `scripts/prep-64mb.txt` runs them, and **New 64 MB** is their result.

```
hdformat 1 c=1024 h=8 -p l=/sys/HDos_ldr
mount disk 2 /drivers/hd p=1 ; bbt 1 +u <$null ; mount disk 2 d=2 p=1
fdisk 1 +p
dinit 2 +h -p ; mount disk 3 d=2 p=2 ; dinit 3 +h -p ; mount disk 4 d=2 p=3 ; dinit 4 +h -p
```

The drive is 1,024 cylinders × 8 heads × 17 sectors (a Micropolis 1335 class drive).

**`convert_4.02`** is only for upgrading from system software 4.02. A fresh install doesn't need it.
