from __future__ import annotations

import os
import sys
from pathlib import Path

# Make `genio_server` importable without installing the package.
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

# The daemon builds its broker at import time; keep test state out of /var/lib.
os.environ.setdefault("GENIO_STATE_DIR", "/tmp/opencode/genio-test-state")
os.environ.setdefault("GENIO_LOG_LEVEL", "WARNING")
