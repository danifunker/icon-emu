# Installing ICON System Software 4.10.01 onto an emulated LEXICON

Result: a 10 MB LEXICON hard disk with ISSSW 4.10.01 installed. The fileserver boots it from its own hard disk ("ICON Series SSW Release 4.10.01 – September 1991 · Cluster 1 Node 1"), and an ICON workstation network-boots 4.10 from it as node 16. A finished image is kept at `disks/lexicon-4.10.01-10mb.img` (gitignored).

## Disks used (YorkU `Disk_1` set)

| Step | Disk |
|---|---|
| Boot the fileserver | `System 4.01.00/ICON_SERIES_ICON_FILESERVER_BOOT_DISKETTE_4.01.00_JAN_1990_DISK_1_OF_1.img` |
| Serve the 4.10 workstation kernel | `System 4.10.01/ICON_SERIES_ICON_NETCARD_FILESERVER_BOOT_DISK_4.10.01_EXCEPTION_REL._SEP_17_1991_DISK_1_OF_1.img` |
| `configure` | `System 4.10.01/ICON_SERIES_CONFIGURATION_DISKETTE_4.10.01_EXCEPTION_REL._SEP_17_1991_DISK_1_OF_1.img` |
| System Software | `System 4.10.01/ICON_SERIES_SYSTEM_SW_DISKETTE_4.10.01_EXCEPTION_REL._SEP_17_1991_DISK_1_OF_5.img` … `5_OF_5` |

There is no 4.10.01 *ICON* fileserver boot diskette in the set (the "home-brew" one carries the 4.01 kernel and lacks workstation images), so the fileserver starts from the 4.01 boot diskette. Everything installed on the hard disk is 4.10.01.

## Why the Netcard boot disk is swapped in before the workstation boots

The **4.01** workstation kernel reads the NVRAM without disabling interrupts. While `configure` runs, the trackball driver polls its counter chips 40 times a second on the **same serial bus** and leaves the bus selecting a counter chip. If it lands in the middle of an NVRAM read, the read returns 0. `machine_test +f` then sees NVRAM word 0 bit 2 (workstation) clear and reports the workstation as a *storage fileserver*, and `configure` stops with "The following nodes have an old format disk: 16". The **4.10** kernel wraps the read in `pushf`/`cli`, which fixes it. The 4.10.01 Netcard boot disk carries the 4.10 `ws_boot`, and its `boot_config` maps an ICON (selector 1) to it, so swapping it into the fileserver's drive before the workstation powers on makes the fileserver's `bootsnd` serve the fixed kernel.

## Steps (browser)

1. Fileserver: **New 10 MB**, insert the **4.01 fileserver boot diskette**, **Power**. Wait for `Login:` (node 1).
2. Insert the **4.10.01 Netcard fileserver boot disk** into the fileserver (no reboot).
3. Workstation: **Power**. It network-boots "SSW Release 4.10.01" to `Login:` as node 16. Log in `root` / `superuser`.
4. Insert the **4.10.01 Configuration diskette** into the fileserver. On the workstation type `configure`.
5. Click the workstation screen to capture the mouse, then with the pointer and the left button (ACTION):
   1. **OK** on "You will be configuring the NETWORK fileserver".
   2. **OK** on "Are all previously ATTACHED fileservers and workstations switched on?".
   3. **Perform** (leave the name blank; node ID 1).
   4. **OK** on the "WARNING! All files … will be erased" dialog.
6. "Performing the initializations" formats the hard disk. When the title bar says *Press any key to continue*, press a key.
7. "Please put the first diskette of the SYSTEM SOFTWARE package…": insert **System SW 1 of 5**, click **OK**. After each "Please insert Diskette N", insert the next disk and press a key. Each disk takes about a minute and a quarter (emulated time).
8. "The installation of ISSSW 4.10 on [1]2:/ is now complete" → press a key → "You have completed the configuration procedure" → **OK**.
9. **Eject** the floppy and power-cycle the fileserver: it boots 4.10.01 from the hard disk. Then power-cycle the workstation. **Download .img** to keep the disk.

(`convert_4.02` is only for upgrades from 4.02; a fresh install does not need it.)

## Steps (headless)

```
ICON_DISKS="/Volumes/Software/ICON_ICON2/From YorkU/UNISYS_ICON/Disk_1" \
  target/release/icon-cli --preset fs \
  --floppy "$ICON_DISKS/System 4.01.00/ICON_SERIES_ICON_FILESERVER_BOOT_DISKETTE_4.01.00_JAN_1990_DISK_1_OF_1.img" \
  --hd blank-10mb.img --script scripts/install-4.10.01.txt --seconds 1561 --hd-out lexicon-4.10.01-10mb.img
```

`blank-10mb.img` is 10,653,696 zero bytes (`head -c 10653696 /dev/zero`). The script holds the exact timings and trackball movements; screenshots land next to it as `*_fs.ppm` / `*_ws.ppm`. It takes about 40 seconds of real time.

## Disk layout written by `configure`

- Physical block 1: boot loader pointer (+2 start 3, +4 count 14), new-format geometry record at +10 (4 heads, 20,808 blocks, 68 sectors/cylinder, 17 sectors/track), BBT record at +30, **PC-style partition table at +1BE**, AA55 at +1FE.
- One partition, **type 0x07** (QNX 2), start **30**, size 20,779. The start is a **1-based QNX block number**: the partition's block 1 is LBA 29.

## Adding packages afterwards (e.g. the On-line Help diskette)

Packages other than the System Software (Help, QDOS, languages …) go in with `install_software`, which reads each package's `/package` descriptor and runs its install script (`HELP_1` for Help).

1. Fileserver: load the installed hard disk, **no floppy**, **Power**. It boots 4.10.01 from the hard disk.
2. Workstation: **Power** (it network-boots 4.10 from the fileserver), log in `root` / `superuser`.
3. Insert the package's first diskette in the fileserver (e.g. `System 4.10.01/ICON_SERIES_HELP_DISKETTE_4.10.01_EXCEPTION_REL.SEP_17_1991.img`). Booted from its hard disk, the fileserver's floppy is **drive 2**, which is what `install_software` defaults to.
4. On the workstation type `install_software`. With the mouse captured:
   1. Click the fileserver's row (**network, node 1**) so it highlights.
   2. Click **Perform** → "Please put the first diskette of the package in the diskette drive of node 1" → **OK**.
   3. "Installing ICON Series On_line Help Version 4.10.00 … Are you sure you want to install HELP on node 1?" → **OK**.
   4. When the title bar says *Press any key to continue*, press a key → "HELP diskette 1 of 1 successfully installed on node 1" → **OK**, then **Quit**.
5. Eject the floppy. **Download .img** to keep the disk.

Headless: `scripts/install-help.txt` (start from the installed disk with `--hd`, no `--floppy`). Result kept as `disks/lexicon-4.10.01-10mb-help.img`: 130 pages in `/help/qnx` plus `/util/help`.

## 64 MB hard disks

A 64 MB LEXICON drive (here a Micropolis 1335 class: **1,024 cylinders × 8 heads × 17 sectors = 139,264 blocks**) was not prepared by `configure`. Its `init_hd1` only runs `hdformat`/`fdisk` on old-format disks; on a **new-format** disk it skips them and just checks/initialises the existing partitions. So a 64 MB drive needs a "factory" preparation first, done with the real tools from the Configuration diskette:

```
hdformat 1 c=1024 h=8 -p l=/sys/HDos_ldr     # geometry record, BBT (69 spares), 14-block loader at block 4
mount disk 2 /drivers/hd p=1 ; bbt 1 +u <$null ; mount disk 2 d=2 p=1
fdisk 1 +p                                     # partitions: 07 = 65,535 blocks (boot), 08 = 65,535, 09 = 8,106
dinit 2 +h -p ; mount disk 3 d=2 p=2 ; dinit 3 +h -p ; mount disk 4 d=2 p=3 ; dinit 4 +h -p
```

The partitions need empty filesystems (`dinit`): otherwise the floppy-booted fileserver mounts partition 1 as drive 2, searches it, and fails to load its start-up programs ("Bad load file").

In the browser, **New 64 MB** gives you exactly this factory-prepared disk (the 22 blocks those commands write, captured in `disks/hd64-factory.bin` and copied to `web/rom/` by `make web`). Headless: `scripts/prep-64mb.txt` makes it from a blank 71,303,168-byte image.

Installing then differs from the 10 MB flow:
1. Same start: 4.01 boot diskette, wait until the fileserver is **fully** up (it takes longer with a hard disk mounted; swapping floppies earlier gives "Bad load file"), then the Netcard 4.10.01 disk, then power on the workstation, log in, Configuration diskette, `configure`, OK, OK, **Perform**.
2. There is no "initialize" warning: `configure` asks for System SW disk 1 → insert, OK → "Please re-insert the Configuration diskette" → insert, OK.
3. The main screen now offers *initialize the first / second hard drive partition? yes / no*. "no" is fine (they already hold empty filesystems). **Perform**.
4. System SW disks 1–5 as before → complete → OK → eject → power-cycle the fileserver. It boots 4.10.01 from the 64 MB disk; the ROM's 4-head assumption doesn't matter because the loader sits in cylinder 0.

Headless: `scripts/install-4.10.01-64mb.txt` with the prepared disk as `--hd`. Result kept as `disks/lexicon-4.10.01-64mb.img`.

The ICON's `fdisk` writes a PC-style table with QNX 2 partition types **07, 08, 09** (one per partition), and the start fields are **1-based** block numbers.

## The graphical environment (House)

The doors-and-notebooks environment is **House** (`/util/house`, which runs `/sys/ui`; the book's `ui`). After logging in, type `house`; CTRL-d from its QNX shell returns. A user can be given House as their login program through the password file's entry-command field (`new_user`). The separate **ICON_LOOK** GUI package (a `/icon_look` directory that `sys.init` and `env_mgr` look for) is not in the YorkU disk set.
