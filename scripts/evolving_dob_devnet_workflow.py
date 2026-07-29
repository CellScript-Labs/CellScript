#!/usr/bin/env python3
"""Run the evolving-DOB proposal devnet workflow gate."""

from __future__ import annotations

import runpy
import sys
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "proposals/evolving-dob/evolving-dob-profile-v1/scripts/evolving_dob_devnet_workflow.py"


if __name__ == "__main__":
    sys.argv[0] = str(SCRIPT)
    runpy.run_path(str(SCRIPT), run_name="__main__")
