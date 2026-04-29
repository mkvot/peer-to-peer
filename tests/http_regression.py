#!/usr/bin/env python3
"""
Focused HTTP regression test.

This is intentionally smaller than test.py. It checks the prax2 baseline
HTTP fixes quickly:
- POST bodies larger than the old 4096-byte read buffer are accepted.
- /getdata/{hash} returns valid JSON with string content.
"""

import hashlib
import json
import subprocess
import tempfile
import time
import urllib.error
import urllib.request


BINARY = "./target/debug/peer-to-peer"
PORT = 9100
TIMEOUT = 3


def sha256(content: str) -> str:
    return hashlib.sha256(content.encode()).hexdigest()


def request(method: str, path: str, body=None):
    data = None
    headers = {}
    if body is not None:
        data = json.dumps(body).encode()
        headers["Content-Type"] = "application/json"

    req = urllib.request.Request(
        f"http://127.0.0.1:{PORT}{path}",
        data=data,
        headers=headers,
        method=method,
    )
    with urllib.request.urlopen(req, timeout=TIMEOUT) as response:
        raw = response.read().decode()
        return response.status, json.loads(raw) if raw else None


def wait_for_node(timeout=8):
    deadline = time.time() + timeout
    while time.time() < deadline:
        try:
            status, _ = request("GET", "/status")
            if status == 200:
                return
        except (urllib.error.URLError, TimeoutError):
            pass
        time.sleep(0.2)
    raise RuntimeError(f"node {PORT} did not start")


def main():
    content = "large block " + ("x" * 6000) + ' quote " slash \\ newline \n end'
    block_hash = sha256(content)

    with tempfile.NamedTemporaryFile("w", delete=False) as peers_file:
        json.dump([], peers_file)

    process = subprocess.Popen(
        [BINARY, str(PORT), peers_file.name],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        text=True,
    )

    try:
        wait_for_node()
        if process.poll() is not None:
            stderr = process.stderr.read() if process.stderr else ""
            raise RuntimeError(f"node exited early: {stderr.strip()}")

        status, response = request(
            "POST",
            "/block",
            {"hash": block_hash, "content": content},
        )
        assert status == 200, response

        _, stored_block = request("GET", f"/getdata/{block_hash}")
        assert stored_block == {"hash": block_hash, "content": content}

        print("tests/http_regression.py: passed")
    finally:
        process.terminate()
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            process.kill()


if __name__ == "__main__":
    main()
