#!/usr/bin/env python3
"""Build and verify trusted native connector release archives."""

import ctypes
import hashlib
import json
import pathlib
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
LIBRARY = "libai_gateway_connector_codex.so"
TARGET_MACHINES = {"x86_64-unknown-linux-gnu": 62, "aarch64-unknown-linux-gnu": 183}


class ByteSlice(ctypes.Structure):
    _fields_ = [("ptr", ctypes.c_void_p), ("length", ctypes.c_uint64)]


class Descriptor(ctypes.Structure):
    _fields_ = [
        ("abi_version", ctypes.c_uint32),
        ("struct_size", ctypes.c_uint32),
        ("manifest", ByteSlice),
        ("dispatch", ctypes.c_void_p),
        ("free_buffer", ctypes.c_void_p),
    ]


def sha256(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def inspect_library(path, version, target):
    header = path.read_bytes()[:20]
    if (
        len(header) != 20
        or header[:6] != b"\x7fELF\x02\x01"
        or int.from_bytes(header[18:20], "little") != TARGET_MACHINES[target]
    ):
        raise ValueError("library is not a supported 64-bit little-endian ELF target")
    library = ctypes.CDLL(str(path.resolve()))
    entry = library.ai_gateway_connector_entry_v1
    entry.restype = ctypes.POINTER(Descriptor)
    pointer = entry()
    if not pointer:
        raise ValueError("plugin entry returned null")
    descriptor = pointer.contents
    if descriptor.abi_version != 1 or descriptor.struct_size != ctypes.sizeof(Descriptor):
        raise ValueError("incompatible native ABI")
    if (
        not descriptor.dispatch
        or not descriptor.free_buffer
        or not descriptor.manifest.ptr
        or not 0 < descriptor.manifest.length <= 65536
    ):
        raise ValueError("invalid descriptor fields")
    manifest = json.loads(ctypes.string_at(descriptor.manifest.ptr, descriptor.manifest.length))
    if manifest["id"] != "codex" or manifest["version"] != version:
        raise ValueError("exported plugin identity/version mismatch")
    for field in ("operations", "commands"):
        values = manifest[field]
        if not values or len(values) != len(set(values)):
            raise ValueError(f"invalid manifest {field}")
    return manifest


def dependency_licenses(stage, metadata):
    notices = [
        "# Third-party notices",
        "",
        "Generated from the locked Cargo dependency graph. These license texts",
        "and notices accompany binary redistribution; see LICENSE for project terms.",
        "",
    ]
    sdk_source = None
    for package in sorted(metadata["packages"], key=lambda item: (item["name"], item["version"])):
        if package["id"] in metadata["workspace_members"]:
            continue
        name = f'{package["name"]}-{package["version"]}'
        source = pathlib.Path(package["manifest_path"]).parent
        destination = stage / "LICENSES" / name
        destination.mkdir(parents=True)
        texts = [
            path
            for path in source.iterdir()
            if path.name.lower().startswith(("license", "licence", "copying", "notice"))
        ]
        if package.get("license_file"):
            declared = source / package["license_file"]
            if declared not in texts:
                texts.append(declared)
        if not texts and package.get("license") == "AGPL-3.0-only":
            texts = [ROOT / "LICENSE"]
        if not texts:
            raise ValueError(f"missing dependency license material: {name}")
        for text in texts:
            if text.is_dir():
                shutil.copytree(text, destination / text.name, dirs_exist_ok=True)
            else:
                shutil.copyfile(text, destination / text.name)
        notices.extend(
            [
                f"## {name}",
                "",
                f"License expression: `{package.get('license') or 'see license files'}`",
                "",
                f"License material: `LICENSES/{name}/`",
                "",
            ]
        )
        if package["name"] == "ai-gateway-connector-sdk":
            sdk_source = package["source"] or "development-path"
    if sdk_source is None:
        raise ValueError("locked dependency graph has no connector SDK")
    (stage / "THIRD_PARTY_NOTICES.md").write_text("\n".join(notices))
    return sdk_source


def build(version, target):
    if target not in TARGET_MACHINES or not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise ValueError("invalid version or target")
    output = ROOT / "target" / "release-package"
    output.mkdir(parents=True, exist_ok=True)
    source = ROOT / "target" / target / "release" / LIBRARY
    manifest = inspect_library(source, version, target)
    metadata = json.loads(
        subprocess.check_output(["cargo", "metadata", "--locked", "--format-version", "1"], cwd=ROOT)
    )
    name = f"ai-gateway-connector-codex-{version}-{target}"
    archive = output / f"{name}.tar.gz"
    with tempfile.TemporaryDirectory(prefix=".package-", dir=output) as temporary:
        stage = pathlib.Path(temporary) / name
        stage.mkdir()
        shutil.copyfile(source, stage / LIBRARY)
        (stage / LIBRARY).chmod(0o444)
        for filename in ("LICENSE", "README.md", "CHANGELOG.md"):
            shutil.copyfile(ROOT / filename, stage / filename)
        shutil.copytree(ROOT / "docs", stage / "docs")
        sdk_source = dependency_licenses(stage, metadata)
        revision = subprocess.run(
            ["git", "rev-parse", "--verify", "HEAD"], cwd=ROOT, capture_output=True, text=True
        )
        write_json(stage / "manifest.json", manifest)
        write_json(
            stage / "build-info.json",
            {
                "schema_version": 1,
                "connector_abi": 1,
                "connector_version": version,
                "target": target,
                "library": LIBRARY,
                "library_sha256": sha256(stage / LIBRARY),
                "source_repository": "https://github.com/oai404iao/ai-gateway-connectors",
                "source_revision": revision.stdout.strip() if revision.returncode == 0 else "uncommitted",
                "sdk_source": sdk_source,
            },
        )
        files = sorted(path for path in stage.rglob("*") if path.is_file())
        (stage / "SHA256SUMS").write_text(
            "".join(f"{sha256(path)}  {path.relative_to(stage).as_posix()}\n" for path in files)
        )
        with tarfile.open(archive, "w:gz") as destination:
            destination.add(stage, arcname=name)
    archive.with_suffix(archive.suffix + ".sha256").write_text(f"{sha256(archive)}  {archive.name}\n")
    verify(archive)
    print(archive)


def verify(archive):
    archive = archive.resolve()
    checksum = archive.with_suffix(archive.suffix + ".sha256").read_text()
    if checksum != f"{sha256(archive)}  {archive.name}\n":
        raise ValueError("archive checksum mismatch")
    with tempfile.TemporaryDirectory(prefix=".verify-", dir=archive.parent) as temporary:
        extraction = pathlib.Path(temporary)
        with tarfile.open(archive, "r:gz") as source:
            members = source.getmembers()
            names = [item.name for item in members]
            if len(names) != len(set(names)) or any(
                not (item.isfile() or item.isdir()) for item in members
            ):
                raise ValueError("archive contains duplicate or nonregular entries")
            source.extractall(extraction, filter="data")
        roots = list(extraction.iterdir())
        if len(roots) != 1 or not roots[0].is_dir():
            raise ValueError("archive must contain a single package directory")
        stage = roots[0]
        files = {path.relative_to(stage).as_posix() for path in stage.rglob("*") if path.is_file()}
        required = {
            LIBRARY,
            "manifest.json",
            "build-info.json",
            "SHA256SUMS",
            "LICENSE",
            "THIRD_PARTY_NOTICES.md",
            "docs/deployment.md",
            "docs/releasing.md",
        }
        if not required.issubset(files) or not any(name.startswith("LICENSES/") for name in files):
            raise ValueError("missing release or license materials")
        checked = set()
        for line in (stage / "SHA256SUMS").read_text().splitlines():
            digest, filename = line.split("  ", 1)
            if filename not in files or filename in checked or sha256(stage / filename) != digest:
                raise ValueError(f"invalid package checksum: {filename}")
            checked.add(filename)
        if checked != files - {"SHA256SUMS"}:
            raise ValueError("incomplete checksum set")
        info = json.loads((stage / "build-info.json").read_text())
        if info["schema_version"] != 1 or info["connector_abi"] != 1:
            raise ValueError("unsupported build-info schema or ABI")
        if info["library"] != LIBRARY or info["library_sha256"] != sha256(stage / LIBRARY):
            raise ValueError("build-info library digest mismatch")
        manifest = inspect_library(stage / LIBRARY, info["connector_version"], info["target"])
        if manifest != json.loads((stage / "manifest.json").read_text()):
            raise ValueError("packaged manifest differs from exported descriptor")
    print(f"verified {archive.name}")


if __name__ == "__main__":
    try:
        if len(sys.argv) == 4 and sys.argv[1] == "build":
            build(sys.argv[2], sys.argv[3])
        elif len(sys.argv) == 3 and sys.argv[1] == "verify":
            verify(pathlib.Path(sys.argv[2]))
        else:
            raise ValueError("usage: release-package.py build VERSION TARGET | verify ARCHIVE")
    except (ValueError, KeyError, OSError, subprocess.CalledProcessError) as error:
        raise SystemExit(str(error)) from error
