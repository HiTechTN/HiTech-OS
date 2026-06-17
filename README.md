# ⚡ Vibe-OS

[![Rust](https://img.shields.io/badge/rust-nightly_1.98.0-orange?logo=rust)](https://www.rust-lang.org)
[![Arch](https://img.shields.io/badge/arch-x86__64-blue?logo=amd)](https://en.wikipedia.org/wiki/X86-64)
[![Boot](https://img.shields.io/badge/boot-bootloader-green?logo=linux)](https://github.com/rust-osdev/bootloader)
[![License](https://img.shields.io/badge/license-MIT-brightgreen)](#license)

> **Un noyau bare-metal x86-64 performant, écrit en Rust.**  
> Démarre sur tout matériel PC compatible — avec réseau, multitâche préemptif, ext2, TCP, et un shell interactif.

---

## 📸 Aperçu

```
╔══════════════════════════════════════════════════╗
║   ⚡ Vibe-OS v0.8.0                             ║
║   ────────────────────────────────              ║
║   [OK] Heap memoire                             ║
║   [OK] IDT | Timer | PIC | PCIe                 ║
║   [OK] Scheduler multitâche (100Hz)             ║
║   [OK] RTL8139 | IP: 10.0.2.15                  ║
║   [OK] ext2 monté depuis ramdisk                ║
║   ────────────────────────────────              ║
║   > ping 10.0.2.2                               ║
║   Réponse de 10.0.2.2: seq=1                   ║
║   > dhclient                                    ║
║   DHCP: IP 10.0.2.15 reçue                      ║
║   > ls                                          ║
║     drwxr-xr-x  bin  etc  home  usr             ║
╚══════════════════════════════════════════════════╝
```

---

## 🚀 Quick Start

### Prérequis
- **Rust nightly 1.98.0** — installé via `rustup`
- **QEMU** — pour l'émulation (`qemu-system-x86_64`)
- **bootimage** — pour créer l'image bootable

```bash
# Installer la toolchain
rustup toolchain install nightly-2025-03-01
rustup component add rust-src --toolchain nightly-2025-03-01-x86_64-unknown-linux-gnu
cargo install bootimage

# Compiler et lancer
cargo bootimage
qemu-system-x86_64 -drive format=raw,file=target/x86_64-vibe_os/debug/bootimage-vibe-os.bin -serial stdio -m 256M
```

> **Alternative** : `cargo run` si QEMU est dans `$PATH`.

---

## 🏗️ Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    ⚡ Vibe-OS Kernel                     │
├──────────────┬──────────────┬───────────────────────────┤
│   Cœur       │  Drivers     │   Systèmes de fichiers    │
│──────────────┼──────────────┼───────────────────────────┤
│ • Allocateur │ • RTL8139   │ • ext2 (ramdisk)          │
│ • IDT/PIC    │ • AHCI/SATA │ • tmpfs                   │
│ • Timer PIT  │ • USB/XHCI  │ • devpts                  │
│ • Scheduler  │ • AC97      │ • fat32 / ntfs / iso9660  │
│ • Syscalls   │ • E1000     │ • procfs / sysfs          │
│ • Signaux    │ • VGA/grph  │                           │
│ • Pipes      │ • Mouse     │                           │
└──────────────┴──────────────┴───────────────────────────┘
         │                    │
         ▼                    ▼
┌─────────────────────────────────────────────────────────┐
│                  Pile Réseau                            │
├──────────────┬──────────────┬───────────────────────────┤
│ • ARP        │ • IPv4       │ • ICMP (ping)             │
│ • UDP        │ • TCP (états)│ • DHCP client             │
└──────────────┴──────────────┴───────────────────────────┘
```

### Flux de démarrage

```
BIOS → Bootloader → Vibe-OS entry
                        │
                        ▼
              ┌──────────────────┐
              │  allocateur tas  │ ← heap + buddy
              │  IDT + PIC       │ ← interruptions
              │  timer (100Hz)   │ ← scheduler
              │  PCIe scan       │ ← détection matériel
              │  drivers (SATA,  │
              │    RTL8139, USB, │
              │    AC97, etc.)   │
              │  ext2 monté      │ ← ramdisk 4MB
              │  réseau initialisé│
              └──────────────────┘
                        │
                        ▼
              ┌──────────────────┐
              │  Shell interactif│
              │ ＞ prête pour    │
              │   commandes      │
              └──────────────────┘
```

---

## 🧠 Fonctionnalités

<details>
<summary><b>🧰 Noyau & Matériel</b> — Cliquez pour déplier</summary>

| Domaine | Statut | Détails |
|---------|--------|---------|
| Allocateur tas | ✅ | Heap free-list + buddy |
| IDT (vecteurs d'interruption) | ✅ | Breakpoint, Timer, Keyboard, NIC |
| PIC 8259 | ✅ | Remappé offsets 32/40 |
| Timer PIT (100Hz) | ✅ | Base du scheduler |
| PCIe bus scan | ✅ | Bus 0..255, device 0..31, fonction 0..7 |
| BAR I/O/MMIO | ✅ | Lecture et décodage 32/64-bit |
| AHCI SATA | ✅ | HBA détection, READ/WRITE DMA EXT |
| USB (UHCI/EHCI/XHCI) | ✅ | Détection contrôleur |
| AC97 Audio | ✅ | Son jouable via bar |
| RTL8139 Ethernet | ✅ | IRQ + polling |
| E1000 Ethernet | ✅ | Init + détection |
| VGA texte | ✅ | Écran couleur 80×25 |
| Souris PS/2 | ✅ | Événements |
</details>

<details>
<summary><b>📁 Systèmes de fichiers</b></summary>

| FS | Statut |
|----|--------|
| **ext2** | ✅ Monté depuis ramdisk 4MB, `ls`/`cat`/`stat` |
| **tmpfs** | ✅ Volatile in-memory |
| **devpts** | ✅ Pseudo-terminal |
| **procfs** | ✅ Process info |
| **sysfs** | ✅ Périphériques |
| **fat32 / ntfs** | ✅ Init (structurelles) |
| **iso9660** | ✅ CD-ROM |
</details>

<details>
<summary><b>🌐 Réseau</b></summary>

| Protocole | Statut |
|-----------|--------|
| **ARP** | ✅ Cache + résolution + requêtes |
| **IPv4** | ✅ En-têtes, checksum, envoi/réception |
| **ICMP** | ✅ Echo request/reply (ping) |
| **UDP** | ✅ Envoi paquets, checksum |
| **TCP** | ✅ Machine d'états complète (LISTEN → CLOSED) |
| **DHCP** | ✅ Client DISCOVER/OFFER/REQUEST/ACK |
| **Ping** | ✅ `ping <ip>` avec timeout |
</details>

<details>
<summary><b>⚙️ Shell & Commandes</b></summary>

```
Système:   help  clear  info  mem  date  time  uptime
           reboot  shutdown  exit
Fichiers:  ls  cat  mkdir  touch  rm  df  mount
Réseau:    ifconfig  ping  netstat  dhclient
Matériel:  lspci  lsusb
Processus: ps  kill
Audio:     beep  test
Graphique: clearcolor  draw
```
</details>

---

## 💻 Commandes réseau

```bash
> dhclient                    # Obtient une IP via DHCP
  DHCP: envoi DISCOVER...
  DHCP: OFFER recu
  DHCP: ACK recu
  IP: 10.0.2.15

> ifconfig                    # Affiche les interfaces
  eth0: 52:54:00:12:34:56 - 10.0.2.15

> ping 10.0.2.2               # Ping une machine
  Pinging 10.0.2.2...
  Réponse de 10.0.2.2: seq=1

> netstat                     # Connexions TCP
  Connexions TCP actives:
  Proto  Local          Remote         State
  TCP    0.0.0.0:80     0.0.0.0:0      LISTEN
```

---

## 🛠️ Build & Déploiement

### Compilation

```bash
# Debug
cargo bootimage

# Release (optimisé)
cargo bootimage --release
```

### Lancement QEMU

```bash
qemu-system-x86_64 \
  -drive format=raw,file=target/x86_64-vibe_os/debug/bootimage-vibe-os.bin \
  -serial stdio \
  -m 256M \
  -netdev user,id=net0,hostfwd=tcp::8080-:80 \
  -device rtl8139,netdev=net0
```

### Boot réel

1. `cargo bootimage --release`
2. Copier `target/x86_64-vibe_os/release/bootimage-vibe-os.bin` sur une clé USB
3. `dd if=bootimage-vibe-os.bin of=/dev/sdX bs=1M`
4. Démarrer depuis la clé USB

---

## 📂 Structure du code

```
src/
├── lib.rs          # Point d'entrée, init séquentiel
├── main.rs         # Rust entry _start
├── shell.rs        # Shell interactif
├── network.rs      # RTL8139, ARP, IPv4, ICMP, UDP, TCP, DHCP
├── vfs.rs          # Table de montage VFS
├── ext2.rs         # Système de fichiers ext2
├── ramdisk.rs      # Ramdisk 4MB statique
├── ahci.rs         # Contrôleur AHCI SATA
├── acpi.rs         # ACPI, PCI config, PM
├── pcie.rs         # Scan bus PCIe
├── interrupts.rs   # IDT, PIC, handlers
├── scheduler.rs    # Multitâche préemptif
├── keyboard.rs     # Clavier PS/2
├── timer.rs        # PIT 100Hz
├── allocator.rs    # Heap + free-list
├── usb.rs          # USB UHCI/EHCI/XHCI
├── dhcp.rs         # Client DHCP
├── graphics.rs     # VBE framebuffer
├── audio.rs        # AC97 son
├── ac97.rs         # Driver AC97
├── e1000.rs        # Intel E1000 NIC
├── fat32.rs        # FAT32 lecture
├── ntfs.rs         # NTFS structures
├── iso9660.rs      # ISO 9660 CD-ROM
├── tmpfs.rs        # Tmpfs mémoire
├── devpts.rs       # Pseudo-terminal
├── proc.rs         # Procfs
├── sysfs.rs        # Sysfs
├── syscall.rs      # Appels système
├── elf.rs          # ELF loader
├── block.rs        # Block device dispatch
├── pagecache.rs    # Cache de pages
├── pipe.rs         # Tubes (pipes)
├── signal.rs       # Signaux
├── console.rs      # Console
├── rng.rs          # Générateur aléatoire
├── resource.rs     # Gestionnaire de ressources
├── ...
```

---

## 📜 License

MIT © 2026 Vibe-OS Contributors

---

<p align="center">
  <i>Construit avec ❤️ en Rust pur, sur métal nu.</i>
  <br>
  <a href="https://github.com/rust-osdev/bootloader">bootloader</a> ·
  <a href="https://docs.rs/x86_64">x86_64</a> ·
  <a href="https://wiki.osdev.org">OSDev Wiki</a>
</p>
