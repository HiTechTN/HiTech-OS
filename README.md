# 🚀 HiTech-OS — The Sovereign AI-First OS

Système d'exploitation souverain, local-first, conçu pour l'IoT et les
infrastructures intelligentes. L'IA tourne au cœur du système (AI-First),
sans dépendance cloud, pour la confidentialité, la latence minimale et
une sécurité de niveau industriel.

> Ce dépôt est la refonte complète du projet Vibe-OS (kernel bare-metal
> x86_64). L'architecture actuelle repose sur un NixOS minimal + un
> démon IA en Rust, voir `docs/ROADMAP.md`.

## Structure du dépôt

| Dossier | Rôle |
|---|---|
| `os/` | Configuration NixOS : `common.nix` (socle partagé), `configuration.nix` (nœud disque/VM headless), `iso.nix` (ISO live/installeur graphique — GNOME, Firefox, Calamares, Flatpak) |
| `ai-daemon/` | Démon d'inférence IA en Rust (backend GGUF/llama.cpp, CUDA optionnel) |
| `firmware/patient-zero/` | Firmware ESP32 du nœud hardware MVP |
| `telemetry/` | Configuration Mosquitto (MQTT) + Telegraf → InfluxDB |
| `docs/` | Roadmap et décisions d'architecture |

## Décisions d'architecture

Voir [`docs/ROADMAP.md`](docs/ROADMAP.md) pour le détail des choix
(NixOS vs Buildroot/Yocto, mode de survie edge, backend d'inférence, etc.)
et le plan de développement par phases.

## Tester l'ISO sur matériel réel

Le dernier build validé est publié dans la release préliminaire
[`live-iso`](https://github.com/HiTechTN/HiTech-OS/releases/tag/live-iso).
Téléchargez l'ISO et son fichier `.sha256`, puis vérifiez l'intégrité avant
de flasher :

```console
sha256sum --check hitechos-live-*.iso.sha256
sudo dd if=hitechos-live-*.iso of=/dev/sdX bs=16M status=progress conv=fsync
```

Remplacez `/dev/sdX` par le périphérique USB entier, jamais par une partition
comme `/dev/sdX1`. Démarrez ensuite en mode UEFI sur la clé et choisissez
**HiTech-OS — Installateur** dans GNOME pour tester Calamares. L'image live
utilise le compte `nixos` avec le mot de passe temporaire `hitechos`; ces
identifiants ne sont valables que pour l'ISO de test et ne doivent pas être
réutilisés sur un système installé.

## Fondateur

**Mohamed Azmi Kaaniche** — [HiTechTN](https://github.com/HiTechTN) / HiTechLab

## Licence

AGPL v3
