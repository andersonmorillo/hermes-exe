#!/usr/bin/env python3
from pathlib import Path

from ptt_tooling import _write_setup_config

_write_setup_config(
    stt_backend="whisperx",
    whisperx_model="small",
    release_dir=Path(__file__).resolve().parent.parent / "target" / "release",
)
