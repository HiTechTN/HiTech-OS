"""Capability tokens — HMAC-signed, scoped, TTL-bounded, nonce-tracked.

Two token kinds share one format (`g1.<payload>.<sig>`):

  * ``session``  — reusable within its TTL, carries a list of capability
                   strings (``telemetry:read``, ``fs:write``, ...). Issued by the
                   daemon to a local client.
  * ``approval`` — single-use, bound to one tool id + one arguments hash,
                   issued only after a human grants a pending R2 request.

Security properties:
  * HS256-style HMAC over canonical JSON (sorted keys, compact separators);
  * constant-time signature comparison;
  * ``exp`` enforced with a 0 s clock skew allowance;
  * nonces of single-use tokens are remembered for their TTL: a second use
    raises :class:`ReplayDetected` (the "used nonce" test);
  * the key never leaves the process, is never logged, and is read from
    ``GENIO_CAP_KEY`` (hex) / ``GENIO_CAP_KEY_FILE`` with a loud dev fallback.

This module has no third-party imports and is safe to unit-test in isolation.
"""

from __future__ import annotations

import base64
import hashlib
import hmac
import json
import logging
import os
import secrets
import threading
import time
from pathlib import Path
from typing import Any, Iterable

log = logging.getLogger("genio.caps")

PREFIX = "g1"
DEV_KEY = b"genio-dev-insecure-key-do-not-use-in-production"
_clock_skew_s = 0


# --------------------------------------------------------------------------- errors
class CapError(Exception):
    """Base class for capability failures."""


class TokenInvalid(CapError):
    """Malformed token, bad signature or unknown version."""


class TokenExpired(CapError):
    """Token TTL elapsed."""


class CapabilityDenied(CapError):
    """Token is valid but does not grant the required capability."""


class ReplayDetected(CapError):
    """Single-use token (nonce) already consumed."""


# --------------------------------------------------------------------------- key
def _load_key() -> tuple[bytes, str]:
    raw = os.getenv("GENIO_CAP_KEY", "").strip()
    key_file = os.getenv("GENIO_CAP_KEY_FILE", "").strip()
    if not raw and key_file:
        try:
            raw = Path(key_file).read_text().strip()
        except OSError as exc:
            raise RuntimeError(f"GENIO_CAP_KEY_FILE unreadable: {exc}") from exc
    if raw:
        try:
            return bytes.fromhex(raw), "env"
        except ValueError:
            return raw.encode(), "env"
    log.warning(
        "GENIO_CAP_KEY not set — using the INSECURE development key. "
        "Set a 64-hex-char key in /run/secrets/genio.env before deployment."
    )
    return DEV_KEY, "dev"


_KEY, _KEY_SOURCE = _load_key()

# --------------------------------------------------------------------------- nonces
_nonce_lock = threading.Lock()
_used_nonces: dict[str, float] = {}  # nonce -> expiry (epoch seconds)


def _b64e(raw: bytes) -> str:
    return base64.urlsafe_b64encode(raw).rstrip(b"=").decode()


def _b64d(text: str) -> bytes:
    return base64.urlsafe_b64decode(text + "=" * (-len(text) % 4))


def _canonical(obj: Any) -> bytes:
    return json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()


def _sign(payload_b64: str) -> str:
    return _b64e(hmac.new(_KEY, f"{PREFIX}.{payload_b64}".encode(), hashlib.sha256).digest())


def _purge_nonces(now: float) -> None:
    for nonce, exp in list(_used_nonces.items()):
        if exp < now:
            _used_nonces.pop(nonce, None)


def hash_args(args: dict[str, Any]) -> str:
    """Stable digest used to bind an approval to the exact arguments."""
    return hashlib.sha256(_canonical(args or {})).hexdigest()


# --------------------------------------------------------------------------- issue
def issue(
    subject: str,
    caps: Iterable[str],
    ttl_s: int = 900,
    *,
    kind: str = "session",
    reusable: bool = True,
    tool: str | None = None,
    args_hash: str | None = None,
    meta: dict[str, Any] | None = None,
) -> str:
    """Mint a signed token. ``kind='approval'`` forces single-use."""
    if kind not in ("session", "approval"):
        raise ValueError("kind must be 'session' or 'approval'")
    if kind == "approval":
        reusable = False
        if not tool or not args_hash:
            raise ValueError("approval tokens require tool + args_hash")
    now = int(time.time())
    payload: dict[str, Any] = {
        "v": 1,
        "kind": kind,
        "sub": subject,
        "caps": sorted(set(caps)),
        "iat": now,
        "exp": now + max(1, int(ttl_s)),
        "nonce": secrets.token_hex(16),
        "reuse": bool(reusable),
        "key": _KEY_SOURCE,
    }
    if tool:
        payload["tool"] = tool
    if args_hash:
        payload["args_hash"] = args_hash
    if meta:
        payload["meta"] = meta
    body = _b64e(_canonical(payload))
    return f"{PREFIX}.{body}.{_sign(body)}"


# --------------------------------------------------------------------------- verify
def verify(token: str, required_cap: str | None = None, *, consume: bool = True) -> dict[str, Any]:
    """Validate a token and (for single-use kinds) burn its nonce.

    Raises TokenInvalid / TokenExpired / CapabilityDenied / ReplayDetected.
    """
    if not token or token.count(".") != 2:
        raise TokenInvalid("malformed token")
    prefix, body, sig = token.split(".")
    if prefix != PREFIX:
        raise TokenInvalid("unsupported token version")
    expected = _sign(body)
    if not hmac.compare_digest(expected, sig):
        raise TokenInvalid("bad signature")
    try:
        payload = json.loads(_b64d(body))
    except (ValueError, json.JSONDecodeError) as exc:
        raise TokenInvalid("undecodable payload") from exc
    if not isinstance(payload, dict) or payload.get("v") != 1:
        raise TokenInvalid("unsupported payload version")
    now = int(time.time())
    if int(payload.get("exp", 0)) + _clock_skew_s < now:
        raise TokenExpired("token expired")
    if required_cap and required_cap not in payload.get("caps", []) and "*" not in payload.get("caps", []):
        raise CapabilityDenied(f"missing capability {required_cap!r}")

    if not payload.get("reuse", False):
        nonce = payload.get("nonce")
        if not nonce:
            raise TokenInvalid("single-use token without nonce")
        with _nonce_lock:
            _purge_nonces(now)
            if nonce in _used_nonces:
                raise ReplayDetected("nonce already consumed")
            if consume:
                _used_nonces[nonce] = float(payload["exp"])
    return payload


def reset_nonce_cache() -> None:
    """Test hook only."""
    with _nonce_lock:
        _used_nonces.clear()


def key_source() -> str:
    return _KEY_SOURCE
