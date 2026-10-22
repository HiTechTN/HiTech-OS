"""HiTech Drive — Central Sovereign Storage, Drive Explorer & AI Agent Artifact Hub.

Public explorer : GET /store
Admin console   : GET /store/admin (OTP-gated, azmi.hitech@gmail.com only)
Drive JSON API  : GET /api/drive/list, GET/HEAD /api/drive/download, POST /api/drive/upload
Auth JSON API   : POST /api/auth/quick-otp, POST /api/auth/verify-otp (alias: /verify), GET /api/auth/me
Agent API       : POST /api/v1/artifacts/upload, GET /api/v1/artifacts/list,
                  POST /api/v1/artifacts/create-share-link, GET /share/{path}
"""
import fcntl
import hashlib
import hmac
import json
import logging
import mimetypes
import os
import re
import secrets
import shutil
import smtplib
import subprocess
import tempfile
from contextlib import contextmanager
from datetime import datetime, timedelta, timezone
from email.message import EmailMessage
from pathlib import Path
from urllib.parse import quote, unquote

from dotenv import load_dotenv
from fastapi import FastAPI, Request, Form, File, UploadFile, Header, HTTPException, status
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import HTMLResponse, JSONResponse, RedirectResponse, StreamingResponse
from fastapi.staticfiles import StaticFiles
from fastapi.templating import Jinja2Templates
from jose import jwt, JWTError
from pydantic import BaseModel
from uvicorn.middleware.proxy_headers import ProxyHeadersMiddleware

load_dotenv(Path(__file__).resolve().parent / ".env")

ADMIN_EMAIL = "azmi.hitech@gmail.com"
SECRET_KEY = os.getenv("SECRET_KEY", secrets.token_hex(32))
AGENT_SHARED_SECRET = os.getenv("AGENT_SHARED_SECRET", "")
BASE_DIR = Path(__file__).resolve().parent
STORAGE_DIR = Path(os.getenv("STORAGE_DIR", str(BASE_DIR / "storage")))
STORAGE_DIR.mkdir(parents=True, exist_ok=True)
LOG_DIR = BASE_DIR / "logs"
LOG_DIR.mkdir(parents=True, exist_ok=True)

SMTP_HOST = os.getenv("SMTP_HOST", "smtp.gmail.com")
SMTP_PORT = int(os.getenv("SMTP_PORT", 587))
SMTP_USER = os.getenv("SMTP_USER", ADMIN_EMAIL)
SMTP_PASSWORD = os.getenv("SMTP_PASSWORD", "")

# DEV fallback: if SMTP is unreachable, keep the OTP valid and write the PIN to
# the local service log (journal/console) instead of returning a 500. This makes
# the lab testable without a mail relay. MUST be 0 in production:
#   DEV_OTP_FALLBACK=0  →  hard 500 on SMTP failure, PIN cleared (no leak).
DEV_OTP_FALLBACK = os.getenv("DEV_OTP_FALLBACK", "1").strip().lower() not in ("0", "false", "no", "off")

OTP_TTL_MIN = 10
SESSION_HOURS = 12
MAX_UPLOAD_BYTES = 15 * 1024**3  # 15 Go
CHUNK = 1024 * 1024 * 4

ALLOWED_AGENT_FOLDERS = {"genio", "opencode", "claude", "shared"}
UPLOAD_FOLDERS = {"public", "private"} | {f"agents/{a}" for a in ALLOWED_AGENT_FOLDERS}

PREVIEW_AUDIO = {".wav", ".mp3", ".ogg", ".m4a", ".flac", ".aac", ".opus"}
PREVIEW_VIDEO = {".mp4", ".webm", ".mkv", ".mov", ".avi"}
PREVIEW_IMAGE = {".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg", ".bmp", ".ico", ".tif", ".tiff"}
PREVIEW_MODEL = {".glb", ".gltf"}
PREVIEW_MARKDOWN = {".md", ".markdown"}
PREVIEW_PDF = {".pdf"}
PREVIEW_HTML = {".html", ".htm"}
PREVIEW_WORD = {".doc", ".docx", ".odt", ".rtf"}
PREVIEW_EXCEL = {".xls", ".xlsx", ".ods", ".csv", ".tsv"}
PREVIEW_POWERPOINT = {".ppt", ".pptx", ".odp"}
PREVIEW_ARCHIVE = {".zip", ".tar", ".gz", ".tgz", ".7z", ".rar"}
PREVIEW_CODE = {".py", ".sh", ".json", ".log", ".txt", ".yaml", ".yml", ".toml",
                ".js", ".ts", ".css", ".xml", ".conf", ".env.example", ".c", ".h",
                ".cpp", ".java", ".go", ".rs", ".php", ".sql", ".ini", ".cfg"}

# Explicit MIME fallback (system mime.types may miss Office types)
EXTRA_MIME = {
    ".docx": "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    ".xlsx": "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    ".pptx": "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    ".doc": "application/msword",
    ".xls": "application/vnd.ms-excel",
    ".ppt": "application/vnd.ms-powerpoint",
    ".odt": "application/vnd.oasis.opendocument.text",
    ".ods": "application/vnd.oasis.opendocument.spreadsheet",
    ".odp": "application/vnd.oasis.opendocument.presentation",
    ".md": "text/markdown",
    ".mkv": "video/x-matroska",
}

# Server-side text-extraction caps (protect workers from giant docs)
EXTRACT_MAX_CHARS = 200_000
EXTRACT_MAX_PDF_PAGES = 50
EXTRACT_MAX_SHEET_ROWS = 200
EXTRACT_MAX_SHEETS = 10

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [drive] %(levelname)s: %(message)s",
    handlers=[logging.FileHandler(LOG_DIR / "store.log"), logging.StreamHandler()],
)
log = logging.getLogger("hitech-drive")
if not AGENT_SHARED_SECRET:
    log.warning("AGENT_SHARED_SECRET empty — agent API disabled until set in .env")

app = FastAPI(title="HiTech Drive Engine")
# CORS for the HCF client (client.hitech.tn) with admin-cookie credentials.
# Same-origin (lab.hitech.tn) needs nothing; LAN-IP origins stay blocked.
app.add_middleware(
    CORSMiddleware,
    allow_origins=["https://client.hitech.tn", "https://lab.hitech.tn",
                     "capacitor://localhost", "http://localhost"],
    allow_credentials=True, allow_methods=["*"], allow_headers=["*"])
# Trust the reverse proxy (Traefik/Nginx) so request.url.scheme reflects the
# real client scheme via X-Forwarded-Proto. Without this, TLS terminated
# upstream leaves the app thinking it is on plain HTTP, which breaks the
# secure=True session cookie flow (F3 in GEMINI_EYES_REPORT).
# Keep the proxy network trusted-only in production (e.g. ["127.0.0.1"]).
app.add_middleware(ProxyHeadersMiddleware, trusted_hosts=["*"])
# OTP state is shared across uvicorn workers via a locked runtime file
# (in-memory dicts desynchronize with --workers > 1).
OTP_STATE_FILE = BASE_DIR / ".otp_state.json"
OTP_LOCK_FILE = BASE_DIR / ".otp_state.lock"
OTP_MAX_ATTEMPTS = 5
templates = Jinja2Templates(directory=str(BASE_DIR / "templates"))
app.mount("/static", StaticFiles(directory=str(BASE_DIR / "static")), name="static")


# ---------------------------------------------------------------- helpers
def calculate_sha256(filepath: Path, block_size: int = 1024 * 1024) -> str:
    sha = hashlib.sha256()
    with open(filepath, "rb") as f:
        while chunk := f.read(block_size):
            sha.update(chunk)
    return sha.hexdigest()


def sha_sidecar(path: Path) -> Path:
    return path.with_name(path.name + ".sha256")


def get_sha256(path: Path) -> str | None:
    sidecar = sha_sidecar(path)
    if sidecar.exists():
        try:
            return sidecar.read_text().strip().split()[0]
        except OSError:
            return None
    return None


def safe_name(filename: str) -> str:
    name = Path(filename).name
    if not name or name in (".", ".."):
        raise HTTPException(status_code=400, detail="Nom de fichier invalide.")
    return name


def fmt_size(size: int) -> str:
    if size >= 1024**3:
        return f"{size / (1024**3):.2f} Go"
    if size >= 1024**2:
        return f"{size / (1024**2):.2f} Mo"
    if size >= 1024:
        return f"{size / 1024:.1f} Ko"
    return f"{size} o"


def preview_kind(name: str) -> str:
    ext = Path(name).suffix.lower()
    if ext in PREVIEW_AUDIO:
        return "audio"
    if ext in PREVIEW_VIDEO:
        return "video"
    if ext in PREVIEW_IMAGE:
        return "image"
    if ext in PREVIEW_MODEL:
        return "model"
    if ext in PREVIEW_MARKDOWN:
        return "markdown"
    if ext in PREVIEW_PDF:
        return "pdf"
    if ext in PREVIEW_HTML:
        return "html"
    if ext in PREVIEW_WORD:
        return "word"
    if ext in PREVIEW_EXCEL:
        return "excel"
    if ext in PREVIEW_POWERPOINT:
        return "powerpoint"
    if ext in PREVIEW_ARCHIVE:
        return "archive"
    if ext in PREVIEW_CODE:
        return "code"
    return "other"


def guess_mime(filename: str) -> str:
    ext = Path(filename).suffix.lower()
    if ext in EXTRA_MIME:
        return EXTRA_MIME[ext]
    return mimetypes.guess_type(filename)[0] or "application/octet-stream"


def esc_html(text: str) -> str:
    import html as _html
    return _html.escape(text or "")


def public_base_url(request: Request) -> str:
    """Public-facing base URL: prefer the edge proxy's forwarded host
    (Cloudflare Worker sets X-Forwarded-Host: lab.hitech.tn) so generated
    share links stay on the canonical domain."""
    fwd = (request.headers.get("x-forwarded-host") or "").strip().split(",")[0].strip()
    if fwd and re.fullmatch(r"[A-Za-z0-9.-]+(?::\d+)?", fwd):
        proto = (request.headers.get("x-forwarded-proto") or "").strip().split(",")[0].strip()
        if proto not in ("http", "https"):
            proto = request.url.scheme
        return f"{proto}://{fwd}"
    return str(request.base_url).rstrip("/")


def send_pin_email(target_email: str, pin: str):
    msg = EmailMessage()
    msg["Subject"] = f"HiTech Drive // PIN de Connexion : {pin}"
    msg["From"] = SMTP_USER
    msg["To"] = target_email
    msg.set_content(f"""
Bonjour Azmi,

Voici ton code PIN temporaire pour administrer HiTech Drive (lab.hitech.tn/store) :

================================
          {pin}
================================

- Durée de validité : 10 minutes.
- Usage unique : ce code expire dès sa première saisie.
""")
    with smtplib.SMTP(SMTP_HOST, SMTP_PORT) as server:
        server.starttls()
        server.login(SMTP_USER, SMTP_PASSWORD)
        server.send_message(msg)


def deliver_pin_or_fallback(pin: str) -> bool:
    """Email the PIN; on SMTP failure fall back to a local DEV log line.

    Returns True when the email was sent, False when the dev fallback was used.
    The OTP state is deliberately *not* cleared on fallback so the logged PIN
    stays valid for OTP_TTL_MIN minutes. With DEV_OTP_FALLBACK=0 the previous
    hard-failure behaviour is restored (500 + PIN cleared, no leak).
    """
    try:
        send_pin_email(ADMIN_EMAIL, pin)
        return True
    except Exception as e:
        if not DEV_OTP_FALLBACK:
            clear_otp()
            log.error(f"SMTP send failed: {e}")
            raise HTTPException(status_code=500, detail=f"Erreur d'envoi SMTP: {str(e)}")
        log.warning(f"SMTP unreachable ({e}) — DEV OTP fallback active "
                    f"(PIN valid {OTP_TTL_MIN} min, logged locally only; set "
                    f"DEV_OTP_FALLBACK=0 in production).")
        log.warning(f"SMTP failed. Fallback DEV PIN: {pin}")
        return False


@contextmanager
def _otp_locked():
    """Exclusive cross-process lock so concurrent workers serialize OTP access."""
    fd = os.open(str(OTP_LOCK_FILE), os.O_CREAT | os.O_RDWR, 0o600)
    try:
        fcntl.flock(fd, fcntl.LOCK_EX)
        yield
    finally:
        try:
            fcntl.flock(fd, fcntl.LOCK_UN)
        finally:
            os.close(fd)


def _otp_read_unlocked() -> dict | None:
    try:
        raw = OTP_STATE_FILE.read_text().strip()
    except (FileNotFoundError, OSError):
        return None
    if not raw:
        return None
    try:
        state = json.loads(raw)
    except (json.JSONDecodeError, ValueError):
        return None
    if not isinstance(state, dict) or "pin" not in state or "expires" not in state:
        return None
    return state


def _otp_write_unlocked(state: dict) -> None:
    tmp = OTP_STATE_FILE.with_suffix(".tmp")
    tmp.write_text(json.dumps(state))
    os.chmod(tmp, 0o600)
    os.replace(tmp, OTP_STATE_FILE)
    os.chmod(OTP_STATE_FILE, 0o600)


def _otp_clear_unlocked() -> None:
    try:
        OTP_STATE_FILE.unlink()
    except FileNotFoundError:
        pass


def _otp_expired(state: dict) -> bool:
    try:
        exp = datetime.fromisoformat(state["expires"])
    except (ValueError, TypeError):
        return True
    if exp.tzinfo is None:
        exp = exp.replace(tzinfo=timezone.utc)
    return datetime.now(timezone.utc) > exp


def issue_otp() -> str:
    pin = f"{secrets.randbelow(900000) + 100000}"
    state = {
        "pin": pin,
        "expires": (datetime.now(timezone.utc) + timedelta(minutes=OTP_TTL_MIN)).isoformat(),
        "attempts": 0,
    }
    with _otp_locked():
        _otp_write_unlocked(state)
    return pin


def clear_otp() -> None:
    with _otp_locked():
        _otp_clear_unlocked()


def consume_otp(pin: str) -> None:
    pin = (pin or "").strip()
    with _otp_locked():
        state = _otp_read_unlocked()
        if not state:
            raise HTTPException(status_code=400, detail="Aucun code en attente ou code expiré.")
        if _otp_expired(state):
            _otp_clear_unlocked()
            raise HTTPException(status_code=400, detail="Code expiré.")
        if int(state.get("attempts", 0)) >= OTP_MAX_ATTEMPTS:
            _otp_clear_unlocked()
            raise HTTPException(status_code=401,
                                detail="Trop de tentatives. Demande un nouveau code.")
        if not pin or not secrets.compare_digest(str(state["pin"]), pin):
            state["attempts"] = int(state.get("attempts", 0)) + 1
            _otp_write_unlocked(state)
            raise HTTPException(status_code=401, detail="Code PIN erroné.")
        _otp_clear_unlocked()


def make_session_token() -> str:
    return jwt.encode(
        {"sub": ADMIN_EMAIL, "exp": datetime.now(timezone.utc) + timedelta(hours=SESSION_HOURS)},
        SECRET_KEY, algorithm="HS256",
    )


def set_session_cookie(res, token: str):
    res.set_cookie(key="hitech_admin_token", value=token, httponly=True,
                   secure=True, samesite="lax", max_age=SESSION_HOURS * 3600)


def is_admin(request: Request) -> bool:
    token = request.cookies.get("hitech_admin_token")
    if not token:
        return False
    try:
        return jwt.decode(token, SECRET_KEY, algorithms=["HS256"]).get("sub") == ADMIN_EMAIL
    except JWTError:
        return False


def agent_authorized(request: Request) -> bool:
    if not AGENT_SHARED_SECRET:
        return False
    token = request.headers.get("X-Agent-Token", "")
    return bool(token) and secrets.compare_digest(token, AGENT_SHARED_SECRET)


def resolve_drive_path(rel: str) -> Path:
    """Sanitize a drive-relative path; raises 403/404 on escape or miss."""
    rel = unquote(rel or "").strip().strip("/")
    rel = re.sub(r"\x00", "", rel)
    candidate = (STORAGE_DIR / rel) if rel else STORAGE_DIR
    try:
        resolved = candidate.resolve()
    except (OSError, RuntimeError):
        raise HTTPException(status_code=400, detail="Chemin invalide.")
    try:
        resolved.relative_to(STORAGE_DIR.resolve())
    except ValueError:
        raise HTTPException(status_code=403, detail="Accès interdit (traversal).")
    return resolved


def drive_rel(path: Path) -> str:
    return str(path.resolve().relative_to(STORAGE_DIR.resolve()))


def visibility_allows(rel: str, request: Request) -> bool:
    top = rel.split("/")[0] if rel else ""
    if top == "private":
        return is_admin(request)
    if top == "agents":
        return is_admin(request) or agent_authorized(request)
    return True  # public/ + root listing


def file_info(p: Path) -> dict:
    st = p.stat()
    digest = get_sha256(p)
    return {
        "name": p.name,
        "path": drive_rel(p),
        "size": st.st_size,
        "size_str": fmt_size(st.st_size),
        "mtime": datetime.fromtimestamp(st.st_mtime).strftime("%Y-%m-%d %H:%M"),
        "sha256": digest,
        "sha_short": (digest[:16] + "…") if digest else None,
        "kind": preview_kind(p.name),
    }


def parse_range(range_header: str, size: int):
    m = re.match(r"bytes=(\d*)-(\d*)$", (range_header or "").strip())
    if not m:
        return None
    s, e = m.groups()
    start = int(s) if s else max(size - int(e), 0) if e else 0
    end = int(e) if e else size - 1
    if start >= size or end >= size or start > end:
        return None
    return start, end


def stream_file(path: Path, request: Request, filename: str, inline: bool = True):
    size = path.stat().st_size
    media = guess_mime(filename)
    disp = "inline" if inline else "attachment"
    base_headers = {
        "Accept-Ranges": "bytes",
        "Content-Disposition": f'{disp}; filename="{filename}"',
    }
    rng = parse_range(request.headers.get("range", ""), size)
    if rng is None:
        def gen_full():
            with open(path, "rb") as f:
                while chunk := f.read(CHUNK):
                    yield chunk
        return StreamingResponse(gen_full(), media_type=media,
                                 headers={**base_headers, "Content-Length": str(size)})
    start, end = rng
    length = end - start + 1

    def gen_part():
        with open(path, "rb") as f:
            f.seek(start)
            remaining = length
            while remaining > 0:
                chunk = f.read(min(CHUNK, remaining))
                if not chunk:
                    break
                remaining -= len(chunk)
                yield chunk
    return StreamingResponse(gen_part(), status_code=206, media_type=media, headers={
        **base_headers, "Content-Length": str(length),
        "Content-Range": f"bytes {start}-{end}/{size}",
    })


# ------------------------------------------------------- DRIVE UI + HEALTH
@app.api_route("/store", methods=["GET", "HEAD"], response_class=HTMLResponse)
async def drive_ui(request: Request):
    if request.method == "HEAD":
        return HTMLResponse(content="", headers={"X-Drive": "hitech-drive"})
    return templates.TemplateResponse("store.html", {"request": request})


@app.get("/store/health")
async def store_health():
    return {"status": "ok", "service": "hitech-drive", "storage": str(STORAGE_DIR)}


# ------------------------------------------------------------- AUTH API
class VerifyBody(BaseModel):
    pin: str


@app.post("/api/auth/quick-otp")
async def quick_otp():
    """1-click OTP: no email field needed, always targets the admin address."""
    pin = issue_otp()
    emailed = deliver_pin_or_fallback(pin)
    if not emailed:
        return {"ok": True, "expires_in_s": OTP_TTL_MIN * 60,
                "delivery": "dev_log_fallback",
                "message": "SMTP indisponible : code PIN de secours écrit dans les logs du service (mode DEV)."}
    log.info("Quick-OTP dispatched to admin email")
    return {"ok": True, "expires_in_s": OTP_TTL_MIN * 60, "delivery": "email"}


@app.post("/api/auth/verify-otp")
async def verify_otp_json(body: VerifyBody):
    """Canonical JSON PIN validation (file-backed, worker-safe)."""
    consume_otp(body.pin)
    token = make_session_token()
    res = JSONResponse({"ok": True, "token": token, "max_age_s": SESSION_HOURS * 3600})
    set_session_cookie(res, token)
    log.info("Admin session established via JSON API (12h JWT)")
    return res


@app.post("/api/auth/verify")
async def verify_json(body: VerifyBody):
    """Backward-compatible alias of /api/auth/verify-otp."""
    return await verify_otp_json(body)


@app.get("/api/auth/me")
async def auth_me(request: Request):
    return {"admin": is_admin(request), "agent": agent_authorized(request)}


# ------------------------------------------------------------- DRIVE API
@app.get("/api/drive/list")
async def drive_list(request: Request, path: str = ""):
    target = resolve_drive_path(path)
    rel = "" if target.resolve() == STORAGE_DIR.resolve() else drive_rel(target)
    if not visibility_allows(rel, request):
        raise HTTPException(status_code=403, detail="Session admin requise.")
    if not target.exists():
        raise HTTPException(status_code=404, detail="Dossier introuvable.")
    if not target.is_dir():
        raise HTTPException(status_code=400, detail="Pas un dossier.")
    crumbs = [{"name": "Home", "path": ""}]
    if rel:
        acc = []
        for part in rel.split("/"):
            acc.append(part)
            crumbs.append({"name": part, "path": "/".join(acc)})
    dirs, files = [], []
    for child in sorted(target.iterdir(), key=lambda p: (p.is_file(), p.name.lower())):
        if child.name.endswith(".sha256"):
            continue
        child_rel = drive_rel(child)
        if not visibility_allows(child_rel, request):
            continue
        if child.is_dir():
            dirs.append({"name": child.name, "path": child_rel})
        elif child.is_file():
            files.append(file_info(child))
    return {"path": rel, "breadcrumbs": crumbs,
            "dirs": dirs, "files": files, "admin": is_admin(request)}


@app.api_route("/api/drive/download", methods=["GET", "HEAD"])
async def drive_download(request: Request, path: str):
    target = resolve_drive_path(path)
    rel = drive_rel(target)
    if not visibility_allows(rel, request):
        raise HTTPException(status_code=403, detail="Session admin requise.")
    if not target.exists() or not target.is_file():
        raise HTTPException(status_code=404, detail="Fichier introuvable.")
    log.info(f"Drive download: {rel}")
    return stream_file(target, request, target.name, inline=True)


@app.post("/api/drive/upload")
async def drive_upload(request: Request, folder: str = Form("public"),
                       file: UploadFile = File(...)):
    if not is_admin(request):
        raise HTTPException(status_code=403, detail="Session expirée.")
    folder = (folder or "public").strip().strip("/")
    if folder not in UPLOAD_FOLDERS:
        raise HTTPException(status_code=400, detail="Dossier cible invalide.")
    name = safe_name(file.filename or "unnamed")
    dest_dir = resolve_drive_path(folder)
    dest_dir.mkdir(parents=True, exist_ok=True)
    target = dest_dir / name
    if (target.resolve().parent != dest_dir.resolve()):
        raise HTTPException(status_code=403, detail="Accès interdit.")
    length = request.headers.get("content-length")
    if length and int(length) > MAX_UPLOAD_BYTES + 1024**2:
        raise HTTPException(status_code=413, detail="Fichier trop volumineux (max 15 Go).")
    sha = hashlib.sha256()
    total = 0
    with open(target, "wb") as buf:
        while chunk := await file.read(1024 * 1024 * 10):
            total += len(chunk)
            if total > MAX_UPLOAD_BYTES:
                buf.close()
                target.unlink(missing_ok=True)
                raise HTTPException(status_code=413, detail="Fichier trop volumineux (max 15 Go).")
            buf.write(chunk)
            sha.update(chunk)
    sha_sidecar(target).write_text(sha.hexdigest() + f"  {name}\n")
    log.info(f"Drive upload {folder}/{name} ({total} bytes)")
    return {"ok": True, "path": f"{folder}/{name}", "size": total, "sha256": sha.hexdigest()}


def safe_segments(relpath: str) -> list:
    """Split a relative path into clean segments, dropping traversal tricks."""
    parts = []
    for seg in (relpath or "").replace("\\", "/").split("/"):
        seg = seg.strip()
        if not seg or seg in (".", ".."):
            continue
        seg = Path(seg).name[:255]
        if seg and seg not in (".", ".."):
            parts.append(seg)
    return parts


def top_allowed(rel: str) -> bool:
    """Mirror upload rules: public/*, private/*, agents/<known>/*."""
    bits = rel.split("/")
    if bits[0] in ("public", "private"):
        return True
    return len(bits) > 1 and bits[0] == "agents" and bits[1] in ALLOWED_AGENT_FOLDERS


async def _write_upload_file(dest_dir: Path, filename: str, file: UploadFile) -> dict:
    name = safe_name(filename)
    dest_dir.mkdir(parents=True, exist_ok=True)
    target = dest_dir / name
    if target.resolve().parent != dest_dir.resolve():
        raise HTTPException(status_code=403, detail="Accès interdit.")
    sha = hashlib.sha256()
    total = 0
    with open(target, "wb") as buf:
        while chunk := await file.read(1024 * 1024 * 10):
            total += len(chunk)
            if total > MAX_UPLOAD_BYTES:
                buf.close()
                target.unlink(missing_ok=True)
                raise HTTPException(status_code=413, detail="Fichier trop volumineux (max 15 Go).")
            buf.write(chunk)
            sha.update(chunk)
    sha_sidecar(target).write_text(sha.hexdigest() + f"  {name}\n")
    return {"path": drive_rel(target), "size": total, "sha256": sha.hexdigest()}


class MkdirBody(BaseModel):
    parent: str = ""
    name: str = ""


@app.post("/api/drive/mkdir")
async def drive_mkdir(request: Request, body: MkdirBody):
    """Create a new folder (admin only). body: {parent, name}."""
    if not is_admin(request):
        raise HTTPException(status_code=403, detail="Session admin requise.")
    parent = resolve_drive_path(body.parent or "")
    if not parent.exists() or not parent.is_dir():
        raise HTTPException(status_code=404, detail="Dossier parent introuvable.")
    segs = safe_segments(body.name)
    if not segs or len(segs) > 5:
        raise HTTPException(status_code=400, detail="Nom de dossier invalide.")
    target = parent.joinpath(*segs)
    try:
        rel = str(target.resolve().relative_to(STORAGE_DIR.resolve()))
    except (ValueError, OSError, RuntimeError):
        raise HTTPException(status_code=403, detail="Accès interdit (traversal).")
    if not top_allowed(rel):
        raise HTTPException(status_code=400, detail="Dossier cible invalide.")
    target.mkdir(parents=True, exist_ok=True)
    log.info(f"Drive mkdir: {rel}")
    return {"ok": True, "path": rel}


@app.post("/api/drive/upload-batch")
async def drive_upload_batch(request: Request, folder: str = Form("public"),
                             paths: str = Form("[]"),
                             files: list[UploadFile] = File(...)):
    """Upload many files at once (whole-folder upload). `paths` = JSON array
    of relative paths aligned with `files`. Admin only."""
    if not is_admin(request):
        raise HTTPException(status_code=403, detail="Session expirée.")
    folder = (folder or "public").strip().strip("/")
    base_dir = resolve_drive_path(folder)
    try:
        base_rel = "" if base_dir.resolve() == STORAGE_DIR.resolve() else drive_rel(base_dir)
    except (OSError, RuntimeError):
        raise HTTPException(status_code=400, detail="Dossier cible invalide.")
    if base_rel and not top_allowed(base_rel):
        raise HTTPException(status_code=400, detail="Dossier cible invalide.")
    try:
        relpaths = json.loads(paths)
    except (json.JSONDecodeError, ValueError):
        raise HTTPException(status_code=400, detail="Liste de chemins invalide.")
    if not isinstance(relpaths, list):
        raise HTTPException(status_code=400, detail="Liste de chemins invalide.")
    if len(files) > 500:
        raise HTTPException(status_code=400, detail="Trop de fichiers (max 500).")
    base_dir = resolve_drive_path(folder)
    base_dir.mkdir(parents=True, exist_ok=True)
    base_resolved = base_dir.resolve()
    results = []
    for i, file in enumerate(files):
        rel = relpaths[i] if i < len(relpaths) else (file.filename or "unnamed")
        segs = safe_segments(rel)
        if not segs:
            results.append({"ok": False, "path": str(rel)[:120], "error": "Chemin invalide."})
            continue
        *dirs, fname = segs
        try:
            dest_dir = base_resolved.joinpath(*dirs) if dirs else base_resolved
            dest_dir.mkdir(parents=True, exist_ok=True)
            try:
                dest_dir.resolve().relative_to(base_resolved)
            except ValueError:
                raise HTTPException(status_code=403, detail="Accès interdit (traversal).")
            info = await _write_upload_file(dest_dir, fname, file)
            results.append({"ok": True, **info})
        except HTTPException as e:
            results.append({"ok": False, "path": "/".join(segs)[:120], "error": e.detail})
        except Exception as e:
            log.warning(f"Batch upload failed for {'/'.join(segs)}: {e}")
            results.append({"ok": False, "path": "/".join(segs)[:120], "error": "Échec d'écriture."})
    ok_count = sum(1 for r in results if r.get("ok"))
    log.info(f"Drive upload-batch {folder}/: {ok_count}/{len(results)} fichiers")
    return {"ok": True, "uploaded": ok_count, "total": len(results), "files": results}


# ------------------------------------------- document text extraction
def _extract_pdf(path: Path):
    from pypdf import PdfReader
    reader = PdfReader(str(path))
    total_pages = len(reader.pages)
    pages = min(total_pages, EXTRACT_MAX_PDF_PAGES)
    parts, chars = [], 0
    for i in range(pages):
        txt = reader.pages[i].extract_text() or ""
        if len(txt) > 20000:
            txt = txt[:20000] + " […]"
        # dir="auto" lets the browser bidi-resolve each block (Arabic RTL vs
        # Latin LTR) instead of forcing the inherited page direction (F5).
        parts.append(f'<h4>Page {i + 1}</h4><pre dir="auto">{esc_html(txt)}</pre>')
        chars += len(txt)
        if chars >= EXTRACT_MAX_CHARS:
            break
    body = "".join(parts) or "<p class='muted'>Aucun texte extractible (PDF scanné ?).</p>"
    return f"<div class='docview'>{body}</div>", (total_pages > pages or chars >= EXTRACT_MAX_CHARS)


def _extract_docx(path: Path):
    import docx
    doc = docx.Document(str(path))
    parts, chars = [], 0
    for p in doc.paragraphs:
        t = (p.text or "").strip()
        if not t:
            continue
        style = (p.style.name if p.style else "")
        parts.append(f"<h4>{esc_html(t)}</h4>" if style.startswith("Heading") else f"<p>{esc_html(t)}</p>")
        chars += len(t)
        if chars >= EXTRACT_MAX_CHARS:
            break
    for table in doc.tables:
        if chars >= EXTRACT_MAX_CHARS:
            break
        rows = []
        for row in table.rows[:50]:
            rows.append("<tr>" + "".join(f"<td>{esc_html(c.text)}</td>" for c in row.cells) + "</tr>")
        parts.append("<table>" + "".join(rows) + "</table>")
    body = "".join(parts) or "<p class='muted'>Document vide.</p>"
    return f"<div class='docview'>{body}</div>", chars >= EXTRACT_MAX_CHARS


def _extract_xlsx(path: Path):
    import openpyxl
    wb = openpyxl.load_workbook(str(path), read_only=True, data_only=True)
    parts, truncated = [], False
    for ws in wb.worksheets[:EXTRACT_MAX_SHEETS]:
        parts.append(f"<h4>{esc_html(ws.title)}</h4><table>")
        n = 0
        for row in ws.iter_rows(min_row=1, max_row=EXTRACT_MAX_SHEET_ROWS, values_only=True):
            if all(v is None or str(v).strip() == "" for v in row):
                continue
            parts.append("<tr>" + "".join(
                f"<td>{esc_html('' if v is None else str(v))}</td>" for v in row) + "</tr>")
            n += 1
        parts.append("</table>")
        if n >= EXTRACT_MAX_SHEET_ROWS:
            truncated = True
    wb.close()
    if len(wb.worksheets) > EXTRACT_MAX_SHEETS:
        truncated = True
    body = "".join(parts) or "<p class='muted'>Classeur vide.</p>"
    return f"<div class='docview'>{body}</div>", truncated


def _extract_csv(path: Path, delimiter: str):
    import csv
    parts = ["<div class='docview'><table>"]
    n = 0
    with open(path, newline="", encoding="utf-8-sig") as f:
        for row in csv.reader(f, delimiter=delimiter):
            if all((c or "").strip() == "" for c in row):
                continue
            parts.append("<tr>" + "".join(f"<td>{esc_html(c)}</td>" for c in row) + "</tr>")
            n += 1
            if n >= EXTRACT_MAX_SHEET_ROWS:
                break
    parts.append("</table></div>")
    return "".join(parts), n >= EXTRACT_MAX_SHEET_ROWS


def _extract_pptx(path: Path):
    import pptx
    prs = pptx.Presentation(str(path))
    parts = []
    for i, slide in enumerate(list(prs.slides)[:100], 1):
        texts = []
        for shape in slide.shapes:
            if shape.has_table:
                rows = []
                for row in list(shape.table.rows)[:20]:
                    rows.append("<tr>" + "".join(
                        f"<td>{esc_html(cell.text)}</td>" for cell in row.cells) + "</tr>")
                texts.append("<table>" + "".join(rows) + "</table>")
            elif shape.has_text_frame:
                t = (shape.text or "").strip()
                if t:
                    texts.append(t)
        if not texts:
            continue
        parts.append(f"<h4>Slide {i} — {esc_html(texts[0][:120])}</h4>" + "".join(
            t if t.startswith("<table>") else f"<p>{esc_html(t)}</p>" for t in texts[1:]))
    body = "".join(parts) or "<p class='muted'>Aucun texte extractible.</p>"
    return f"<div class='docview'>{body}</div>", len(list(prs.slides)) > 100


def extract_document(path: Path, kind: str):
    ext = path.suffix.lower()
    if kind == "pdf":
        return _extract_pdf(path)
    if kind == "word":
        if ext == ".docx":
            return _extract_docx(path)
        return (f"<div class='docview'><p class='muted'>Format {esc_html(ext)} ancien : "
                "aperçu non convertible — téléchargez le fichier.</p></div>", False)
    if kind == "excel":
        if ext in (".xlsx", ".xlsm", ".xltx", ".xltm"):
            return _extract_xlsx(path)
        if ext == ".csv":
            return _extract_csv(path, ",")
        if ext == ".tsv":
            return _extract_csv(path, "\t")
        return (f"<div class='docview'><p class='muted'>Format {esc_html(ext)} ancien : "
                "aperçu non convertible — téléchargez le fichier.</p></div>", False)
    if kind == "powerpoint":
        if ext == ".pptx":
            return _extract_pptx(path)
        return (f"<div class='docview'><p class='muted'>Format {esc_html(ext)} ancien : "
                "aperçu non convertible — téléchargez le fichier.</p></div>", False)
    raise ValueError("unsupported")


@app.get("/api/drive/extract")
async def drive_extract(request: Request, path: str):
    """Server-side readable excerpt of PDF / Office documents (safe HTML)."""
    target = resolve_drive_path(path)
    rel = drive_rel(target)
    if not visibility_allows(rel, request):
        raise HTTPException(status_code=403, detail="Session admin requise.")
    if not target.exists() or not target.is_file():
        raise HTTPException(status_code=404, detail="Fichier introuvable.")
    kind = preview_kind(target.name)
    if kind not in ("pdf", "word", "excel", "powerpoint"):
        raise HTTPException(status_code=400, detail="Extraction non supportée pour ce format.")
    if target.stat().st_size > 100 * 1024 * 1024:
        raise HTTPException(status_code=413, detail="Fichier trop volumineux pour l'aperçu (téléchargez-le).")
    try:
        html, truncated = extract_document(target, kind)
    except Exception as e:
        log.warning(f"Extract failed for {rel}: {e}")
        raise HTTPException(status_code=422, detail="Aperçu impossible pour ce fichier (téléchargez-le).")
    return {"ok": True, "kind": kind, "name": target.name, "html": html, "truncated": truncated}


@app.get("/api/drive/archive")
async def drive_archive(request: Request, path: str):
    """List entries of a ZIP / TAR archive without extracting it."""
    import tarfile
    import zipfile
    target = resolve_drive_path(path)
    rel = drive_rel(target)
    if not visibility_allows(rel, request):
        raise HTTPException(status_code=403, detail="Session admin requise.")
    if not target.exists() or not target.is_file():
        raise HTTPException(status_code=404, detail="Fichier introuvable.")
    if preview_kind(target.name) != "archive":
        raise HTTPException(status_code=400, detail="Pas une archive supportée.")
    entries = []
    try:
        if zipfile.is_zipfile(target):
            with zipfile.ZipFile(target) as z:
                for info in z.infolist()[:500]:
                    if info.is_dir():
                        continue
                    entries.append({"name": info.filename, "size": info.file_size,
                                    "size_str": fmt_size(info.file_size)})
        elif tarfile.is_tarfile(target):
            with tarfile.open(target, "r") as t:
                for m in t.getmembers()[:500]:
                    if not m.isfile():
                        continue
                    entries.append({"name": m.name, "size": m.size, "size_str": fmt_size(m.size)})
        else:
            raise HTTPException(status_code=422, detail="Archive illisible (format non supporté).")
    except HTTPException:
        raise
    except Exception as e:
        log.warning(f"Archive list failed for {rel}: {e}")
        raise HTTPException(status_code=422, detail="Archive illisible.")
    return {"ok": True, "name": target.name, "count": len(entries), "entries": entries}


# ------------------------------------------- office -> PDF (LibreOffice)
OFFICE_CACHE_DIR = BASE_DIR / ".office_cache"
OFFICE_CACHE_DIR.mkdir(parents=True, exist_ok=True)
OFFICE_LOCK_FILE = BASE_DIR / ".office.lock"
OFFICE_MAX_BYTES = 100 * 1024 * 1024
OFFICE_CONVERT_TIMEOUT = 120
OFFICE_CACHE_KEEP = 200


@contextmanager
def _office_locked():
    """Serialize LibreOffice conversions (soffice profile is process-sensitive)."""
    fd = os.open(str(OFFICE_LOCK_FILE), os.O_CREAT | os.O_RDWR, 0o600)
    try:
        fcntl.flock(fd, fcntl.LOCK_EX)
        yield
    finally:
        try:
            fcntl.flock(fd, fcntl.LOCK_UN)
        finally:
            os.close(fd)


def _office_cache_key(rel: str, target: Path) -> Path:
    st = target.stat()
    digest = hashlib.sha256(f"{rel}:{st.st_size}:{st.st_mtime_ns}".encode()).hexdigest()[:32]
    return OFFICE_CACHE_DIR / f"{digest}.pdf"


def _office_cache_prune() -> None:
    try:
        pdfs = sorted(OFFICE_CACHE_DIR.glob("*.pdf"), key=lambda p: p.stat().st_mtime)
        for old in pdfs[:-OFFICE_CACHE_KEEP]:
            try:
                old.unlink()
            except OSError:
                pass
    except OSError:
        pass


def convert_office_to_pdf(src: Path, dst: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="lo-convert-") as tmp:
        cmd = ["soffice", "--headless", "--nolockcheck",
               f"-env:UserInstallation=file://{tmp}/profile",
               "--convert-to", "pdf", "--outdir", tmp, str(src)]
        subprocess.run(cmd, capture_output=True, timeout=OFFICE_CONVERT_TIMEOUT, check=True)
        produced = Path(tmp) / (src.stem + ".pdf")
        if not produced.exists():
            pdfs = list(Path(tmp).glob("*.pdf"))
            if not pdfs:
                raise RuntimeError("LibreOffice n'a produit aucun PDF.")
            produced = pdfs[0]
        shutil.move(str(produced), str(dst))
        os.chmod(dst, 0o600)


@app.get("/api/drive/office-pdf")
async def drive_office_pdf(request: Request, path: str):
    """Pixel-faithful Office rendering: convert to PDF via LibreOffice (cached)."""
    target = resolve_drive_path(path)
    rel = drive_rel(target)
    if not visibility_allows(rel, request):
        raise HTTPException(status_code=403, detail="Session admin requise.")
    if not target.exists() or not target.is_file():
        raise HTTPException(status_code=404, detail="Fichier introuvable.")
    if preview_kind(target.name) not in ("word", "excel", "powerpoint"):
        raise HTTPException(status_code=400, detail="Pas un document bureautique.")
    if target.stat().st_size > OFFICE_MAX_BYTES:
        raise HTTPException(status_code=413, detail="Fichier trop volumineux pour la conversion.")
    key = _office_cache_key(rel, target)
    if not key.exists():
        with _office_locked():
            if not key.exists():
                try:
                    convert_office_to_pdf(target, key)
                    _office_cache_prune()
                except subprocess.TimeoutExpired:
                    raise HTTPException(status_code=422, detail="Conversion trop longue.")
                except Exception as e:
                    log.warning(f"Office->PDF failed for {rel}: {e}")
                    raise HTTPException(status_code=422,
                                        detail="Conversion impossible (utilisez l'aperçu texte).")
    log.info(f"Office-PDF view: {rel}")
    return stream_file(key, request, target.stem + ".pdf", inline=True)


# ------------------------------------------------------- SHARE LINKS v2
# Public share links for files AND folders, with TTL + optional single-use.
# State lives in a locked runtime file (worker-safe, like OTP state).
SHARES_FILE = BASE_DIR / ".shares.json"
SHARES_LOCK_FILE = BASE_DIR / ".shares.lock"


@contextmanager
def _shares_locked():
    fd = os.open(str(SHARES_LOCK_FILE), os.O_CREAT | os.O_RDWR, 0o600)
    try:
        fcntl.flock(fd, fcntl.LOCK_EX)
        yield
    finally:
        try:
            fcntl.flock(fd, fcntl.LOCK_UN)
        finally:
            os.close(fd)


def _shares_read_unlocked() -> dict:
    try:
        raw = SHARES_FILE.read_text().strip()
    except (FileNotFoundError, OSError):
        return {}
    try:
        data = json.loads(raw)
    except (json.JSONDecodeError, ValueError):
        return {}
    return data if isinstance(data, dict) else {}


def _shares_write_unlocked(data: dict) -> None:
    tmp = SHARES_FILE.with_suffix(".tmp")
    tmp.write_text(json.dumps(data))
    os.chmod(tmp, 0o600)
    os.replace(tmp, SHARES_FILE)
    os.chmod(SHARES_FILE, 0o600)


def _share_sign(share_id: str) -> str:
    return hmac.new(SECRET_KEY.encode(), f"share:{share_id}".encode(),
                    hashlib.sha256).hexdigest()[:32]


def _share_new_sid(token: str) -> str | None:
    """Return share id if token carries a valid v2 signature, else None."""
    try:
        sid, sig = (token or "").split(".", 1)
    except (ValueError, AttributeError):
        return None
    if not sid or not sig:
        return None
    try:
        if hmac.compare_digest(sig, _share_sign(sid)):
            return sid
    except (TypeError, ValueError):
        return None
    return None


def _share_lookup(sid: str) -> dict | None:
    with _shares_locked():
        return _shares_read_unlocked().get(sid)


def _share_peek_valid(sid: str) -> dict:
    """Validate a v2 share without consuming a use (for folder page views)."""
    with _shares_locked():
        data = _shares_read_unlocked()
        rec = data.get(sid)
        if not rec:
            raise HTTPException(status_code=401, detail="Lien révoqué ou inexistant.")
        if int(rec.get("exp", 0)) < int(datetime.now(timezone.utc).timestamp()):
            del data[sid]
            _shares_write_unlocked(data)
            raise HTTPException(status_code=401, detail="Lien expiré.")
        return dict(rec)


def _share_consume_download(sid: str) -> dict:
    """Validate a v2 share and count one download; single-use links are deleted."""
    with _shares_locked():
        data = _shares_read_unlocked()
        rec = data.get(sid)
        if not rec:
            raise HTTPException(status_code=401, detail="Lien révoqué ou inexistant.")
        if int(rec.get("exp", 0)) < int(datetime.now(timezone.utc).timestamp()):
            del data[sid]
            _shares_write_unlocked(data)
            raise HTTPException(status_code=401, detail="Lien expiré.")
        rec["uses"] = int(rec.get("uses", 0)) + 1
        if int(rec.get("max_uses", 0)) > 0 and rec["uses"] >= int(rec["max_uses"]):
            del data[sid]
        else:
            data[sid] = rec
        _shares_write_unlocked(data)
        return dict(rec)


def _share_scope_ok(rec: dict, rel: str) -> bool:
    root = rec.get("path", "")
    if rec.get("dir"):
        return root == "" or rel == root or rel.startswith(root + "/")
    return rel == root


class ShareCreateBody(BaseModel):
    path: str
    ttl_min: int = 60
    max_uses: int = 0
    label: str = ""


class ShareRevokeBody(BaseModel):
    id: str


@app.post("/api/share/create")
async def share_create(request: Request, body: ShareCreateBody):
    """Create a public share link for a file or folder (admin only)."""
    if not is_admin(request):
        raise HTTPException(status_code=403, detail="Session admin requise.")
    target = resolve_drive_path(body.path or "")
    if not target.exists():
        raise HTTPException(status_code=404, detail="Introuvable.")
    if target.resolve() == STORAGE_DIR.resolve():
        rel = ""
    else:
        rel = drive_rel(target)
    if rel.split("/")[0] == "private":
        raise HTTPException(status_code=403, detail="Partage interdit pour /private.")
    ttl = max(1, min(int(body.ttl_min or 60), 43200))
    max_uses = max(0, min(int(body.max_uses or 0), 10000))
    now = datetime.now(timezone.utc)
    sid = secrets.token_hex(8)
    rec = {"id": sid, "path": rel, "dir": target.is_dir(), "name": target.name,
           "exp": int((now + timedelta(minutes=ttl)).timestamp()),
           "expires_at": (now + timedelta(minutes=ttl)).isoformat(),
           "max_uses": max_uses, "uses": 0,
           "created": now.isoformat(), "label": (body.label or "")[:80]}
    with _shares_locked():
        data = _shares_read_unlocked()
        data[sid] = rec
        _shares_write_unlocked(data)
    token = f"{sid}.{_share_sign(sid)}"
    base = public_base_url(request)
    if rec["dir"]:
        url = f"{base}/share-folder/{quote(rel)}?token={token}" if rel else f"{base}/share-folder/?token={token}"
    else:
        url = f"{base}/share/{quote(rel)}?token={token}"
    log.info(f"Share created: {rel or '/'} (dir={rec['dir']}, ttl={ttl}min, max_uses={max_uses})")
    return {"ok": True, "id": sid, "token": token, "url": url,
            "expires_at": rec["expires_at"], "max_uses": max_uses, "dir": rec["dir"]}


@app.get("/api/share/list")
async def share_list(request: Request):
    """List active share links (admin only)."""
    if not is_admin(request):
        raise HTTPException(status_code=403, detail="Session admin requise.")
    now = int(datetime.now(timezone.utc).timestamp())
    with _shares_locked():
        data = _shares_read_unlocked()
        expired = [k for k, r in data.items() if int(r.get("exp", 0)) < now]
        for k in expired:
            del data[k]
        if expired:
            _shares_write_unlocked(data)
        items = sorted(data.values(), key=lambda r: r.get("created", ""), reverse=True)
    base = public_base_url(request)
    out = []
    for r in items:
        token = f"{r['id']}.{_share_sign(r['id'])}"
        if r.get("dir"):
            rel = r.get("path", "")
            url = f"{base}/share-folder/{quote(rel)}?token={token}" if rel else f"{base}/share-folder/?token={token}"
        else:
            url = f"{base}/share/{quote(r.get('path', ''))}?token={token}"
        out.append({"id": r["id"], "name": r.get("name", ""), "path": r.get("path", ""),
                    "dir": r.get("dir", False), "url": url, "expires_at": r.get("expires_at", ""),
                    "uses": int(r.get("uses", 0)), "max_uses": int(r.get("max_uses", 0)),
                    "label": r.get("label", "")})
    return {"ok": True, "shares": out}


@app.post("/api/share/revoke")
async def share_revoke(request: Request, body: ShareRevokeBody):
    """Revoke a share link immediately (admin only)."""
    if not is_admin(request):
        raise HTTPException(status_code=403, detail="Session admin requise.")
    with _shares_locked():
        data = _shares_read_unlocked()
        if body.id in data:
            del data[body.id]
            _shares_write_unlocked(data)
            log.info(f"Share revoked: {body.id}")
            return {"ok": True}
    raise HTTPException(status_code=404, detail="Lien introuvable.")


@app.api_route("/share-folder/{folder_path:path}", methods=["GET", "HEAD"])
async def share_folder_page(folder_path: str, request: Request, token: str = ""):
    """Public shared-folder page: browse + download within the shared root."""
    sid = _share_new_sid(token)
    if sid is None:
        raise HTTPException(status_code=401, detail="Lien de partage invalide.")
    rec = _share_peek_valid(sid)
    if not rec.get("dir"):
        raise HTTPException(status_code=400, detail="Ce lien partage un fichier, pas un dossier.")
    wanted = (folder_path or "").strip().strip("/")
    root = rec.get("path", "")
    rel = wanted or root
    if root and not (rel == root or rel.startswith(root + "/")):
        raise HTTPException(status_code=403, detail="Hors du dossier partagé.")
    target = resolve_drive_path(rel)
    if not target.exists() or not target.is_dir():
        raise HTTPException(status_code=404, detail="Dossier introuvable.")
    if request.method == "HEAD":
        return HTMLResponse(content="", headers={"X-Share": sid})
    crumbs = [(rec.get("name") or "Partage", root)]
    if rel != root:
        acc = []
        rest = rel[len(root) + 1:] if root else rel
        for part in rest.split("/"):
            acc.append(part)
            crumbs.append((part, (root + "/" + "/".join(acc)) if root else "/".join(acc)))
    rows = []
    for child in sorted(target.iterdir(), key=lambda p: (p.is_file(), p.name.lower())):
        if child.name.endswith(".sha256"):
            continue
        c_rel = drive_rel(child)
        if child.is_dir():
            rows.append(f"<li>📁 <a href='/share-folder/{quote(c_rel)}?token={esc_html(token)}'>{esc_html(child.name)}</a></li>")
        elif child.is_file():
            info = file_info(child)
            rows.append(f"<li>{esc_html(child.name)} <span class='sz'>({info['size_str']})</span> "
                        f"<a href='/share/{quote(c_rel)}?token={esc_html(token)}'>⬇ Télécharger</a></li>")
    exp_txt = rec.get("expires_at", "")[:16].replace("T", " ")
    quota = (f"{rec.get('uses', 0)}/{rec.get('max_uses', 0)} téléchargements utilisés"
             if int(rec.get("max_uses", 0)) > 0 else "téléchargements illimités")
    crumb_html = " › ".join(
        f"<a href='/share-folder/{quote(p)}?token={esc_html(token)}'>{esc_html(n)}</a>" for n, p in crumbs)
    return HTMLResponse(f"""<!DOCTYPE html>
<html lang="fr"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Dossier partagé — HiTech Drive</title></head>
<body style="font-family:system-ui,sans-serif;background:#0b1220;color:#e8f0fc;margin:0;padding:2rem;">
<div style="max-width:760px;margin:auto;background:#111c30;border:1px solid #24344f;border-radius:12px;padding:1.6rem;">
<h2 style="margin-top:0;color:#38bdf8;">🔗 Dossier partagé — {esc_html(rec.get('name') or 'Drive')}</h2>
<p style="color:#8ba0bd;font-size:.85rem;">{crumb_html}</p>
<ul style="list-style:none;padding:0;">{''.join(rows) if rows else "<li style='color:#8ba0bd;'>Dossier vide.</li>"}</ul>
<p style="color:#8ba0bd;font-size:.8rem;">Expire le {esc_html(exp_txt)} · {esc_html(quota)}</p>
</div></body></html>""")


# ------------------------------------------------------- AGENT ARTIFACT API
class ShareLinkBody(BaseModel):
    path: str
    ttl_min: int = 60


def sign_share(rel: str, exp: int) -> str:
    return hmac.new(AGENT_SHARED_SECRET.encode(), f"{rel}:{exp}".encode(),
                    hashlib.sha256).hexdigest()


@app.post("/api/v1/artifacts/upload")
async def artifact_upload(request: Request, folder: str = Form("shared"),
                          file: UploadFile = File(...)):
    if not agent_authorized(request) and not is_admin(request):
        raise HTTPException(status_code=401, detail="X-Agent-Token invalide.")
    folder = (folder or "shared").strip().strip("/")
    if folder not in ALLOWED_AGENT_FOLDERS:
        raise HTTPException(status_code=400, detail="Dossier agent invalide (genio/opencode/claude/shared).")
    name = safe_name(file.filename or "unnamed")
    dest_dir = resolve_drive_path(f"agents/{folder}")
    dest_dir.mkdir(parents=True, exist_ok=True)
    target = dest_dir / name
    sha = hashlib.sha256()
    total = 0
    with open(target, "wb") as buf:
        while chunk := await file.read(1024 * 1024 * 10):
            total += len(chunk)
            if total > MAX_UPLOAD_BYTES:
                buf.close()
                target.unlink(missing_ok=True)
                raise HTTPException(status_code=413, detail="Fichier trop volumineux (max 15 Go).")
            buf.write(chunk)
            sha.update(chunk)
    sha_sidecar(target).write_text(sha.hexdigest() + f"  {name}\n")
    log.info(f"Agent artifact upload agents/{folder}/{name} ({total} bytes)")
    return {"ok": True, "path": f"agents/{folder}/{name}",
            "size": total, "sha256": sha.hexdigest()}


@app.get("/api/v1/artifacts/list")
async def artifact_list(request: Request, folder: str = "shared"):
    if not agent_authorized(request) and not is_admin(request):
        raise HTTPException(status_code=401, detail="X-Agent-Token invalide.")
    folder = (folder or "shared").strip().strip("/")
    if folder not in ALLOWED_AGENT_FOLDERS:
        raise HTTPException(status_code=400, detail="Dossier agent invalide.")
    target = resolve_drive_path(f"agents/{folder}")
    target.mkdir(parents=True, exist_ok=True)
    return {"folder": f"agents/{folder}",
            "files": [file_info(p) for p in sorted(target.iterdir())
                      if p.is_file() and not p.name.endswith(".sha256")]}


@app.post("/api/v1/artifacts/create-share-link")
async def create_share_link(request: Request, body: ShareLinkBody):
    if not agent_authorized(request) and not is_admin(request):
        raise HTTPException(status_code=401, detail="X-Agent-Token invalide.")
    target = resolve_drive_path(body.path)
    rel = drive_rel(target)
    if rel.split("/")[0] == "private":
        raise HTTPException(status_code=403, detail="Partage interdit pour /private.")
    if not target.exists() or not target.is_file():
        raise HTTPException(status_code=404, detail="Fichier introuvable.")
    ttl = max(1, min(int(body.ttl_min or 60), 43200))
    exp = int((datetime.now(timezone.utc) + timedelta(minutes=ttl)).timestamp())
    token = f"{exp}.{sign_share(rel, exp)}"
    base = public_base_url(request)
    return {"ok": True, "artifact_id": rel, "token": token,
            "expires_at": datetime.fromtimestamp(exp, tz=timezone.utc).isoformat(),
            "url": f"{base}/share/{quote(rel)}?token={token}"}


@app.api_route("/share/{artifact_path:path}", methods=["GET", "HEAD"])
async def share_download(artifact_path: str, request: Request, token: str = ""):
    target = resolve_drive_path(artifact_path)
    rel = drive_rel(target)
    if rel.split("/")[0] == "private":
        raise HTTPException(status_code=403, detail="Partage interdit pour /private.")
    if not target.exists() or not target.is_file():
        raise HTTPException(status_code=404, detail="Fichier introuvable.")
    sid = _share_new_sid(token)
    if sid is not None:
        # v2 link: TTL + usage quota enforced, file-or-folder scope checked
        rec = _share_consume_download(sid)
        if not _share_scope_ok(rec, rel):
            raise HTTPException(status_code=403, detail="Hors du partage autorisé.")
        log.info(f"Share v2 download: {rel} (uses={rec.get('uses', 0)})")
    else:
        # v1 legacy stateless token {exp}.{hmac}
        try:
            exp_s, sig = token.split(".", 1)
            exp = int(exp_s)
        except (ValueError, AttributeError):
            raise HTTPException(status_code=401, detail="Token invalide.")
        if exp < int(datetime.now(timezone.utc).timestamp()):
            raise HTTPException(status_code=401, detail="Lien expiré.")
        if not hmac.compare_digest(sig, sign_share(rel, exp)):
            raise HTTPException(status_code=401, detail="Signature invalide.")
        log.info(f"Share download: {rel}")
    return stream_file(target, request, target.name, inline=True)


# ------------------------------------------- LEGACY ROUTES (compatibilité)
@app.get("/store/download/{filename}")
async def download_file(filename: str, request: Request):
    name = safe_name(filename)
    file_path = STORAGE_DIR / name
    if not file_path.exists() or not file_path.is_file():
        raise HTTPException(status_code=404, detail="Binaire introuvable.")
    return stream_file(file_path, request, name, inline=False)


@app.get("/store/admin", response_class=HTMLResponse)
async def admin_portal(request: Request):
    if is_admin(request):
        files = []
        for p in sorted(STORAGE_DIR.rglob("*"), key=os.path.getmtime, reverse=True):
            if not p.is_file() or p.name.endswith(".sha256"):
                continue
            rel = drive_rel(p)
            size_str = fmt_size(p.stat().st_size)
            digest = get_sha256(p) or "—"
            files.append(f"""
                <li style="display:flex; justify-content:space-between; gap:0.6rem; padding:0.6rem 0; border-bottom:1px solid #1f2937; align-items:center; flex-wrap:wrap;">
                    <span style="overflow-wrap:anywhere;"><b>{rel}</b> ({size_str})<br><span style="font-size:0.75rem; color:#64748b; font-family:monospace;">{digest}</span></span>
                    <span style="display:flex; gap:0.4rem;">
                    <form action="/store/admin/rehash" method="post" style="margin:0;">
                        <input type="hidden" name="filename" value="{rel}">
                        <button type="submit" style="background:#0284c7; color:#fff; border:none; border-radius:4px; padding:0.3rem 0.6rem; cursor:pointer;">SHA256</button>
                    </form>
                    <form action="/store/admin/delete" method="post" style="margin:0;" onsubmit="return confirm('Confirmer la suppression de {rel} ?');">
                        <input type="hidden" name="filename" value="{rel}">
                        <button type="submit" style="background:#ef4444; color:#fff; border:none; border-radius:4px; padding:0.3rem 0.6rem; cursor:pointer;">Supprimer</button>
                    </form>
                    </span>
                </li>
                """)
        folder_opts = "".join(
            f'<option value="{f}">{f}</option>' for f in sorted(UPLOAD_FOLDERS))

        return HTMLResponse(f"""<!DOCTYPE html>
        <html lang="fr">
        <head>
            <meta charset="UTF-8">
            <title>HiTech Console // Admin</title>
            <meta name="viewport" content="width=device-width, initial-scale=1.0">
        </head>
        <body style="font-family:system-ui, sans-serif; background:#090d16; color:#e2e8f0; margin:0; padding:2rem;">
            <div style="max-width:800px; margin:auto; background:#111827; border:1px solid #1f2937; border-radius:8px; padding:2rem;">
                <div style="display:flex; justify-content:space-between; align-items:center; margin-bottom:1.5rem;">
                    <h2 style="margin:0; color:#38bdf8;">Console d'Administration</h2>
                    <span><a href="/store" style="color:#38bdf8; text-decoration:none; font-size:0.9rem; margin-right:1rem;">Drive</a><a href="/store/admin/logout" style="color:#ef4444; text-decoration:none; font-size:0.9rem;">Déconnexion</a></span>
                </div>
                <div style="background:#1f2937; border-radius:6px; padding:1.2rem; margin-bottom:1.5rem;">
                    <h3 style="margin-top:0;">Téléverser un nouvel artefact (Jusqu'à 15 Go)</h3>
                    <form action="/store/admin/upload" method="post" enctype="multipart/form-data">
                        <select name="folder" style="display:block; margin:0.6rem 0; padding:0.5rem; border-radius:4px; background:#0f172a; color:#fff; border:1px solid #475569;">{folder_opts}</select>
                        <input type="file" name="file" required style="display:block; margin:1rem 0; color:#cbd5e1;">
                        <button type="submit" style="background:#16a34a; color:#fff; border:none; padding:0.6rem 1.4rem; border-radius:4px; font-weight:bold; cursor:pointer;">Lancer le transfert</button>
                    </form>
                </div>
                <h3>Fichiers hébergés :</h3>
                <ul style="list-style:none; padding:0; margin:0;">{''.join(files) if files else '<li style="color:#64748b;">Aucun fichier stocké.</li>'}</ul>
            </div>
        </body>
        </html>""")

    return HTMLResponse("""<!DOCTYPE html>
    <html lang="fr">
    <head>
        <meta charset="UTF-8">
        <title>Identification Requise</title>
        <meta name="viewport" content="width=device-width, initial-scale=1.0">
    </head>
    <body style="font-family:system-ui, sans-serif; background:#0f172a; color:#f8fafc; height:100vh; display:flex; align-items:center; justify-content:center; margin:0;">
        <div style="background:#1e293b; border:1px solid #334155; border-radius:8px; padding:2rem; width:100%; max-width:360px;">
            <h3 style="margin-top:0; color:#38bdf8;">Authentification Admin</h3>
            <button onclick="quickOtp(this)" style="width:100%; background:#0284c7; color:#fff; border:none; padding:0.6rem; border-radius:4px; font-weight:bold; cursor:pointer; margin-bottom:1.5rem;">Recevoir le PIN en 1 clic</button>
            <div style="border-top:1px solid #334155; padding-top:1.2rem;">
                <form action="/store/admin/verify-otp" method="post">
                    <label style="display:block; font-size:0.85rem; margin-bottom:0.4rem; color:#94a3b8;">Code PIN (6 chiffres) :</label>
                    <input type="text" name="pin" maxlength="6" pattern="[0-9]{6}" placeholder="ex: 492018" required style="width:100%; box-sizing:border-box; padding:0.6rem; border-radius:4px; border:1px solid #475569; background:#0f172a; color:#fff; margin-bottom:0.6rem; font-size:1.1rem; letter-spacing:2px; text-align:center;">
                    <button type="submit" style="width:100%; background:#16a34a; color:#fff; border:none; padding:0.6rem; border-radius:4px; font-weight:bold; cursor:pointer;">Déverrouiller la Console</button>
                </form>
            </div>
        </div>
        <script>
        async function quickOtp(btn){
            btn.disabled = true; btn.textContent = 'Envoi en cours…';
            const r = await fetch('/api/auth/quick-otp', {method:'POST'});
            if (r.ok) alert('Code PIN transmis a ton adresse email.');
            else alert('Erreur: ' + (await r.text()));
            btn.disabled = false; btn.textContent = 'Recevoir le PIN en 1 clic';
        }
        </script>
    </body>
    </html>""")


@app.post("/store/admin/request-otp")
async def request_otp(email: str = Form(...)):
    if email.strip().lower() != ADMIN_EMAIL:
        log.warning("OTP refused for unauthorized email attempt")
        raise HTTPException(status_code=status.HTTP_403_FORBIDDEN, detail="Accès refusé.")
    pin = issue_otp()
    emailed = deliver_pin_or_fallback(pin)
    if not emailed:
        return HTMLResponse("<script>alert('SMTP indisponible — code PIN de secours écrit dans les logs du service (mode DEV).'); window.location.href='/store/admin';</script>")
    log.info("OTP PIN dispatched to admin email")
    return HTMLResponse("<script>alert('Code PIN transmis a ton adresse email.'); window.location.href='/store/admin';</script>")


@app.post("/store/admin/verify-otp")
async def verify_otp(pin: str = Form(...)):
    consume_otp(pin)
    token = make_session_token()
    res = RedirectResponse(url="/store/admin", status_code=status.HTTP_303_SEE_OTHER)
    set_session_cookie(res, token)
    log.info("Admin session established (12h JWT)")
    return res


@app.post("/store/admin/upload")
async def upload_asset(request: Request, folder: str = Form("public"),
                       file: UploadFile = File(...)):
    if not is_admin(request):
        raise HTTPException(status_code=403, detail="Session expirée.")
    folder = (folder or "public").strip().strip("/")
    if folder not in UPLOAD_FOLDERS:
        raise HTTPException(status_code=400, detail="Dossier cible invalide.")
    name = safe_name(file.filename or "unnamed")
    dest_dir = resolve_drive_path(folder)
    dest_dir.mkdir(parents=True, exist_ok=True)
    target_path = dest_dir / name
    if target_path.resolve().parent != dest_dir.resolve():
        raise HTTPException(status_code=403, detail="Accès interdit.")
    sha = hashlib.sha256()
    total = 0
    with open(target_path, "wb") as buffer:
        while chunk := await file.read(1024 * 1024 * 10):
            total += len(chunk)
            if total > MAX_UPLOAD_BYTES:
                buffer.close()
                target_path.unlink(missing_ok=True)
                raise HTTPException(status_code=413, detail="Fichier trop volumineux (max 15 Go).")
            buffer.write(chunk)
            sha.update(chunk)
    sha_sidecar(target_path).write_text(sha.hexdigest() + f"  {name}\n")
    log.info(f"Uploaded {folder}/{name} ({total} bytes)")
    return RedirectResponse(url="/store/admin", status_code=status.HTTP_303_SEE_OTHER)


@app.post("/store/admin/rehash")
async def rehash_asset(request: Request, filename: str = Form(...)):
    if not is_admin(request):
        raise HTTPException(status_code=403, detail="Session expirée.")
    target_path = resolve_drive_path(filename)
    if not target_path.exists() or not target_path.is_file():
        raise HTTPException(status_code=404, detail="Fichier introuvable.")
    digest = calculate_sha256(target_path)
    sha_sidecar(target_path).write_text(digest + f"  {target_path.name}\n")
    log.info(f"Rehashed {drive_rel(target_path)}")
    return RedirectResponse(url="/store/admin", status_code=status.HTTP_303_SEE_OTHER)


@app.post("/store/admin/delete")
async def delete_asset(request: Request, filename: str = Form(...)):
    if not is_admin(request):
        raise HTTPException(status_code=403, detail="Session expirée.")
    target_path = resolve_drive_path(filename)
    if target_path.exists() and target_path.is_file():
        rel = drive_rel(target_path)
        target_path.unlink()
        sidecar = sha_sidecar(target_path)
        if sidecar.exists():
            sidecar.unlink()
        log.info(f"Deleted {rel}")
    return RedirectResponse(url="/store/admin", status_code=status.HTTP_303_SEE_OTHER)


@app.get("/store/admin/logout")
async def admin_logout():
    res = RedirectResponse(url="/store", status_code=status.HTTP_303_SEE_OTHER)
    res.delete_cookie("hitech_admin_token")
    return res
