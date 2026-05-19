#!/usr/bin/env python3

import argparse
from pathlib import Path
import subprocess
import sys


SCENARIOS = {
    "divergence": "01_no_sync_divergence.py",
    "convergence": "02_mining_convergence.py",
    "reorg": "03_longer_chain_reorg.py",
    "invalid": "04_invalid_data_rejection.py",
    "orphan": "05_orphan_block_recovery.py",
    "partition": "06_partition_failure.py",
    "overload": "07_overload_failure.py",
}


def main():
    parser = argparse.ArgumentParser(description="Run one of the mined-ledger scenario scripts.")
    parser.add_argument("scenario", nargs="?", choices=SCENARIOS.keys(), default="convergence")
    parser.add_argument("scenario_args", nargs=argparse.REMAINDER)
    args = parser.parse_args()

    script = Path(__file__).resolve().parent / "scenarios" / SCENARIOS[args.scenario]
    command = [sys.executable, str(script), *args.scenario_args]
    raise SystemExit(subprocess.call(command))


if __name__ == "__main__":
    main()
