# HiTech-OS — Roadmap & Décisions d'Architecture (v2, post-refonte)

*Document de référence pour le pivot Vibe-OS → HiTech-OS*
*Fondateur : Mohamed Azmi Kaaniche — HiTechTN / HiTechLab*

---

## 1. Décisions d'architecture (réponses au RFC)

### 1.1 Base système : NixOS minimal (pas Buildroot/Yocto)

Vous utilisez déjà des flakes Nix pour la reproductibilité — autant construire directement sur **NixOS** plutôt que sur un Linux générique piloté par Nix en périphérie.

- **Pourquoi** : NixOS donne nativement les rollbacks atomiques et les générations immuables (base de l'"Immutable Core"), sans avoir à réinventer un mécanisme OTA par-dessus Buildroot/Yocto.
- **Cible** : configuration NixOS *headless*, kernel reconfiguré (désactivation des modules inutiles : audio, Bluetooth, GPU desktop, etc.) pour viser <150 Mo RAM idle.
- **Conteneurisation** : `systemd-nspawn` en premier choix (déjà intégré à systemd/NixOS, empreinte minimale) ; Podman rootless en option si l'écosystème OCI est nécessaire côté B2B.

### 1.2 Moteur d'inférence IA : Rust-first, backend modulaire

Réponse au RFC #1 (Rust pur vs C++/CUDA) : **les deux, mais pas au même niveau**.

- **Cœur du démon** en Rust (sécurité mémoire, latence prévisible, pas de GC).
- **Backend d'inférence** : bindings vers `llama.cpp` (format **GGUF**, support natif 4-bit/2-bit, `mmap` déjà intégré pour le chargement zero-copy — inutile de le réécrire).
- **Abstraction de backend** derrière un trait Rust (`InferenceBackend`) : un backend CPU/edge (ggml) par défaut, et un backend CUDA (via wrapper C++/FFI) activable en feature flag pour les configurations serveur hybride B2B.
- Ça répond aux deux cas d'usage sans forcer un choix binaire dès le départ.

### 1.3 Mode de survie (RFC #2) : autonomie locale par défaut

Si le broker MQTT central devient injoignable (partition réseau) :

1. Chaque nœud ESP32 conserve **localement** ses seuils critiques (ex. 30°C → relais de refroidissement) et continue à les appliquer sans dépendre du broker.
2. Les lectures sont **bufferisées en mémoire volatile** (RTC RAM, qui survit aux soft reboots — ou simplement RAM classique) pendant la coupure. **Pas de Flash** : le nœud est alimenté en permanence par la station de batterie 24V, donc la persistance survivant à une coupure d'alimentation n'est pas nécessaire, et l'écriture continue sur Flash en cas de coupure réseau longue userait prématurément la puce (cycles d'écriture limités — *hardware trap* à éviter).
3. Reconnexion en **backoff exponentiel**, puis réémission du buffer à la reprise.
4. Aucune action destructive côté agent central tant que la télémétrie n'est pas retrouvée fraîche (évite les décisions sur données obsolètes).

C'est cohérent avec la philosophie "sovereign / local-first" : le nœud doit rester fonctionnel même isolé.

### 1.4 Télémétrie

- **Bus** : Mosquitto (déjà retenu), schéma de topics par convention `hitechos/<site>/<node_id>/<capteur>`.
- **Persistance** : Telegraf en pont MQTT → InfluxDB (évite d'écrire un consumer maison).
- **AgentOS** consomme InfluxDB pour le contexte historique + s'abonne en direct à MQTT pour le temps réel.

### 1.5 Firmware ESP32 (Patient Zero)

- **Prototypage rapide** : Arduino framework via PlatformIO (flash Web Serial API déjà prévu).
- **Migration prévue** vers ESP-IDF une fois le firmware stabilisé, pour réduire l'empreinte et gagner en contrôle bas niveau (utile si le nombre de nœuds B2C grandit).

---

## 2. Roadmap (phases, pas de durée figée)

### Phase 0 — Fondations du nouveau repo
- Restructuration du dépôt GitHub (renommage, arborescence `os/`, `ai-daemon/`, `firmware/`, `telemetry/`)
- `flake.nix` racine : environnements de dev séparés par composant
- CI de base (build Nix, lint Rust, compile firmware)

### Phase 1 — OS Core (NixOS minimal)
- Configuration NixOS headless, kernel allégé
- Mesure de l'empreinte RAM idle, itération jusqu'à <150 Mo
- Intégration `systemd-nspawn`, premier conteneur de test

### Phase 2 — Système nerveux (Télémétrie)
- Déploiement Mosquitto + schéma de topics
- Pont Telegraf → InfluxDB
- Dashboard de vérification (Grafana ou équivalent léger)

### Phase 3 — Patient Zero (hardware MVP)
- Firmware ESP32 : lecture DHT22, affichage OLED, publication MQTT
- Lecture JK BMS via UART (tension, courant, état des cellules)
- Logique de relais de refroidissement (seuil 30°C) + mode survie local

### Phase 4 — Démon IA v0
- Squelette Rust du démon, trait `InferenceBackend`
- Intégration `llama.cpp`/GGUF, chargement `mmap`, benchmark temps de boot
- Test avec un modèle quantifié 4-bit sur le hardware cible

### Phase 5 — AgentOS (autonomie)
- Boucle agent : lecture contexte (InfluxDB + MQTT live) → décision → action
- Règles déterministes d'abord (seuils), raisonnement LLM en complément ensuite
- Journalisation des décisions (auditabilité, important pour le B2B sécurité)

### Phase 6 — OTA & immutabilité
- Mécanisme de mise à jour via générations Nix
- Test de rollback (mise à jour cassée → retour automatique)
- Signature des builds

### Phase 7 — Durcissement & mode survie
- Tests de partition réseau réels (coupure MQTT, coupure WAN)
- Audit sécurité de base (surface d'attaque, permissions conteneurs)

### Phase 8 — Packaging B2C
- Image "kit maison connectée" préconfigurée
- Provisioning simplifié (QR code / appli mobile ?)

### Phase 9 — Packaging B2B
- Fonctionnalités infra entreprise (multi-nœuds, gestion de flotte, sécurité électrique)
- Documentation orientée intégrateurs

### Phase 10 — Beta publique & RFC
- Ouverture du dépôt, retours communauté sur les deux questions RFC restées ouvertes
- Itération selon les retours

---

## 3. État d'avancement (mis à jour au fil des sessions)

| Phase | Statut | Détail |
|---|---|---|
| 0 — Fondations | ✅ Fait | Structure repo, `flake.nix`, CI de base |
| 1 — OS Core | ✅ Fait, testé (nœud) / ⚙️ config écrite, non testée (ISO) | `configuration.nix` bootable, testé sur `nix build .#...build.vm` réel, SSH fonctionnel. ISO live/installeur (`os/iso.nix`) : GNOME + Firefox + Calamares + Flatpak/Flathub, générée à chaque push par la CI (`iso-build`) et publiée en Release GitHub (`live-iso`) ; jamais testée sur matériel physique réel. **Limite à connaître** : Calamares installe un NixOS générique — appliquer ensuite `nixos-rebuild switch --flake .#patient-zero` pour obtenir les services HiTech-OS sur le disque. Mots de passe console (`nixos`/`hitechos`) volontairement faibles, valables uniquement sur cette image live éphémère. |
| 2 — Télémétrie | ⚙️ Config écrite, non testée en réel | Mosquitto (auth par utilisateur) + Telegraf + InfluxDB déclarés comme services NixOS ; secrets (mot de passe MQTT, token InfluxDB) à déposer manuellement sur le nœud, pas encore de gestion de secrets chiffrés |
| 3 — Patient Zero (firmware) | ⚙️ Code écrit, jamais flashé sur un ESP32 réel | Lecture DHT22/OLED/MQTT/UART BMS implémentée avec vraies libs, backoff de reconnexion, buffer RTC RAM ; CI compile le firmware (avec des secrets placeholder) mais rien n'a tourné sur du hardware physique |
| 4 — Démon IA | 🔲 Squelette seulement | Trait `InferenceBackend` posé, mais pas d'intégration GGUF/llama.cpp réelle — nécessite de choisir/tester un binding Rust et d'avoir un modèle quantifié sous la main |
| 5 — AgentOS | 🔲 Non commencé | Dépend de 2, 3 et 4 étant réellement opérationnels (il faut de la vraie donnée à consommer) |
| 6 — OTA & immutabilité | ⚙️ Config écrite, non testée | `system.autoUpgrade` déclaré (pull depuis le flake GitHub), rollback = mécanisme natif NixOS (générations de boot) ; jamais testé en conditions réelles (ni un vrai cycle update→rollback) |
| 7 — Durcissement | ⚙️ Bases posées | SSH root désactivé, sudo avec mot de passe, firewall explicite, auditd activé ; **pas d'audit de sécurité réel effectué** |
| 8 — Packaging B2C | 🔲 Non commencé | |
| 9 — Packaging B2B | 🔲 Non commencé | |
| 10 — Beta publique | 🔲 Non commencé | |

**Ce qui bloque une vraie progression sur 4/5/8/9/10** : ce sont des étapes qui demandent soit du matériel physique (ESP32, BMS, station de batterie), soit un modèle IA + GPU/edge device pour tester l'inférence, soit des décisions produit (tarification, UX de provisioning) qui ne se codent pas à l'aveugle. La CI valide que le code *compile*, pas qu'il *fonctionne* sur le terrain.

---

## 4. Notes
- L'ancien roadmap Vibe-OS (kernel bare-metal x86_64 en Rust pur, GDT/IDT/VGA/PS2) est **abandonné** — ce travail ne sert plus de base technique pour HiTech-OS.
- Les phases ci-dessus ne sont pas datées : à affiner une fois que Phase 0 et 1 auront donné une idée de vitesse réelle.
