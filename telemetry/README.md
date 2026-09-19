# Télémétrie — état (Phase 2)

La configuration réelle et active de Mosquitto/Telegraf/InfluxDB vit
désormais dans `os/configuration.nix` (services NixOS déclaratifs),
et non dans ce dossier.

Les fichiers `mosquitto.conf` / `telegraf.conf` ci-dessous sont
conservés comme **référence indépendante de NixOS** — utile pour un
déploiement hors NixOS (ex. conteneur Docker autonome) — mais ne sont
pas chargés par le nœud HiTech-OS lui-même.

## Secrets

Le mot de passe MQTT et le token InfluxDB ne sont **jamais** committés.
Ils sont attendus à :
- `/run/secrets/mosquitto-hitechos-agent` (mot de passe hashé, format `mosquitto_passwd`)
- `/run/secrets/telegraf-influx-token` (fichier d'env avec `INFLUX_TOKEN=...`)

Ces fichiers sont pour l'instant à déposer manuellement sur le nœud.
Leur gestion propre (chiffrés dans le dépôt, déployés au build) est
prévue en Phase 6 via `sops-nix` ou `agenix` — pas encore fait.
