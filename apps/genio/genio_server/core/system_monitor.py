"""Genio system awareness — read-only telemetry collector.

Phase 1 of the "nervous system": gives the agent a cheap, safe, bounded view of
the host. Every probe is individually guarded so one failing source degrades a
single field instead of breaking the snapshot.

Invariants (see docs/audit/GENIO_MASTER_ARCHITECTURE.md §6.3):
  * read-only — this module never mutates system state;
  * no shell — external commands are allowlisted argv lists, timeout-bounded;
  * bounded — TTL cache, capped lists, no unbounded /proc walks;
  * no secrets — environment variables are never collected.

Public API: snapshot(force=False) -> dict
"""

from __future__ import annotations

import logging
import os
import platform
import shutil
import socket
import subprocess
import threading
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

log = logging.getLogger("genio.telemetry")

SCHEMA = "genio.telemetry/v1"
CACHE_TTL_S = float(os.getenv("GENIO_TELEMETRY_TTL", "2.0"))
CMD_TIMEOUT_S = float(os.getenv("GENIO_TELEMETRY_CMD_TIMEOUT", "2.0"))
MAX_LIST = 50

try:  # optional: /proc fallbacks cover the degraded path
    import psutil
except ImportError:  # pragma: no cover - exercised only on minimal images
    psutil = None  # type: ignore[assignment]

_SYSTEMCTL = shutil.which("systemctl")
_MIN_ENV = {
    "PATH": os.environ.get("PATH", "/run/current-system/sw/bin:/usr/bin:/bin"),
    "LANG": "C",
    "LC_ALL": "C",
}

_lock = threading.Lock()
_cache: tuple[float, dict[str, Any]] | None = None
_primed = False


# --------------------------------------------------------------------------- util
def _run(argv: list[str]) -> tuple[int, str, str]:
    """Execute an allowlisted, shell-free command with a hard timeout."""
    try:
        proc = subprocess.run(
            argv,
            capture_output=True,
            text=True,
            timeout=CMD_TIMEOUT_S,
            check=False,
            env=_MIN_ENV,
        )
        return proc.returncode, proc.stdout.strip(), proc.stderr.strip()
    except (OSError, subprocess.TimeoutExpired) as exc:
        return -1, "", f"{type(exc).__name__}: {exc}"


def _mb(value: float) -> float:
    return round(value / 1024 / 1024, 1)


def _iso(ts: float | None = None) -> str:
    return datetime.fromtimestamp(ts or time.time(), tz=timezone.utc).isoformat(timespec="seconds")


# ------------------------------------------------------------------------- probes
def _host() -> dict[str, Any]:
    uname = platform.uname()
    return {
        "hostname": socket.gethostname(),
        "kernel": uname.release,
        "machine": uname.machine,
        "python": platform.python_version(),
        "nixos_generation": _current_system(),
    }


def _current_system() -> str | None:
    """NixOS generation marker; None on non-NixOS hosts (no dangling symlink)."""
    link = Path("/run/current-system")
    try:
        if not link.exists():  # .exists() follows the symlink, unlike resolve()
            return None
        return link.resolve().name
    except OSError:
        return None


def _uptime_s() -> float | None:
    if psutil is not None:
        try:
            return round(time.time() - psutil.boot_time(), 1)
        except Exception:  # pragma: no cover
            pass
    try:
        return round(float(Path("/proc/uptime").read_text().split()[0]), 1)
    except (OSError, ValueError, IndexError):
        return None


def _load() -> dict[str, float] | None:
    try:
        one, five, fifteen = os.getloadavg()
        return {"1m": round(one, 2), "5m": round(five, 2), "15m": round(fifteen, 2)}
    except (OSError, AttributeError):
        return None


def _cpu() -> dict[str, Any] | None:
    global _primed
    if psutil is not None:
        try:
            if not _primed:  # first call would otherwise report 0.0
                psutil.cpu_percent(interval=None, percpu=True)
                _primed = True
                time.sleep(0.05)
            per_core = psutil.cpu_percent(interval=None, percpu=True)
            freq = psutil.cpu_freq()
            return {
                "count": psutil.cpu_count(logical=True),
                "physical": psutil.cpu_count(logical=False),
                "percent": round(sum(per_core) / len(per_core), 1) if per_core else None,
                "per_core": [round(c, 1) for c in per_core[:16]],
                "freq_mhz": round(freq.current, 0) if freq else None,
            }
        except Exception as exc:  # pragma: no cover
            log.debug("psutil cpu probe failed: %s", exc)
    try:  # degraded fallback: 1-minute load normalised by core count
        cores = os.cpu_count() or 1
        pct = min(100.0, os.getloadavg()[0] / cores * 100)
        return {"count": cores, "physical": None, "percent": round(pct, 1),
                "per_core": None, "freq_mhz": None}
    except (OSError, AttributeError):
        return None


def _memory() -> dict[str, Any] | None:
    if psutil is not None:
        try:
            vm = psutil.virtual_memory()
            sw = psutil.swap_memory()
            return {
                "total_mb": _mb(vm.total),
                "used_mb": _mb(vm.total - vm.available),
                "available_mb": _mb(vm.available),
                "percent": round(vm.percent, 1),
                "swap_total_mb": _mb(sw.total),
                "swap_used_mb": _mb(sw.used),
            }
        except Exception as exc:  # pragma: no cover
            log.debug("psutil memory probe failed: %s", exc)
    try:  # /proc/meminfo fallback (values in kB)
        info: dict[str, int] = {}
        for line in Path("/proc/meminfo").read_text().splitlines():
            key, _, rest = line.partition(":")
            info[key] = int(rest.strip().split()[0])
        total, avail = info["MemTotal"] * 1024, info["MemAvailable"] * 1024
        return {
            "total_mb": _mb(total),
            "used_mb": _mb(total - avail),
            "available_mb": _mb(avail),
            "percent": round((total - avail) / total * 100, 1),
            "swap_total_mb": _mb(info.get("SwapTotal", 0) * 1024),
            "swap_used_mb": _mb((info.get("SwapTotal", 0) - info.get("SwapFree", 0)) * 1024),
        }
    except (OSError, KeyError, ValueError):
        return None


def _disks() -> dict[str, Any] | None:
    mounts: dict[str, Any] = {}
    for mount in ("/", "/data", "/nix"):
        try:
            usage = shutil.disk_usage(mount)
        except OSError:
            continue
        mounts[mount] = {
            "total_gb": round(usage.total / 1024**3, 1),
            "used_gb": round(usage.used / 1024**3, 1),
            "free_gb": round(usage.free / 1024**3, 1),
            "percent": round(usage.used / usage.total * 100, 1) if usage.total else None,
        }
    return mounts or None


def _services() -> dict[str, Any] | None:
    if not _SYSTEMCTL:
        return None
    base = [_SYSTEMCTL, "list-units", "--type=service", "--no-legend",
            "--plain", "--no-pager"]
    rc_run, out_run, err_run = _run(base + ["--state=running"])
    rc_fail, out_fail, err_fail = _run(base + ["--state=failed"])
    if rc_run != 0 or rc_fail != 0:
        return {"error": (err_run or err_fail or "systemctl unavailable")[:200]}
    running = [ln.split()[0] for ln in out_run.splitlines() if ln.strip()]
    failed = [ln.split()[0] for ln in out_fail.splitlines() if ln.strip()]
    return {
        "running_count": len(running),
        "failed_count": len(failed),
        "failed_units": failed[:MAX_LIST],
        "sample": running[:MAX_LIST],
    }


def _processes() -> dict[str, Any] | None:
    if psutil is not None:
        try:
            rows: list[dict[str, Any]] = []
            for proc in psutil.process_iter(["pid", "name", "cpu_percent", "memory_info"]):
                info = proc.info
                rows.append({
                    "pid": info["pid"],
                    "name": (info["name"] or "?")[:40],
                    "rss_mb": _mb(info["memory_info"].rss) if info["memory_info"] else None,
                    "cpu_percent": round(info["cpu_percent"] or 0.0, 1),
                })
            rows.sort(key=lambda r: r["rss_mb"] or 0, reverse=True)
            return {"count": len(rows), "top_by_rss": rows[:8]}
        except Exception as exc:  # pragma: no cover
            log.debug("psutil process probe failed: %s", exc)
    rc, out, _ = _run(["ps", "-eo", "pid=,comm=,pcpu=,rss=", "--sort=-rss"])
    if rc != 0:
        return None
    rows = []
    for line in out.splitlines()[:8]:
        parts = line.split(None, 3)
        if len(parts) == 4:
            rows.append({"pid": int(parts[0]), "name": parts[1][:40],
                         "cpu_percent": float(parts[2]),
                         "rss_mb": round(int(parts[3]) / 1024, 1)})
    return {"count": None, "top_by_rss": rows}


def _sensors() -> dict[str, Any] | None:
    """Thermal zones when exposed by the kernel (containers often have none)."""
    try:
        zones: dict[str, float] = {}
        for zone in sorted(Path("/sys/class/thermal").glob("thermal_zone*"))[:8]:
            try:
                label = (zone / "type").read_text().strip() or zone.name
                milli = int((zone / "temp").read_text().strip())
            except (OSError, ValueError):
                continue
            if -40_000 < milli < 200_000:
                zones[label] = round(milli / 1000, 1)
        return zones or None
    except OSError:
        return None


# --------------------------------------------------------------------------- API
def _collect() -> dict[str, Any]:
    started = time.perf_counter()
    payload: dict[str, Any] = {
        "schema": SCHEMA,
        "timestamp": _iso(),
        "host": _host(),
        "uptime_s": _uptime_s(),
        "load": _load(),
        "cpu": _cpu(),
        "memory": _memory(),
        "disks": _disks(),
        "services": _services(),
        "processes": _processes(),
        "sensors": _sensors(),
    }
    payload["collector_ms"] = round((time.perf_counter() - started) * 1000, 2)
    return payload


def snapshot(force: bool = False) -> dict[str, Any]:
    """Return a telemetry snapshot, cached for CACHE_TTL_S seconds.

    Never raises: a catastrophic failure returns a minimal error payload so the
    daemon's loop and /health keep working.
    """
    global _cache
    now = time.monotonic()
    with _lock:
        if not force and _cache and now - _cache[0] < CACHE_TTL_S:
            return _cache[1]
    try:
        data = _collect()
    except Exception as exc:  # pragma: no cover - last-resort guard
        log.exception("telemetry collection failed")
        data = {"schema": SCHEMA, "timestamp": _iso(), "error": str(exc)[:200]}
    with _lock:
        _cache = (time.monotonic(), data)
    return data


if __name__ == "__main__":  # manual probe: python -m core.system_monitor
    import json
    print(json.dumps(snapshot(force=True), indent=2, ensure_ascii=False))
