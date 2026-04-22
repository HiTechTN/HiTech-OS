# 🦀 Vibe-OS 🐍

> **Un système hybride Rust-Python boosté par l'IA locale.** > *Performance brute, flexibilité infinie et intelligence intégrée.*

---

## 📌 Vue d'ensemble

**Vibe-OS** est un projet d'exploration de système d'exploitation moderne. Il ne cherche pas à remplacer votre OS actuel, mais à offrir un environnement de développement et d'automatisation "Full-Stack" taillé pour le **Vibe Coding**.

L'architecture repose sur un duo complémentaire :
- **Moteur Rust :** Pour la gestion du matériel (VGA, interruptions, mémoire) et la performance système sans compromis.
- **Intelligence Python :** Pour le scripting rapide, l'automatisation et l'intégration de modèles d'IA (via Ollama).

## 🏗️ Architecture

Le projet est structuré en plusieurs modules (crates) pour garantir la propreté du code :

- **/crates/vibe-kernel** : Le cœur du système (Bare-metal Rust).
- **/crates/vibe-vga** : Moteur d'affichage et rendu texte.
- **/python** : Scripts d'automatisation, intégration de l'IA et REPL interactif.
- **/docs** : Documentation technique et roadmap détaillée.

## 🚀 Vision & Roadmap (24 Semaines)

Le développement est découpé en phases progressives :

1.  **Phase 1 : Fondations (Semaines 1-5)** – Stabilisation du Kernel Rust, IDT, GDT et affichage VGA.
2.  **Phase 2 : Langage & Scripting (Semaines 6-11)** – Intégration de Python comme interpréteur de commandes système.
3.  **Phase 3 : Agent IA (Semaines 12-14)** – Connexion native à l'API Ollama pour le refactoring et l'aide au codage.
4.  **Phase 4 : Interface & Écosystème (Semaines 15-24)** – Shell TUI avancé (Ratatui) et gestionnaire de paquets.

## 🛠️ Configuration du Développement

Ce projet utilise **Nix** pour garantir un environnement de développement identique partout.

```bash
# Pour entrer dans l'environnement de développement
nix develop

# Pour compiler le kernel Rust
cargo build

# Pour lancer l'OS dans QEMU
cargo run
