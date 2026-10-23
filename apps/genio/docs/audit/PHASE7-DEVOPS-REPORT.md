# Phase 7 — Calamares, Genio and HiTech Store as native NixOS services

**Date:** 2026-10-23
**Status:** Nix evaluation and builds verified; physical Calamares install not run in this environment
**Scope:** `os/calamares-hitechos.nix`, `os/common.nix`, `os/installation.nix`, `os/services/{genio,hitech-store,secrets}.nix`, `apps/genio/default.nix`, `apps/hitech-store/{app.py,static/app.js,templates/store.html}`

---

## 1. Root cause of the installer failure

The live ISO already carried `git` (`os/iso.nix`, commit `9fa87b62`). The
manual `shellprocess@hitechos` job nevertheless ran **chrooted into the
freshly installed NixOS target** (`dontChroot: false`). That target contains
only the base Calamares installation, not `git`, so the clone failed and
Calamares aborted the remaining jobs.

The previous code also imported only `os/common.nix` into the target, so the
installed system had no `ecosystem`, `hitech-store` or Genio module.

## 2. Calamares fix

`os/calamares-hitechos.nix` now:

1. runs the job in the live environment (`dontChroot: true`) with a 3600 s
   timeout;
2. executes a store-backed helper with `${ROOT}` (the target mount point);
3. clones `HiTechTN/HiTech-OS` into `${ROOT}/etc/nixos/hitech-os`;
4. inserts the relative import `./hitech-os/os/installation.nix` after
   `hardware-configuration.nix` (valid both during live `nixos-install`
   evaluation and on first boot);
5. runs `nixos-install --root "${ROOT}" --no-root-passwd` from the live
   system, so neither the target's `git` nor its `nixos-rebuild` is required.

`git` and `curl` are also part of `os/common.nix`, so the installed target is
usable for later rebuilds. The ISO keeps its own git for the installer.

## 3. Native service modules

### Installation profile

`os/installation.nix` is the single post-install entry point. It imports:

- `common.nix` — base, hardening, MQTT/InfluxDB, git/curl;
- `ecosystem.nix` — Docker/AgentOS/app2nix tooling;
- `services/hitech-store.nix`;
- `services/genio.nix`;
- `services/secrets.nix`.

`os/configuration.nix` now imports this profile, so patient-zero and every
Calamares-installed node evaluate the same service set.

### Genio

`apps/genio/default.nix` builds:

- the React/Vite dashboard (`buildNpmPackage`, fixed `npmDepsHash`);
- a Python environment with FastAPI, Uvicorn, psutil and Rich;
- `$out/share/genio/genio_server`, `$out/share/genio/genio_web/dist`, and
  the `genio` / `genio-daemon` wrappers.

`services.genio` provides:

- a dedicated `genio` user and `/var/lib/genio` state tree
  (`memory`, `audit`, `files`, `tmp`);
- a persistent capability key (`/var/lib/genio/cap.key`) generated once by
  `genio-keygen.service` (no insecure development key in production);
- a systemd service bound to `127.0.0.1:8765` by default, serving `/ui/`;
- `EnvironmentFile=/run/secrets/genio.env` for store/OpenRouter tokens;
- a narrow polkit rule allowing restarts of `hitech-store.service` and
  `ollama.service` only (Genio itself remains protected);
- Ollama enabled on loopback, with `loadModels = [ "qwen2.5:7b-instruct" ]`
  when `pullModel = true`. Model weights are downloaded on first boot and
  are intentionally not part of the Nix closure.

### HiTech Store

`services.hitech-store` is now option-driven (enabled by default):

- `stateDir` (default `/data/hitech-store`), `listenAddress`, `port`;
- `devOtpFallback`, `cookieSecure`, `openFirewall`;
- `environmentFile` (default `/run/secrets/hitech-store.env`);
- tmpfiles ownership for storage, logs, OTP/shares state and the secret
  placeholder.

The admin panel was also repaired at the application level:
`hitech_admin_token` was hard-coded `Secure=True`; over plain HTTP localhost
the browser stored it but never sent it back, so the UI appeared logged in
while every request was anonymous. It is now controlled by `COOKIE_SECURE`
(default `0` for local HTTP; set `1` behind HTTPS) and verified end-to-end.

### GitHub OAuth

The vendored store had no GitHub authentication code. Phase 7 adds an
opt-in, allow-list-only OAuth flow:

- `GET /auth/github` → GitHub authorize URL with a signed, 10-minute state;
- `GET /auth/github/callback` → code exchange, `/user` lookup, login
  allow-list check, then the same 12-hour admin JWT/cookie as OTP;
- disabled (404) unless `GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET` and
  `GITHUB_ALLOWED_LOGIN` are all set;
- the admin dialog shows the GitHub button only when
  `/api/auth/me` reports `github_oauth: true`.

This is an additional login path, not a replacement for the existing OTP.
No GitHub client secret is committed or baked into the ISO.

### sops secrets

`os/services/secrets.nix` runs a oneshot `hitechos-secrets.service` before
the store and Genio. If present, it decrypts:

- `/etc/hitechos/hitech-store.env.enc`
- `/etc/hitechos/genio.env.enc`

using `/etc/hitechos/age.key` and writes root-owned `0640` files to
`/run/secrets/`. Without encrypted files, it is a no-op and the tmpfiles
placeholders keep the services bootable in explicit degraded mode. This
avoids a flake-only `sops-nix` import, which cannot be resolved by the
non-flake `/etc/nixos/configuration.nix` generated by Calamares.

## 4. Verification performed

| Check | Result |
|---|---|
| `nix flake check --no-build` | all checks passed |
| `nix eval .#nixosConfigurations.patient-zero...toplevel.drvPath` | OK |
| `nix build ...patient-zero...toplevel` | built successfully |
| `nix eval .#nixosConfigurations.iso...isoImage.drvPath` | OK |
| Genio package build (`npm run build` + Python layout) | OK |
| Genio runtime smoke test (`/health`, `/ui/`) | 200 / 200, 9 tools, audit writable |
| HiTech Store package build | OK |
| Store `/store/health` | 200 |
| OTP login over HTTP + `/api/auth/me` | `admin: true`, cookie no longer `Secure` |
| GitHub OAuth disabled | 404 |
| GitHub OAuth enabled | 302 to GitHub; invalid callback state 400 |
| Generated Calamares `shellprocess-hitechos.conf` | `dontChroot: true`, `${ROOT}` preserved, 3600 s timeout |
| Generated `genio.service` / `hitech-store.service` | correct users, env files, state paths |

## 5. Remaining deployment work

- Run a real Calamares installation on hardware/VM. This environment can
  evaluate and build the ISO but cannot exercise the graphical installer.
- Create a GitHub OAuth app and put its client ID/secret + allowed login in
  the sops-managed store env.
- Create `/etc/hitechos/age.key` and encrypt the two dotenv files with sops
  on the target (or provision them through existing deployment automation).
- Provide the real SMTP credentials in `hitech-store.env.enc` to replace the
  OTP journal fallback; set `devOtpFallback = false` for production.
- Ensure first boot has network access for the git clone, package closure
  downloads and the Ollama model pull.
- The dashboard is loopback-only until a reverse proxy/auth layer is added;
  do not enable `openFirewall` on an untrusted network.
