"""Test bootstrap for the HiTech-Store FastAPI app.

The module reads its configuration from the environment at import time, so the
isolation variables (storage root, shared secret, session key) must be set
before ``app`` is imported. This conftest runs first during collection.
"""
from __future__ import annotations

import atexit
import os
import shutil
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

_STORAGE = tempfile.mkdtemp(prefix="hitech-store-test-")
atexit.register(shutil.rmtree, _STORAGE, True)

os.environ["STORAGE_DIR"] = _STORAGE
os.environ["SECRET_KEY"] = "test-secret-key-not-for-production"
os.environ["AGENT_SHARED_SECRET"] = "test-agent-secret"
os.environ["DEV_OTP_FALLBACK"] = "1"
os.environ["COOKIE_SECURE"] = "0"
os.environ.pop("GITHUB_CLIENT_ID", None)
os.environ.pop("GITHUB_CLIENT_SECRET", None)
os.environ.pop("GITHUB_ALLOWED_LOGIN", None)
