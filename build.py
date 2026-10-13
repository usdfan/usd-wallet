"""Build the two official LightSNS LSMOD3 packages from compiled WASI P2 components."""

from __future__ import annotations

import hashlib
import io
import json
import shutil
import struct
import subprocess
import sys
import tarfile
from pathlib import Path


ROOT = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT / ".build"))
try:
    import zstandard as zstd  # noqa: E402
except ImportError:
    zstd = None


def zstd_bytes(data: bytes, decompress: bool = False) -> bytes:
    if zstd is not None:
        if decompress:
            return zstd.ZstdDecompressor().decompress(data)
        return zstd.ZstdCompressor(level=19).compress(data)
    operation = "zstdDecompressSync" if decompress else "zstdCompressSync"
    script = (
        "const fs=require('fs'),z=require('zlib');"
        f"process.stdout.write(z.{operation}(fs.readFileSync(0)));"
    )
    return subprocess.run(["node", "-e", script], input=data, capture_output=True, check=True).stdout


def package(platform: str, wasm_name: str) -> None:
    folder = ROOT / platform
    manifest = json.loads((folder / "manifest.json").read_text(encoding="utf-8"))
    module_id = manifest["module"]["id"]
    assert manifest["runtime"]["component"] == "backend/module.wasm"

    (folder / "backend").mkdir(exist_ok=True)
    (folder / "frontend").mkdir(exist_ok=True)
    compiled_wasm = ROOT / "target" / "wasm32-wasip2" / "release" / wasm_name
    installed_wasm = folder / "backend" / "module.wasm"
    if compiled_wasm.is_file():
        shutil.copyfile(compiled_wasm, installed_wasm)
    elif not installed_wasm.is_file():
        raise FileNotFoundError(f"WASM component missing: {compiled_wasm}")
    for name in ("module.js", "module.css"):
        shutil.copyfile(ROOT / "shared" / "frontend" / name, folder / "frontend" / name)

    members = sorted([
        "backend",
        "frontend",
        "manifest.json",
        "backend/module.wasm",
        "frontend/module.css",
        "frontend/module.js",
    ])
    tar_buffer = io.BytesIO()
    with tarfile.open(fileobj=tar_buffer, mode="w", format=tarfile.USTAR_FORMAT) as archive:
        for name in members:
            if name in ("backend", "frontend"):
                info = tarfile.TarInfo(name)
                info.type = tarfile.DIRTYPE
                info.mode = 0o755
                info.mtime = 0
                archive.addfile(info)
                continue
            data = (folder / name).read_bytes()
            info = tarfile.TarInfo(name)
            info.size = len(data)
            info.mode = 0o644
            info.mtime = 0
            archive.addfile(info, io.BytesIO(data))

    compressed = zstd_bytes(tar_buffer.getvalue())
    envelope = b"LSMOD3\0\0" + struct.pack(">HHQ", 3, 0, len(compressed)) + hashlib.sha256(compressed).digest()
    output = ROOT / "dist" / f"{module_id}-{manifest['module']['version']}.lsmod"
    output.parent.mkdir(exist_ok=True)
    output.write_bytes(envelope + compressed)
    verify(output, members)
    print(f"{output.name}: {output.stat().st_size} bytes; SHA-256 {hashlib.sha256(output.read_bytes()).hexdigest()}")


def verify(path: Path, members: list[str]) -> None:
    data = path.read_bytes()
    assert data[:8] == b"LSMOD3\0\0"
    version, reserved, compressed_len = struct.unpack(">HHQ", data[8:20])
    assert (version, reserved, compressed_len) == (3, 0, len(data) - 52)
    assert data[20:52] == hashlib.sha256(data[52:]).digest()
    tar_data = zstd_bytes(data[52:], decompress=True)
    with tarfile.open(fileobj=io.BytesIO(tar_data), mode="r:") as archive:
        assert archive.getnames() == members
        wasm = archive.extractfile("backend/module.wasm").read()
        assert wasm[:8] == b"\0asm\x0d\0\x01\0", "WASI P2 component expected"


if __name__ == "__main__":
    package("pc", "usd_wallet_pc.wasm")
    package("mobile", "usd_wallet_mb.wasm")
