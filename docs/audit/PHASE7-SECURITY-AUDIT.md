# Phase 7 — Audit de sécurité initial

*Date : 2026-10-31 — périmètre : `os/` (NixOS), `apps/hitech-store`, `apps/genio`,
`hitech-installer`, `ai-daemon`, `.github/workflows/`.*

Ce document clôt l'item « audit de sécurité réel » resté ouvert en Phase 7.
Il ne remplace pas un pentest : c'est une revue de configuration et de code
ciblée sur la surface d'attaque réellement exposée par le nœud et l'ISO.

## 0. Résumé

| Sévérité | Nombre | État |
|---|---|---|
| Critique | 0 | — |
| Élevée | 2 | 1 corrigée, 1 action de déploiement |
| Moyenne | 4 | 1 corrigée, 3 documentées |
| Faible | 3 | documentées |
| Informatif | 4 | — |

Deux bugs fonctionnels découverts pendant l'audit ont été corrigés au passage :
un `TemplateResponse` incompatible avec Starlette ≥ 0.29 (cassait `/store`) et
`git clone --branch <sha>` dans l'installeur (cassait toute installation depuis
une ISO de release dont `self.rev` est un commit).

## 1. Méthodologie

- Lecture de tous les modules NixOS déployés (`common.nix`, `installation.nix`,
  `services/*.nix`, `iso.nix`, `netinstaller.nix`).
- Revue des points d'entrée réseau : MQTT (1883), hitech-store (8095),
  genio (8765), ollama (11434), Harmonia (5000), SSH (22).
- Revue du code des services exposés (injection shell, traversée de chemin,
  authentification, secrets).
- Tests automatisés ajoutés pour verrouiller les comportements sensibles
  (auth OTP, isolation `private/`/`agents/`, traversée de chemin, échappement
  Nix/JSON).
- Vérification des durcissements systemd, des permissions de secrets et du
  modèle de menaces de l'ISO live.

## 2. Constats

### F-01 — ÉLEVÉ — Clé de signature de session `hitech-store` connue — **CORRIGÉ**

**Constat.** `systemd-tmpfiles` créait `/run/secrets/hitech-store.env` avec
`SECRET_KEY=dev-only-placeholder-change-me`. Cette clé signe le JWT admin
(`make_session_token`) : quiconque peut joindre le port 8095 pouvait forger un
cookie administrateur sans passer l'OTP.

**Correctif.** Nouveau oneshot `hitech-store-keygen`
(`os/services/hitech-store.nix`) exécuté après `hitechos-secrets` et avant
`hitech-store`. Si la valeur est vide ou encore le placeholder, il génère une
clé aléatoire par boot (`openssl rand -hex 32`) et préserve owner/group/mode du
fichier. Une clé fournie par sops n'est jamais écrasée. La session est
invalidée à chaque reboot (acceptable : TTL 12 h).

**Reste à faire.** Générer un vrai `SECRET_KEY` persistant via sops pour les
déploiements où les sessions doivent survivre à un reboot.

### F-02 — ÉLEVÉ — Mot de passe MQTT placeholder + port 1883 ouvert — **ACTION DE DÉPLOIEMENT**

**Constat.** `common.nix` ouvre `1883/tcp` par défaut et le fichier
`/run/secrets/mosquitto-hitechos-node` contient le hash sha512-crypt du mot de
passe littéral `placeholder-change-me` tant que sops n'a rien déployé. Un
attaquant réseau peut donc s'authentifier comme nœud et lire/écrire
`hitechos/#`.

**Mitigation.** C'est un choix « fail-open availability » assumé pour éviter
que le broker bloque le premier boot. Toute exposition réseau **doit**
précéder la mise en place de secrets réels.

**Recommandation.** Avant tout déploiement exposé :
1. Générer le hash réel (`mosquitto_passwd -b -c /tmp/f hitechos-node <mdp>`)
   puis le déployer via sops (sans préfixe `user:`) ;
2. Restreindre `allowedTCPPorts` à l'interface/VPN de télémétrie ou passer par
   un reverse-proxy MQTT/TLS ;
3. Vérifier `mosquitto_sub -h <host> -u hitechos-node -P placeholder-change-me`
   échoue (test négatif de non-régression).

### F-03 — MOYEN — `DEV_OTP_FALLBACK=1` par défaut — **DOCUMENTÉ**

**Constat.** `services.hitech-store.devOtpFallback` vaut `true` : si SMTP échoue,
le PIN admin est écrit dans le journal systemd. Sur un nœud où le journal est
lisible par un groupe non privilégié, cela fuit un facteur d'authentification.

**Recommandation.** Mettre `services.hitech-store.devOtpFallback = false;` en
production. Le laboratoire peut le garder à `true` (valeur par défaut actuelle)
tant que le port reste sur loopback/firewall fermé.

### F-04 — MOYEN — `hitech-store` écoute sur `0.0.0.0` — **DOCUMENTÉ**

**Constat.** `listenAddress = "0.0.0.0"` par défaut. Le firewall NixOS est
fermé (`openFirewall = false`), donc le port n'est pas exposé par défaut ; mais
ouvrir le firewall expose immédiatement l'interface complète (explorateur
public + panneau admin).

**Recommandation.** Garder `listenAddress = "127.0.0.1"` et publier via nginx
TLS avec `cookieSecure = true` lorsque l'accès distant est nécessaire.

### F-05 — MOYEN — `hitechos` dans le groupe `docker` — **DOCUMENTÉ**

**Constat.** `os/ecosystem.nix` ajoute l'administrateur au groupe `docker` pour
`docker compose up` (services locaux/app2nix). Le groupe `docker` est équivalent à root
(montage de `/` dans un conteneur privilégié).

**Recommandation.** Acceptable sur un nœud d'admin dédié. Pour un nœud exposé,
restreindre via `virtualisation.docker.rootless` ou sudo ciblé.

### F-06 — MOYEN — Mots de passe faibles sur l'ISO live — **DOCUMENTÉ**

**Constat.** `os/iso.nix` fixe `hitechos` comme mot de passe pour `nixos` et
`hitechos` (accès console de secours). SSH refuse l'authentification par mot de
passe (`PasswordAuthentication = false`), donc l'exposition est physique/live
uniquement.

**Recommandation.** Ne pas réutiliser ces comptes sur un système installé ;
l'installeur génère un compte/mot de passe propres. Ajouter un avertissement
visible sur l'écran de bienvenue (déjà documenté dans le README).

### F-07 — FAIBLE — Dashboard Genio sans authentification — **DOCUMENTÉ**

**Constat.** `services.genio.host = "127.0.0.1"` et `openFirewall = false` :
pas d'auth utilisateur, mais écoute loopback uniquement. La règle polkit
autorise le groupe `genio` à redémarrer `hitech-store.service` et
`ollama.service` seulement.

**Recommandation.** Accès distant uniquement par tunnel SSH (documenté dans le
module). Ne jamais activer `openFirewall` sans reverse-proxy authentifié.

### F-08 — FAIBLE — Pas de chiffrement de disque ni Secure Boot — **DOCUMENTÉ**

**Constat.** Aucun LUKS, aucun déploiement Secure Boot. Un accès physique
permet de modifier le système ou d'extraire les secrets de `/etc/hitechos`.

**Recommandation.** Option d'installation LUKS en Phase 9/B2B ; documenter le
modèle de menace « accès physique = compromission ».

### F-09 — FAIBLE — OTA non testé de bout en bout — **DOCUMENTÉ**

**Constat.** `system.autoUpgrade` suit `github:HiTechTN/HiTech-OS#patient-zero`
avec `allowReboot = false` et `configurationLimit = 10`. Le rollback natif
NixOS existe mais le cycle update→rollback n'a pas été exercé.

**Recommandation.** Test de rollback réel (génération cassée → sélection GRUB)
avant tout parc B2B.

### F-10 — INFO — Points conformes vérifiés

- SSH : `PermitRootLogin = "no"`, `PasswordAuthentication = false`,
  `KbdInteractiveAuthentication = false`, clés publiques uniquement.
- Sudo : `wheelNeedsPassword = true`.
- Firewall explicite activé ; auditd activé.
- Secrets : déchiffrement sops en `oneshot`, fichiers `root:<groupe>` `0640`,
  répertoire `/etc/hitechos` root-only ; aucune clé privée dans le dépôt.
- Cache binaire : la clé privée Harmonia reste hors CI, les clients vérifient
  les signatures (`trusted-public-keys`), assertion sur `publicKey`.
- Installeur : pas de shell (`Command::new` + arguments), échappement Nix/JSON
  des entrées utilisateur, chemins de périphériques validés, rejet de la
  traversée de chemin dans le store.

## 3. Correctifs fonctionnels liés à l'audit

| Bug | Impact | Correctif |
|---|---|---|
| `TemplateResponse("store.html", {...})` | `/store` renvoie 500 sur Starlette ≥ 0.29 (nixpkgs 2026), donc portail cassé | signature `(request, name, context)` — `apps/hitech-store/app.py` |
| `git clone --branch <sha>` | une ISO de release (ref = `self.rev`) ne s'installait pas | `git init` + `fetch --depth 1 origin <ref>` + `checkout --detach FETCH_HEAD` — `hitech-installer/src/main.rs` |
| `nix build .#hitech-shell` cassé | `calloop` ajouté mais `default.nix` compilait encore en `rustc` direct | passage à `rustPlatform.buildRustPackage` + `cargoLock` (tests exécutés au build) |
| Clé JWT placeholder | forge de session admin | oneshot `hitech-store-keygen` (F-01) |

## 4. Tests de non-régression ajoutés

- `apps/hitech-store/tests/test_store.py` : 14 tests (auth OTP, tentatives,
  isolation `private`/`agents`, traversée, upload/checksum, partages, token
  d'artefacts).
- `hitech-installer/src/main.rs` : 18 tests (validation de requête, échappement
  Nix/JSON, parsing `lsblk`, chemins NVMe, plan git, troncature des logs).
- `apps/hitech-shell/src/*` : 10 tests (layouts, parsing d'environnement).
- `ai-daemon/src/backend.rs` : 3 tests (backend stub, sélection).
- `apps/genio/tests` : 162 tests existants, exécutés en CI.

Total : **207 tests** exécutés en CI (hors `nix flake check`, firmware et ISO).

## 5. Priorités restantes

1. **F-02** — déployer le secret MQTT réel avant toute exposition réseau.
2. **F-03/F-04** — passer `devOtpFallback=false`, loopback + TLS en production.
3. Réaliser le test de rollback OTA (F-09) sur une VM.
4. Ajouter LUKS/Secure Boot en option (F-08) pour le parc B2B.
