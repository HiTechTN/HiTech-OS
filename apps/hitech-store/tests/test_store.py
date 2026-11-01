"""Behavioural tests for the HiTech-Store public/agent/admin surfaces."""
from __future__ import annotations

import json

import pytest
from fastapi.testclient import TestClient

import app as store

AGENT_HEADERS = {"X-Agent-Token": "test-agent-secret"}


@pytest.fixture()
def isolated_store(tmp_path, monkeypatch):
    monkeypatch.setattr(store, "OTP_STATE_FILE", tmp_path / ".otp_state.json")
    monkeypatch.setattr(store, "OTP_LOCK_FILE", tmp_path / ".otp_state.lock")

    def _no_smtp(*_args, **_kwargs):
        raise RuntimeError("SMTP disabled in tests")

    monkeypatch.setattr(store, "send_pin_email", _no_smtp)
    store.clear_otp()
    return store


@pytest.fixture()
def client(isolated_store):
    return TestClient(store.app)


@pytest.fixture()
def admin_client(isolated_store):
    client = TestClient(store.app)
    assert client.post("/api/auth/quick-otp").status_code == 200
    pin = json.loads(store.OTP_STATE_FILE.read_text())["pin"]
    response = client.post("/api/auth/verify-otp", json={"pin": pin})
    assert response.status_code == 200, response.text
    return client


def _write(rel: str, payload: bytes = b"payload") -> None:
    target = store.STORAGE_DIR / rel
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(payload)


def test_health_reports_service(client):
    body = client.get("/store/health").json()
    assert body["status"] == "ok"
    assert body["service"] == "hitech-drive"


def test_store_ui_renders(client):
    response = client.get("/store")
    assert response.status_code == 200
    assert "text/html" in response.headers["content-type"]


def test_anonymous_session_is_not_admin(client):
    body = client.get("/api/auth/me").json()
    assert body["admin"] is False
    assert body["agent"] is False


def test_otp_flow_promotes_anonymous_to_admin(admin_client):
    body = admin_client.get("/api/auth/me").json()
    assert body["admin"] is True


def test_wrong_then_right_pin(client):
    assert client.post("/api/auth/quick-otp").status_code == 200
    pin = json.loads(store.OTP_STATE_FILE.read_text())["pin"]
    wrong = "000000" if pin != "000000" else "111111"

    assert client.post("/api/auth/verify-otp", json={"pin": wrong}).status_code == 401
    assert client.post("/api/auth/verify-otp", json={"pin": pin}).status_code == 200
    # The PIN is single-use.
    assert client.post("/api/auth/verify-otp", json={"pin": pin}).status_code == 400


def test_pin_is_cleared_after_too_many_attempts(client):
    assert client.post("/api/auth/quick-otp").status_code == 200
    pin = json.loads(store.OTP_STATE_FILE.read_text())["pin"]
    wrong = "000000" if pin != "000000" else "111111"
    for _ in range(store.OTP_MAX_ATTEMPTS):
        client.post("/api/auth/verify-otp", json={"pin": wrong})
    # The attempt budget is exhausted: even the real PIN is refused and the
    # state file is dropped, so the next request starts from scratch.
    assert client.post("/api/auth/verify-otp", json={"pin": pin}).status_code == 401
    assert not store.OTP_STATE_FILE.exists()


def test_private_folder_requires_admin(client, admin_client):
    _write("private/secret.txt", b"top secret")

    assert client.get("/api/drive/list", params={"path": "private"}).status_code == 403
    assert client.get("/api/drive/download", params={"path": "private/secret.txt"}).status_code == 403

    listed = admin_client.get("/api/drive/list", params={"path": "private"})
    assert listed.status_code == 200
    assert [f["name"] for f in listed.json()["files"]] == ["secret.txt"]


def test_agents_folder_requires_agent_token_or_admin(client):
    _write("agents/shared/report.txt")

    assert client.get("/api/v1/artifacts/list", params={"folder": "shared"}).status_code == 401
    assert (
        client.get(
            "/api/v1/artifacts/list",
            params={"folder": "shared"},
            headers={"X-Agent-Token": "wrong"},
        ).status_code
        == 401
    )

    ok = client.get("/api/v1/artifacts/list", params={"folder": "shared"}, headers=AGENT_HEADERS)
    assert ok.status_code == 200
    assert [f["name"] for f in ok.json()["files"]] == ["report.txt"]


def test_drive_listing_rejects_path_traversal(client):
    response = client.get("/api/drive/list", params={"path": "../../etc"})
    assert response.status_code == 403


def test_upload_requires_admin_then_persists_checksum(client, admin_client):
    payload = b"hello hitech"
    assert (
        client.post(
            "/api/drive/upload",
            data={"folder": "public"},
            files={"file": ("hello.txt", payload, "text/plain")},
        ).status_code
        == 403
    )

    uploaded = admin_client.post(
        "/api/drive/upload",
        data={"folder": "public"},
        files={"file": ("hello.txt", payload, "text/plain")},
    )
    assert uploaded.status_code == 200, uploaded.text
    body = uploaded.json()
    assert body["path"] == "public/hello.txt"
    assert (store.STORAGE_DIR / "public/hello.txt").read_bytes() == payload
    assert (store.STORAGE_DIR / "public/hello.txt.sha256").exists()

    listing = admin_client.get("/api/drive/list", params={"path": "public"}).json()
    assert [f["name"] for f in listing["files"]] == ["hello.txt"]

    download = admin_client.get("/api/drive/download", params={"path": "public/hello.txt"})
    assert download.status_code == 200
    assert download.content == payload


def test_agent_artifact_upload_and_list(client):
    response = client.post(
        "/api/v1/artifacts/upload",
        data={"folder": "genio"},
        files={"file": ("artifact.bin", b"\x00\x01", "application/octet-stream")},
        headers=AGENT_HEADERS,
    )
    assert response.status_code == 200, response.text
    assert response.json()["path"] == "agents/genio/artifact.bin"

    listing = client.get(
        "/api/v1/artifacts/list", params={"folder": "genio"}, headers=AGENT_HEADERS
    ).json()
    assert [f["name"] for f in listing["files"]] == ["artifact.bin"]


def test_agent_upload_rejects_unknown_folder(client):
    response = client.post(
        "/api/v1/artifacts/upload",
        data={"folder": "../etc"},
        files={"file": ("x", b"x", "text/plain")},
        headers=AGENT_HEADERS,
    )
    assert response.status_code == 400


def test_share_link_roundtrip_and_private_refusal(client):
    _write("public/shared.txt", b"shared bytes")
    _write("private/no-share.txt", b"nope")

    forbidden = client.post(
        "/api/v1/artifacts/create-share-link",
        json={"path": "private/no-share.txt", "ttl_min": 5},
        headers=AGENT_HEADERS,
    )
    assert forbidden.status_code == 403

    created = client.post(
        "/api/v1/artifacts/create-share-link",
        json={"path": "public/shared.txt", "ttl_min": 5},
        headers=AGENT_HEADERS,
    )
    assert created.status_code == 200, created.text
    url = created.json()["url"]
    path = url.split("//", 1)[1].split("/", 1)[1]

    downloaded = client.get("/" + path)
    assert downloaded.status_code == 200
    assert downloaded.content == b"shared bytes"


def test_safe_name_strips_directories():
    assert store.safe_name("../../etc/passwd") == "passwd"
    assert store.safe_name("report.pdf") == "report.pdf"
    with pytest.raises(store.HTTPException):
        store.safe_name("..")
