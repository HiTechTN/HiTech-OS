"""Thin HTTP client for the hitech-store portal API.

Everything Genio knows about documents, directories and media comes through
this client. It is deliberately dumb: no policy, no retries beyond a timeout,
no interpretation of content. Policy lives in ``policy_engine``; parsing lives
in ``tool_registry``.

Auth (optional, for write operations and private paths):
    HITECH_STORE_ADMIN_TOKEN   admin session JWT (cookie ``hitech_admin_token``)
    HITECH_STORE_AGENT_TOKEN   shared agent token (``X-Agent-Token``)

Endpoints used:
    GET  /api/drive/list?path=…          list a directory
    GET  /api/drive/extract?path=…       server-side PDF/Office extraction
    GET  /api/drive/download?path=…      raw bytes (supports Range)
    POST /api/drive/mkdir                create a folder (admin)
"""

from __future__ import annotations

import json
import os
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from typing import Any

DEFAULT_URL = os.getenv("HITECH_STORE_URL", "http://127.0.0.1:8095").rstrip("/")
MAX_TEXT_BYTES = 2 * 1024 * 1024


class StoreError(RuntimeError):
    """The portal refused or failed a request."""

    def __init__(self, message: str, status: int | None = None) -> None:
        super().__init__(message)
        self.status = status


class StoreAuthError(StoreError):
    """The operation needs an admin/agent credential we do not have."""


class StoreNotFound(StoreError):
    """Path does not exist on the drive."""


@dataclass
class StoreConfig:
    base_url: str = DEFAULT_URL
    admin_token: str | None = None
    agent_token: str | None = None
    timeout: float = 8.0

    @property
    def auth_mode(self) -> str:
        if self.admin_token:
            return "admin"
        if self.agent_token:
            return "agent"
        return "anonymous"


class StoreClient:
    def __init__(self, base_url: str | None = None, admin_token: str | None = None,
                 agent_token: str | None = None, timeout: float = 8.0) -> None:
        self.config = StoreConfig(
            base_url=(base_url or DEFAULT_URL).rstrip("/"),
            admin_token=admin_token if admin_token is not None
            else os.getenv("HITECH_STORE_ADMIN_TOKEN") or None,
            agent_token=agent_token if agent_token is not None
            else os.getenv("HITECH_STORE_AGENT_TOKEN") or None,
            timeout=timeout,
        )

    @classmethod
    def from_env(cls) -> "StoreClient":
        return cls()

    # ------------------------------------------------------------------ http
    def _headers(self) -> dict[str, str]:
        headers: dict[str, str] = {"Accept": "application/json"}
        if self.config.admin_token:
            headers["Cookie"] = f"hitech_admin_token={self.config.admin_token}"
        if self.config.agent_token:
            headers["X-Agent-Token"] = self.config.agent_token
        return headers

    def _request(self, method: str, path: str, *, params: dict[str, str] | None = None,
                 body: dict[str, Any] | None = None, range_limit: int | None = None,
                 timeout: float | None = None) -> tuple[bytes, str]:
        url = self.config.base_url + path
        if params:
            url += "?" + urllib.parse.urlencode(params, quote_via=urllib.parse.quote)
        data = json.dumps(body).encode() if body is not None else None
        headers = self._headers()
        if data is not None:
            headers["Content-Type"] = "application/json"
        if range_limit:
            headers["Range"] = f"bytes=0-{max(0, range_limit - 1)}"
        req = urllib.request.Request(url, data=data, headers=headers, method=method)
        try:
            with urllib.request.urlopen(req, timeout=timeout or self.config.timeout) as resp:
                return resp.read(), resp.headers.get("Content-Type", "")
        except urllib.error.HTTPError as exc:
            detail = _detail(exc)
            if exc.code in (401, 403):
                raise StoreAuthError(
                    f"hitech-store refused {path} ({exc.code}): {detail}", exc.code) from exc
            if exc.code == 404:
                raise StoreNotFound(f"not found: {detail}", exc.code) from exc
            raise StoreError(f"hitech-store HTTP {exc.code}: {detail}", exc.code) from exc
        except (urllib.error.URLError, TimeoutError) as exc:
            raise StoreError(f"hitech-store unreachable at {self.config.base_url}: {exc}") from exc

    def _json(self, method: str, path: str, **kwargs: Any) -> dict[str, Any]:
        raw, _ = self._request(method, path, **kwargs)
        try:
            data = json.loads(raw or b"{}")
        except json.JSONDecodeError as exc:
            raise StoreError(f"invalid JSON from hitech-store ({path})") from exc
        if not isinstance(data, dict):
            raise StoreError(f"unexpected payload from hitech-store ({path})")
        return data

    # ------------------------------------------------------------------ api
    def list_dir(self, path: str = "") -> dict[str, Any]:
        data = self._json("GET", "/api/drive/list", params={"path": path or ""})
        return {
            "path": data.get("path", path),
            "dirs": [{"name": d.get("name"), "path": d.get("path")}
                     for d in data.get("dirs", [])][:200],
            "files": [{
                "name": f.get("name"), "path": f.get("path"), "size": f.get("size"),
                "size_str": f.get("size_str"), "mtime": f.get("mtime"),
                "sha256": f.get("sha256"), "kind": f.get("kind"),
            } for f in data.get("files", [])][:200],
        }

    def extract_document(self, path: str) -> dict[str, Any]:
        data = self._json("GET", "/api/drive/extract", params={"path": path},
                          timeout=max(self.config.timeout, 20.0))
        return {"kind": data.get("kind"), "name": data.get("name"),
                "html": data.get("html", ""), "truncated": bool(data.get("truncated"))}

    def download_text(self, path: str, limit: int = MAX_TEXT_BYTES) -> tuple[str, str]:
        raw, content_type = self._request("GET", "/api/drive/download",
                                          params={"path": path}, range_limit=limit,
                                          timeout=max(self.config.timeout, 20.0))
        charset = "utf-8"
        if "charset=" in content_type:
            charset = content_type.split("charset=", 1)[1].split(";")[0].strip() or "utf-8"
        try:
            return raw.decode(charset, errors="replace"), content_type
        except LookupError:
            return raw.decode("utf-8", errors="replace"), content_type

    def mkdir(self, parent: str, name: str) -> dict[str, Any]:
        return self._json("POST", "/api/drive/mkdir", body={"parent": parent, "name": name})

    def find_file(self, path: str) -> dict[str, Any]:
        parent, _, name = path.rpartition("/")
        listing = self.list_dir(parent)
        for item in listing["files"]:
            if item.get("name") == name:
                return item
        raise StoreNotFound(f"file not found: {path}")

    def media_info(self, path: str) -> dict[str, Any]:
        entry = self.find_file(path)
        kind = entry.get("kind") or "other"
        if kind not in ("audio", "video", "image"):
            raise StoreError(f"not a media file (kind={kind})")
        suggested = (entry.get("sha256") or "")[:16]
        return {
            **entry,
            "preview_url": f"{self.config.base_url}/api/drive/download?"
                           + urllib.parse.urlencode({"path": path}),
            "download_url": f"{self.config.base_url}/api/drive/download?"
                            + urllib.parse.urlencode({"path": path, "download": "1"}),
            "checksum_hint": suggested,
        }

    def health(self) -> dict[str, Any]:
        data = self._json("GET", "/store/health", timeout=3.0)
        return {"ok": bool(data.get("ok", True)), "base_url": self.config.base_url,
                "auth_mode": self.config.auth_mode, **data}

    def describe(self) -> dict[str, Any]:
        return {"base_url": self.config.base_url, "auth_mode": self.config.auth_mode,
                "timeout_s": self.config.timeout}


def _detail(exc: urllib.error.HTTPError) -> str:
    try:
        raw = exc.read()[:400].decode(errors="replace")
    except Exception:
        return exc.reason or "no detail"
    try:
        return str(json.loads(raw).get("detail", raw))
    except json.JSONDecodeError:
        return raw


# Known storage roots; a path outside them is a policy violation, not a 404.
STORE_ROOTS = ("public", "private", "agents")
