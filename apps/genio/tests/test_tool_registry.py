"""Tool registry + hitech-store client: catalogue, validation, format dispatch."""

from __future__ import annotations

import io
import json
import urllib.error
from types import SimpleNamespace

import pytest

from genio_server.core.broker import Risk, ToolSpec
from genio_server.core.store_client import (
    StoreAuthError,
    StoreClient,
    StoreError,
    StoreNotFound,
)
from genio_server.core.tool_registry import (
    ToolArgError,
    ToolRegistry,
    ToolRegistryError,
    build_default_registry,
    check_new_name,
    check_store_path,
    sanitize_html,
    store_tools,
)


def spec(tool_id="t.read", risk=Risk.R0, cap="test:read", tags=(), handler=None):
    return ToolSpec(tool_id, risk, "desc", cap, {}, handler or (lambda a: {}), 60,
                    lambda a: "", None, tags)


# --------------------------------------------------------------------------- registry
def test_register_and_lookup():
    registry = ToolRegistry([spec("b.read"), spec("a.read")])
    assert len(registry) == 2
    assert "a.read" in registry and registry.get("missing") is None
    assert registry.require("a.read").id == "a.read"
    assert [t.id for t in registry.all()] == ["a.read", "b.read"]

    with pytest.raises(ToolRegistryError):
        registry.register(spec("a.read"))
    with pytest.raises(ToolRegistryError):
        registry.register(spec("Bad Id"))
    with pytest.raises(KeyError):
        registry.require("nope")


def test_replacement_and_removal_are_explicit():
    registry = ToolRegistry([spec("a.read")])
    registry.replace(spec("a.read", risk=Risk.R1, cap="test:write"))
    assert registry.require("a.read").risk is Risk.R1
    registry.unregister("a.read")
    assert len(registry) == 0


def test_tags_and_capability_queries():
    registry = ToolRegistry([
        spec("store.list", cap="store:read", tags=("store", "directories")),
        spec("store.extract", cap="store:extract", tags=("store", "documents")),
        spec("system.info", cap="telemetry:read", tags=("system",)),
    ])
    assert {t.id for t in registry.by_tag("store")} == {"store.list", "store.extract"}
    assert [t.id for t in registry.by_capability("store:extract")] == ["store.extract"]


def test_catalogue_shape_and_summary():
    registry = ToolRegistry([
        spec("a.read", Risk.R0, "store:read", ("store",)),
        spec("a.write", Risk.R1, "store:write"),
        spec("a.r2", Risk.R2, "store:write"),
    ])
    catalogue = registry.catalogue()
    assert catalogue[0]["id"] == "a.read"
    assert catalogue[0]["tags"] == ["store"]
    assert catalogue[-1]["requires_approval"] is True
    summary = registry.summary()
    assert summary["count"] == 3
    assert summary["by_risk"] == {"R0": 1, "R1": 1, "R2": 1}
    assert summary["capabilities"] == ["store:read", "store:write"]


def test_default_registry_contains_the_universal_portal_tools(tmp_path):
    registry = build_default_registry(tmp_path)
    ids = registry.ids()
    assert {"store.list", "store.extract", "store.media_info", "store.mkdir",
            "telemetry.snapshot", "services.list", "service.status",
            "file.write", "service.restart"} <= ids
    assert registry.require("store.extract").cap == "store:extract"
    assert registry.require("store.extract").tags == ("store", "documents")
    assert registry.require("store.media_info").cap == "media:read"
    assert registry.require("store.mkdir").risk is Risk.R2
    assert registry.require("store.mkdir").cap == "store:write"


# --------------------------------------------------------------------------- validators
@pytest.mark.parametrize("path", ["public", "public/x.pdf", "agents/genio/out.csv",
                                  "private/secret.docx"])
def test_store_paths_are_accepted(path):
    assert check_store_path(path) == path


@pytest.mark.parametrize("path,expected", [
    ("/public", "public"), ("/public/x.pdf", "public/x.pdf"),
    (" /agents/genio/x.csv ", "agents/genio/x.csv"),
])
def test_leading_slash_is_normalised(path, expected):
    """Users (and local models) often write '/public' — accept, then normalise."""
    assert check_store_path(path) == expected


@pytest.mark.parametrize("path", [
    "/etc/passwd", "public/../../etc/passwd", "public//x", "public/./x",
    "HCF/x", "public/x\x00.pdf", "public/" + "a" * 400,
])
def test_store_paths_are_rejected(path):
    with pytest.raises(ToolArgError):
        check_store_path(path)


def test_list_allows_the_root_but_file_tools_do_not():
    assert check_store_path("", allow_empty=True) == ""
    with pytest.raises(ToolArgError):
        check_store_path("")


@pytest.mark.parametrize("name", ["reports", "Q4 2026", "a.b_c-1"])
def test_safe_folder_names(name):
    assert check_new_name(name) == name


@pytest.mark.parametrize("name", ["", "..", "a/b", "x" * 100, "a;b", "evil\nname"])
def test_unsafe_folder_names(name):
    with pytest.raises(ToolArgError):
        check_new_name(name)


# --------------------------------------------------------------------------- fake store
class FakeStore:
    def __init__(self, *, listing=None, extract=None, text=None, media=None,
                 mkdir_result=None, mkdir_error=None):
        self.config = SimpleNamespace(base_url="http://store.test")
        self.calls: list[tuple] = []
        self._listing = listing or {"path": "public", "dirs": [], "files": []}
        self._extract = extract or {"kind": "pdf", "name": "x.pdf",
                                    "html": "<p>hello</p>", "truncated": False}
        self._text = text if text is not None else ("plain text", "text/plain")
        self._media = media or {"name": "v.mp4", "path": "public/v.mp4", "kind": "video",
                                "size": 10, "size_str": "10 o", "mtime": "now",
                                "sha256": "abc123def456", "preview_url": "http://p",
                                "download_url": "http://d"}
        self._mkdir_result = mkdir_result or {"ok": True, "path": "public/reports"}
        self._mkdir_error = mkdir_error

    def list_dir(self, path):
        self.calls.append(("list", path))
        return self._listing

    def extract_document(self, path):
        self.calls.append(("extract", path))
        return self._extract

    def download_text(self, path, limit=None):
        self.calls.append(("download", path, limit))
        return self._text

    def media_info(self, path):
        self.calls.append(("media", path))
        if isinstance(self._media, Exception):
            raise self._media
        return self._media

    def mkdir(self, parent, name):
        self.calls.append(("mkdir", parent, name))
        if self._mkdir_error:
            raise self._mkdir_error
        return self._mkdir_result


def tool(client, tool_id):
    registry = ToolRegistry(store_tools(client))
    return registry.require(tool_id)


# --------------------------------------------------------------------------- handlers
def test_store_list_returns_slim_entries():
    listing = {"path": "public", "dirs": [{"name": "HCF", "path": "public/HCF"}],
               "files": [{"name": "a.pdf", "path": "public/a.pdf", "size": 12,
                          "size_str": "12 o", "mtime": "t", "kind": "pdf",
                          "sha256": "abc", "extra": "drop-me"}]}
    client = FakeStore(listing=listing)
    result = tool(client, "store.list").handler({"path": "public"})
    assert result["counts"] == {"dirs": 1, "files": 1}
    assert result["files"][0] == {"name": "a.pdf", "size": 12, "size_str": "12 o",
                                  "kind": "pdf", "sha256": "abc"}


def test_extract_pdf_uses_office_endpoint_and_strips_html():
    client = FakeStore(extract={"kind": "pdf", "name": "x.pdf", "truncated": True,
                                "html": "<h3>Title</h3><script>alert(1)</script><p>Body</p>"})
    result = tool(client, "store.extract").handler({"path": "public/x.pdf"})
    assert client.calls[0] == ("extract", "public/x.pdf")
    assert result["format"] == "pdf"
    assert "Title" in result["text"] and "Body" in result["text"]
    assert "alert" not in result["text"]
    assert result["truncated"] is True


def test_extract_html_downloads_and_sanitises():
    client = FakeStore(text=("<h1>Doc</h1><style>x{}</style><p>Hi</p>", "text/html"))
    result = tool(client, "store.extract").handler({"path": "public/page.html"})
    assert result["format"] == "html"
    assert "Doc" in result["text"] and "Hi" in result["text"]
    assert "x{}" not in result["text"]
    assert client.calls[0][0] == "download"


def test_extract_csv_returns_structure():
    client = FakeStore(text=("name,size\nalpha,1\nbeta,2\ngamma,3\n", "text/csv"))
    result = tool(client, "store.extract").handler({"path": "public/data.csv"})
    assert result["format"] == "csv"
    assert result["columns"] == ["name", "size"]
    assert result["row_count"] == 3
    assert result["sample"][0] == ["alpha", "1"]
    assert "name | size" in result["text"]


def test_extract_text_and_markdown():
    client = FakeStore(text=("# Title\n\nBody", "text/markdown"))
    result = tool(client, "store.extract").handler({"path": "public/notes.md"})
    assert result["format"] == "md"
    assert "Body" in result["text"]


def test_media_info_returns_preview_and_hides_raw_bytes():
    result = tool(FakeStore(), "store.media_info").handler({"path": "public/v.mp4"})
    assert result["kind"] == "video"
    assert result["preview_url"] == "http://p"
    assert result["sha256"] == "abc123def456"
    assert "metadata only" in result["note"]


def test_media_info_rejects_non_media():
    client = FakeStore(media=StoreError("not a media file (kind=pdf)"))
    with pytest.raises(StoreError):
        tool(client, "store.media_info").handler({"path": "public/x.pdf"})


def test_mkdir_success_reports_target():
    result = tool(FakeStore(), "store.mkdir").handler({"parent": "public", "name": "reports"})
    assert result["ok"] is True and result["path"] == "public/reports"


def test_mkdir_without_admin_credentials_fails_loudly():
    client = FakeStore(mkdir_error=StoreAuthError("Session admin requise.", 403))
    with pytest.raises(RuntimeError, match="admin credentials"):
        tool(client, "store.mkdir").handler({"parent": "public", "name": "reports"})


def test_sanitize_html_is_shared_and_safe():
    assert sanitize_html("<script>x</script><b>ok</b>") == "ok"


# --------------------------------------------------------------------------- store client
class FakeResponse:
    def __init__(self, body: bytes = b"{}", status: int = 200,
                 headers: dict[str, str] | None = None):
        self._body = body
        self.status = status
        self.headers = headers or {}

    def read(self, *args):
        return self._body

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        return False


def capture_urlopen(monkeypatch, response=None, error=None):
    captured = {}

    def fake_urlopen(req, timeout=None):
        captured["url"] = req.full_url
        captured["headers"] = dict(req.headers)
        captured["timeout"] = timeout
        if error:
            raise error
        return response or FakeResponse()

    monkeypatch.setattr("genio_server.core.store_client.urllib.request.urlopen", fake_urlopen)
    return captured


def test_client_sends_admin_cookie_and_encoded_path(monkeypatch):
    captured = capture_urlopen(monkeypatch, FakeResponse(json.dumps(
        {"path": "public/HCF", "dirs": [], "files": []}).encode()))
    client = StoreClient(base_url="http://store.test", admin_token="jwt-token")
    data = client.list_dir("public/HCF")

    assert data["path"] == "public/HCF"
    assert "path=public%2FHCF" in captured["url"]
    assert captured["headers"].get("Cookie") == "hitech_admin_token=jwt-token"


def test_client_sends_agent_token(monkeypatch):
    captured = capture_urlopen(monkeypatch, FakeResponse(b"{}"))
    StoreClient(base_url="http://store.test", agent_token="shared").list_dir("")
    assert captured["headers"].get("X-agent-token") == "shared"


def test_client_sets_range_for_text_download(monkeypatch):
    captured = capture_urlopen(monkeypatch, FakeResponse(
        "hello".encode(), headers={"Content-Type": "text/plain; charset=utf-8"}))
    text, ctype = StoreClient(base_url="http://store.test").download_text("public/a.txt", limit=5)

    assert text == "hello"
    assert captured["headers"].get("Range") == "bytes=0-4"
    assert "charset=utf-8" in ctype


def test_client_maps_http_errors(monkeypatch):
    def http_error(code):
        return urllib.error.HTTPError(
            "http://store.test/x", code, "err", None,
            io.BytesIO(json.dumps({"detail": f"failure-{code}"}).encode()))

    capture_urlopen(monkeypatch, error=http_error(403))
    with pytest.raises(StoreAuthError):
        StoreClient(base_url="http://store.test").list_dir("private")

    capture_urlopen(monkeypatch, error=http_error(404))
    with pytest.raises(StoreNotFound):
        StoreClient(base_url="http://store.test").list_dir("public/missing")

    capture_urlopen(monkeypatch, error=http_error(500))
    with pytest.raises(StoreError):
        StoreClient(base_url="http://store.test").list_dir("public")


def test_client_media_info_finds_entry(monkeypatch):
    payload = {"path": "public", "dirs": [], "files": [
        {"name": "clip.mp4", "path": "public/clip.mp4", "kind": "video",
         "size": 99, "sha256": "f" * 64}]}
    capture_urlopen(monkeypatch, FakeResponse(json.dumps(payload).encode()))
    info = StoreClient(base_url="http://store.test").media_info("public/clip.mp4")

    assert info["kind"] == "video"
    assert info["preview_url"].startswith("http://store.test/api/drive/download?path=")


def test_client_describe_reports_auth_mode():
    assert StoreClient(base_url="http://x", admin_token="").describe()["auth_mode"] == "anonymous"
    assert StoreClient(base_url="http://x", admin_token="t").describe()["auth_mode"] == "admin"
    assert StoreClient(base_url="http://x", agent_token="t").describe()["auth_mode"] == "agent"


def test_client_health(monkeypatch):
    capture_urlopen(monkeypatch, FakeResponse(json.dumps({"ok": True, "service": "hitech-store"}).encode()))
    health = StoreClient(base_url="http://store.test").health()
    assert health["ok"] is True and health["base_url"] == "http://store.test"


def test_broker_maps_store_arg_errors_to_invalid_args(tmp_path):
    """Regression: registry and broker must share one ToolArgError type."""
    from genio_server.core.broker import Broker
    from genio_server.core.caps import issue

    broker = Broker(tmp_path, tools=store_tools(FakeStore()))
    token = issue("agent", ["store:extract"], 600)
    result = broker.invoke("store.extract", {"path": "public/../etc/passwd"}, token)

    assert result["ok"] is False
    assert result["code"] == "invalid_args"
    assert result["decision"] == "denied"
