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

## Qualité, tests et CI

La CI (`.github/workflows/ci.yml`) exécute, à chaque push/PR :

- **Rust** (`ai-daemon`, `apps/hitech-shell`, `hitech-installer`) :
  `cargo fmt --check`, `cargo clippy --all-targets -D warnings`, `cargo test`.
- **Python** : `pytest` sur `apps/genio` (162 tests) et `apps/hitech-store`
  (14 tests d'auth/API/sécurité).
- **Frontend** : `tsc --noEmit && vite build` pour `apps/genio/genio_web`.
- **Nix** : `nix flake check` (construit et teste les paquets),
  `patient-zero`, puis l'ISO live.
- **Firmware** : `pio run` pour le nœud ESP32.

En local :

```console
# Rust
for d in ai-daemon apps/hitech-shell hitech-installer; do
  (cd "$d" && cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test)
done

# Python
(cd apps/genio && python -m pytest -q)
(cd apps/hitech-store && python -m pytest -q)

# Frontend
(cd apps/genio/genio_web && npm ci && npm run build)

# Nix (construit hitech-shell, hitech-installer et ai-daemon + tests)
nix flake check
```

L'audit de sécurité de la Phase 7 (constats, correctifs et actions de
déploiement restantes) est dans
[`docs/audit/PHASE7-SECURITY-AUDIT.md`](docs/audit/PHASE7-SECURITY-AUDIT.md).

## Tester l'ISO sur matériel réel

Le dernier build validé est publié dans la release préliminaire
[`live-iso`](https://github.com/HiTechTN/HiTech-OS/releases/tag/live-iso).
Comme l'image graphique dépasse la limite de taille d'un asset GitHub,
téléchargez toutes les parties `.iso.part-*` ainsi que le fichier `.sha256`,
puis reconstruisez et vérifiez l'image :

```console
ISO_PREFIX=$(printf '%s\n' *.iso.part-00 | sed 's/\.part-00$//')
cat "$ISO_PREFIX".part-* > "$ISO_PREFIX"
sha256sum --check "$ISO_PREFIX.sha256"
sudo dd if="$ISO_PREFIX" of=/dev/sdX bs=16M status=progress conv=fsync
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
