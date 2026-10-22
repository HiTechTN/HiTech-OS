"""Central tool registry — every capability Genio can exercise, in one place.

The registry is the single source of truth for the *catalogue* (what the model
may propose, what the policy engine evaluates, what the broker can execute).
Registration is explicit and duplicate-safe; there is no dynamic discovery and
no plugin loading, so the attack surface is exactly what is listed here.

Tool families
-------------
system      telemetry, services, sandboxed file write, unit restart
documents   store.list / store.extract (PDF, DOCX, XLSX, PPTX, HTML, CSV, text)
media       store.media_info (audio / video / image metadata + preview URL)
directories store.mkdir (R2 — human confirmation)

Capabilities (assigned per tool, never per model):
    telemetry:read  service:read  service:restart  fs:write
    store:read  store:extract  store:write  store:private  media:read
"""

from __future__ import annotations

import csv
import hashlib
import html as html_mod
import io
import re
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Callable, Iterable, Iterator

from .broker import Risk, ToolArgError, ToolSpec
from .store_client import STORE_ROOTS, StoreAuthError, StoreClient, StoreError, StoreNotFound

MAX_TEXT_CHARS = 8000
MAX_TEXT_BYTES = 2 * 1024 * 1024
SAFE_NAME = re.compile(r"[A-Za-z0-9._ -]{1,64}")

OFFICE_EXTS = {".pdf", ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx"}
HTML_EXTS = {".html", ".htm"}
CSV_EXTS = {".csv", ".tsv"}
TEXT_EXTS = {".txt", ".md", ".markdown", ".json", ".xml", ".log", ".yaml", ".yml"}
SUPPORTED_EXTS = OFFICE_EXTS | HTML_EXTS | CSV_EXTS | TEXT_EXTS

FORMAT_LABEL = {
    ".pdf": "pdf", ".doc": "word", ".docx": "word",
    ".xls": "excel", ".xlsx": "excel", ".ppt": "powerpoint", ".pptx": "powerpoint",
    ".html": "html", ".htm": "html", ".csv": "csv", ".tsv": "tsv",
}


class ToolRegistryError(ValueError):
    """Registration problem (duplicate id, invalid spec)."""


# --------------------------------------------------------------------------- registry
class ToolRegistry:
    """Ordered, duplicate-safe collection of :class:`ToolSpec`."""

    def __init__(self, tools: Iterable[ToolSpec] = ()) -> None:
        self._tools: dict[str, ToolSpec] = {}
        for tool in tools:
            self.register(tool)

    # -- registration ------------------------------------------------------
    def register(self, tool: ToolSpec) -> ToolSpec:
        if not isinstance(tool, ToolSpec):
            raise ToolRegistryError("register() expects a ToolSpec")
        if not tool.id or not re.match(r"^[a-z][a-z0-9_.-]*$", tool.id):
            raise ToolRegistryError(f"invalid tool id {tool.id!r}")
        if tool.id in self._tools:
            raise ToolRegistryError(f"duplicate tool id {tool.id!r}")
        self._tools[tool.id] = tool
        return tool

    def replace(self, tool: ToolSpec) -> None:
        """Explicit replacement (used by tests and future hot-reload)."""
        self._tools[tool.id] = tool

    def unregister(self, tool_id: str) -> None:
        self._tools.pop(tool_id, None)

    # -- access ------------------------------------------------------------
    def get(self, tool_id: str) -> ToolSpec | None:
        return self._tools.get(tool_id)

    def require(self, tool_id: str) -> ToolSpec:
        tool = self.get(tool_id)
        if tool is None:
            raise KeyError(f"unknown tool {tool_id!r}")
        return tool

    def all(self) -> list[ToolSpec]:
        return sorted(self._tools.values(), key=lambda t: (t.risk.value, t.id))

    def ids(self) -> set[str]:
        return set(self._tools)

    def by_tag(self, tag: str) -> list[ToolSpec]:
        return [t for t in self.all() if tag in t.tags]

    def by_capability(self, capability: str) -> list[ToolSpec]:
        return [t for t in self.all() if t.cap == capability]

    def catalogue(self) -> list[dict[str, Any]]:
        return [{
            "id": t.id, "risk": t.risk.value, "description": t.description,
            "cap": t.cap, "rate_per_min": t.rate_per_min, "params": t.params,
            "tags": list(t.tags), "requires_approval": t.risk is Risk.R2,
        } for t in self.all()]

    def summary(self) -> dict[str, Any]:
        tools = self.all()
        return {
            "count": len(tools),
            "by_risk": {risk.value: sum(1 for t in tools if t.risk is risk)
                        for risk in Risk},
            "capabilities": sorted({t.cap for t in tools if t.cap}),
        }

    def __len__(self) -> int:
        return len(self._tools)

    def __contains__(self, tool_id: object) -> bool:
        return tool_id in self._tools

    def __iter__(self) -> Iterator[ToolSpec]:
        return iter(self.all())


# --------------------------------------------------------------------------- helpers
def sanitize_html(raw: str) -> str:
    """Untrusted extraction HTML → plain text (prompt-injection surface reduction)."""
    text = re.sub(r"(?i)<(script|style)\b.*?</\1>", " ", raw or "", flags=re.S)
    text = re.sub(r"<br\s*/?>|</p>|</h4>|</tr>|</div>", "\n", text, flags=re.I)
    text = re.sub(r"</td>", " | ", text, flags=re.I)
    text = re.sub(r"<[^>]+>", "", text)
    text = html_mod.unescape(text)
    text = re.sub(r"[ \t]+", " ", text)
    text = re.sub(r"\n{3,}", "\n\n", text)
    return text.strip()


def check_store_path(path: str, *, allow_empty: bool = False) -> str:
    """Normalise and validate a drive-relative path."""
    value = (path or "").strip().lstrip("/")
    if not value:
        if allow_empty:
            return ""
        raise ToolArgError("path is required")
    if len(value) > 400 or "\x00" in value:
        raise ToolArgError("invalid path")
    parts = value.split("/")
    if any(part in ("", ".", "..") for part in parts):
        raise ToolArgError("path must not contain empty, '.' or '..' segments")
    if parts[0] not in STORE_ROOTS:
        raise ToolArgError(f"path must start with one of {STORE_ROOTS}")
    return "/".join(parts)


def check_new_name(name: str) -> str:
    value = (name or "").strip()
    if not SAFE_NAME.fullmatch(value) or value in (".", ".."):
        raise ToolArgError("invalid folder name")
    return value


def _csv_summary(path: str, raw: str) -> dict[str, Any]:
    try:
        dialect = csv.Sniffer().sniff(raw[:65536], delimiters=",;\t|")
    except csv.Error:
        dialect = csv.excel
    rows = list(csv.reader(io.StringIO(raw), dialect))
    if not rows:
        return {"format": "csv", "path": path, "columns": [], "row_count": 0,
                "sample": [], "chars": 0, "truncated": False, "text": ""}
    header, body = rows[0], rows[1:]
    sample = body[:10]
    text = "\n".join(" | ".join(cell for cell in row) for row in [header, *sample])[:MAX_TEXT_CHARS]
    return {"format": "csv", "path": path, "columns": header,
            "row_count": len(body), "sample": sample, "chars": len(text),
            "truncated": len(body) > len(sample), "text": text}


# --------------------------------------------------------------------------- store tools
def store_tools(client: StoreClient) -> list[ToolSpec]:
    # -- validators (run before the risk/policy gate) ----------------------
    def v_list(args: dict[str, Any]) -> dict[str, Any]:
        args["path"] = check_store_path(args["path"], allow_empty=True)
        return args

    def v_file(args: dict[str, Any]) -> dict[str, Any]:
        args["path"] = check_store_path(args["path"])
        ext = Path(args["path"]).suffix.lower()
        if ext not in SUPPORTED_EXTS:
            raise ToolArgError(
                f"unsupported format {ext or '(none)'} — supported: "
                "pdf, docx, xlsx, pptx, html, csv, txt, md, json")
        return args

    def v_media(args: dict[str, Any]) -> dict[str, Any]:
        args["path"] = check_store_path(args["path"])
        return args

    def v_mkdir(args: dict[str, Any]) -> dict[str, Any]:
        args["parent"] = check_store_path(args["parent"], allow_empty=True)
        args["name"] = check_new_name(args["name"])
        return args

    # -- handlers ----------------------------------------------------------
    def t_list(args: dict[str, Any]) -> dict[str, Any]:
        data = client.list_dir(args["path"])
        files = [{k: f.get(k) for k in ("name", "size", "size_str", "kind", "sha256")}
                 for f in data["files"]]
        return {"path": data["path"], "dirs": data["dirs"], "files": files,
                "counts": {"dirs": len(data["dirs"]), "files": len(files)}}

    def t_extract(args: dict[str, Any]) -> dict[str, Any]:
        path = args["path"]
        ext = Path(path).suffix.lower()
        if ext in OFFICE_EXTS:
            data = client.extract_document(path)
            text = sanitize_html(data.get("html", ""))
            return {"format": data.get("kind") or FORMAT_LABEL.get(ext, ext.lstrip(".")),
                    "path": path, "chars": len(text),
                    "truncated": bool(data.get("truncated")) or len(text) > MAX_TEXT_CHARS,
                    "text": text[:MAX_TEXT_CHARS]}
        raw, content_type = client.download_text(path, limit=MAX_TEXT_BYTES)
        if ext in HTML_EXTS:
            text = sanitize_html(raw)
            fmt = "html"
        elif ext in CSV_EXTS:
            summary = _csv_summary(path, raw)
            summary["content_type"] = content_type
            return summary
        else:
            text = raw
            fmt = FORMAT_LABEL.get(ext, ext.lstrip("."))
        return {"format": fmt, "path": path, "content_type": content_type,
                "chars": len(text), "truncated": len(text) > MAX_TEXT_CHARS,
                "text": text[:MAX_TEXT_CHARS]}

    def t_media_info(args: dict[str, Any]) -> dict[str, Any]:
        info = client.media_info(args["path"])
        return {
            "name": info.get("name"), "path": info.get("path"),
            "kind": info.get("kind"), "size": info.get("size"),
            "size_str": info.get("size_str"), "mtime": info.get("mtime"),
            "sha256": info.get("sha256"), "preview_url": info.get("preview_url"),
            "download_url": info.get("download_url"),
            "note": "metadata only — media bytes are not fetched by Genio",
        }

    def t_mkdir(args: dict[str, Any]) -> dict[str, Any]:
        parent = args.get("parent") or ""
        name = args["name"]
        try:
            result = client.mkdir(parent, name)
        except StoreAuthError as exc:
            raise RuntimeError(
                "hitech-store refused the write: admin credentials are not configured "
                f"for Genio ({exc})") from exc
        except StoreError as exc:
            raise RuntimeError(f"store mkdir failed: {exc}") from exc
        target = result.get("path") or "/".join(filter(None, (parent, name)))
        return {"ok": True, "path": target,
                "url": f"{client.config.base_url}/api/drive/list?path={target}"}

    return [
        ToolSpec("store.list", Risk.R0,
                 "List a hitech-store directory (folders, files, sizes, checksums).",
                 "store:read", {"path": {"type": "str", "required": True}},
                 t_list, 120, lambda a: a.get("path") or "/", v_list,
                 tags=("store", "directories")),
        ToolSpec("store.extract", Risk.R0,
                 "Extract text from a document: PDF, DOCX, XLSX, PPTX, HTML, CSV, text.",
                 "store:extract", {"path": {"type": "str", "required": True}},
                 t_extract, 60, lambda a: a.get("path", ""), v_file,
                 tags=("store", "documents")),
        ToolSpec("store.media_info", Risk.R0,
                 "Inspect an audio/video/image file: kind, size, checksum, preview URL.",
                 "media:read", {"path": {"type": "str", "required": True}},
                 t_media_info, 120, lambda a: a.get("path", ""), v_media,
                 tags=("store", "media")),
        ToolSpec("store.mkdir", Risk.R2,
                 "Create a folder in the public or agents area of the hitech-store drive.",
                 "store:write", {"parent": {"type": "str", "required": True},
                                 "name": {"type": "str", "required": True}},
                 t_mkdir, 6, lambda a: f"{a.get('parent') or '/'}/{a.get('name')}", v_mkdir,
                 tags=("store", "directories", "write")),
    ]


# --------------------------------------------------------------------------- build
def build_default_registry(state_dir: Path | str, store: StoreClient | None = None,
                           include_system: bool = True) -> ToolRegistry:
    """System tools + hitech-store tools, fully registered and validated."""
    from .broker import system_tools  # lazy: broker.default_tools() calls back here

    registry = ToolRegistry()
    if include_system:
        for tool in system_tools(Path(state_dir)):
            registry.register(tool)
    client = store or StoreClient.from_env()
    for tool in store_tools(client):
        registry.register(tool)
    return registry
