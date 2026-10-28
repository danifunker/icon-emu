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
