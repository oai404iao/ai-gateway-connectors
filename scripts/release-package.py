#!/usr/bin/env python3
"""Build and verify trusted native connector release archives."""

import ctypes
import gzip
import hashlib
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
SOURCE_REPOSITORY = "https://github.com/oai404iao/ai-gateway-connectors"
LIBRARY = "libai_gateway_connector_codex.so"
TARGET_MACHINES = {"x86_64-unknown-linux-gnu": 62, "aarch64-unknown-linux-gnu": 183}
GLIBC_BASELINE = (2, 36)
MAX_ARCHIVE_MEMBERS = 10_000
MAX_ARCHIVE_BYTES = 512 * 1024 * 1024
MAX_MEMBER_BYTES = 256 * 1024 * 1024
MAX_METADATA_BYTES = 1024 * 1024
OPERATION_PROTOCOLS = {
    "responses": "sse",
    "responses-ws": "websocket",
    "web_search": "non_stream",
    "images_generation": "non_stream",
    "images_edit": "non_stream",
}


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

class CallOutput(ctypes.Structure):
    _fields_ = [("metadata", ByteSlice), ("body", ByteSlice)]


def build_library(target, package_id):
    build = subprocess.run(
        ["cargo", "build", "--locked", "--release", "--package", "ai-gateway-connector-codex",
         "--lib", "--target", target, "--message-format=json"],
        cwd=ROOT, stdout=subprocess.PIPE, text=True,
    )
    libraries = []
    for line in build.stdout.splitlines():
        message = json.loads(line)
        if message["reason"] == "compiler-message":
            print(message["message"]["rendered"], file=sys.stderr, end="")
        if (message["reason"] == "compiler-artifact" and message["package_id"] == package_id
                and "cdylib" in message["target"]["crate_types"]):
            libraries.extend(pathlib.Path(filename) for filename in message["filenames"]
                             if pathlib.Path(filename).name == LIBRARY)
    build.check_returncode()
    if len(libraries) != 1:
        raise ValueError("Cargo did not report exactly one Codex cdylib artifact")
    return libraries[0]


def sha256(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def verify_glibc_requirements(version_info):
    names = set(re.findall(r"Name:\s+(GLIBC_\S+)", version_info))
    if not names:
        raise ValueError("library has no inspectable GNU libc version requirements")
    for name in names:
        suffix = name.removeprefix("GLIBC_")
        if not re.fullmatch(r"\d+\.\d+(?:\.\d+)?", suffix):
            raise ValueError("library requires an unsupported GNU libc ABI")
        version = tuple(int(part) for part in suffix.split("."))
        padded = (*version, *(0 for _ in range(3 - len(version))))
        if padded > (*GLIBC_BASELINE, 0):
            raise ValueError("library requires GNU libc newer than Debian bookworm (2.36)")


def inspect_library(path, version, target):
    header = path.read_bytes()[:20]
    if (
        len(header) != 20
        or header[:6] != b"\x7fELF\x02\x01"
        or int.from_bytes(header[18:20], "little") != TARGET_MACHINES[target]
    ):
        raise ValueError("library is not a supported 64-bit little-endian ELF target")
    verify_glibc_requirements(subprocess.check_output(
        ["readelf", "--version-info", "--wide", str(path)], text=True,
    ))
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
    verify_manifest(manifest, version)
    for operation in manifest["operations"]:
        verify_attempt_descriptor(operation, inspect_attempt_descriptor(descriptor, operation))
    return manifest


def inspect_attempt_descriptor(descriptor, operation):
    dispatch = ctypes.CFUNCTYPE(
        ctypes.c_uint32, ByteSlice, ByteSlice, ByteSlice, ctypes.POINTER(CallOutput),
    )(descriptor.dispatch)
    release = ctypes.CFUNCTYPE(None, ByteSlice)(descriptor.free_buffer)
    command = ctypes.create_string_buffer(b"attempt.describe/v1")
    encoded = json.dumps({"operation": operation}).encode()
    metadata = ctypes.create_string_buffer(encoded)
    output = CallOutput()
    status = dispatch(
        ByteSlice(ctypes.addressof(command), len(command.value)),
        ByteSlice(ctypes.addressof(metadata), len(encoded)),
        ByteSlice(None, 0),
        ctypes.byref(output),
    )
    buffers = (output.metadata, output.body)
    if any(
        buffer.length > MAX_METADATA_BYTES or bool(buffer.ptr) != bool(buffer.length)
        for buffer in buffers
    ) or (output.metadata.ptr and output.metadata.ptr == output.body.ptr):
        raise ValueError("invalid attempt descriptor allocation")
    try:
        if status != 0 or output.body.length:
            raise ValueError("attempt descriptor command failed")
        return json.loads(ctypes.string_at(output.metadata.ptr, output.metadata.length))
    finally:
        for buffer in buffers:
            if buffer.length:
                ctypes.memset(buffer.ptr, 0, buffer.length)
            release(buffer)


def verify_attempt_descriptor(operation, value):
    if not isinstance(value, dict) or set(value) != {"capabilities", "protocols", "usage"}:
        raise ValueError("invalid attempt descriptor shape")
    capabilities = value["capabilities"]
    required_flags = {
        "preserves_affinity_on_failure", "successful_response_is_sse", "changes_request_body",
    }
    if (
        not isinstance(capabilities, dict)
        or set(capabilities) != required_flags
        or any(type(flag) is not bool for flag in capabilities.values())
    ):
        raise ValueError("invalid attempt capability flags")
    protocol = OPERATION_PROTOCOLS.get(operation)
    if not protocol or value["protocols"] != [{"protocol": protocol, "response": "passthrough"}]:
        raise ValueError("invalid Codex transport or response mode")
    if value["usage"] != {"parser": "general", "format": "open_ai_responses"}:
        raise ValueError("invalid Codex upstream usage descriptor")
    if len(json.dumps(value).encode()) > MAX_METADATA_BYTES:
        raise ValueError("attempt descriptor exceeds metadata limit")


def verify_manifest(manifest, version):
    if manifest["id"] != "codex" or manifest["version"] != version:
        raise ValueError("exported plugin identity/version mismatch")
    if manifest.get("protocol_version") != 3:
        raise ValueError("Codex requires gateway command protocol version 3")
    for field in ("operations", "commands"):
        values = manifest[field]
        if not values or len(values) != len(set(values)):
            raise ValueError(f"invalid manifest {field}")
    if set(manifest["operations"]) != set(OPERATION_PROTOCOLS):
        raise ValueError("invalid Codex operation set")
    if any(command.startswith("response.") for command in manifest["commands"]):
        raise ValueError("Codex response adaptation is not supported")
    required = {
        "attempt.context", "attempt.describe/v1",
        "attempt.body", "attempt.target", "attempt.headers",
        "attempt.image_edit_plan", "attempt.image_part_plan",
        "settings.describe/v1", "settings.validate/v1", "settings.compile/v1",
        "endpoints", "authorize_url", "parse_callback", "parse_identity", "parse_expiration",
        "exchange_plan", "exchange_parse", "refresh_plan", "refresh_parse",
        "models_plan", "models_parse", "quota_plan", "quota_parse",
        "quota_reset_plan", "quota_reset_parse",
    }
    missing = required.difference(manifest["commands"])
    if missing:
        raise ValueError(f"plugin lacks required commands: {', '.join(sorted(missing))}")


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


def source_revision(development):
    if development:
        return "uncommitted"
    if subprocess.check_output(["git", "status", "--porcelain", "--untracked-files=all"], cwd=ROOT):
        raise ValueError("release packaging requires a clean checkout; use --development for local checks")
    return subprocess.check_output(["git", "rev-parse", "--verify", "HEAD^{commit}"], cwd=ROOT, text=True).strip()


def verify_provenance(info, development):
    if info.get("source_repository") != SOURCE_REPOSITORY:
        raise ValueError("unexpected source repository")
    revision = info.get("source_revision")
    if development and revision == "uncommitted":
        return
    if not isinstance(revision, str) or not re.fullmatch(r"[0-9a-f]{40}", revision):
        raise ValueError("release package requires a full source commit")
    if subprocess.check_output(["git", "cat-file", "-t", revision], cwd=ROOT, text=True).strip() != "commit":
        raise ValueError("source revision is not a commit")
    manifest = subprocess.check_output(["git", "show", f"{revision}:Cargo.toml"], cwd=ROOT, text=True)
    if tomllib.loads(manifest)["workspace"]["package"]["version"] != info["connector_version"]:
        raise ValueError("source commit version does not match packaged connector version")
    lock = subprocess.check_output(["git", "show", f"{revision}:Cargo.lock"], cwd=ROOT, text=True)
    sdk_sources = [p.get("source") for p in tomllib.loads(lock)["package"]
                   if p["name"] == "ai-gateway-connector-sdk"]
    sdk_source = info.get("sdk_source")
    if (not isinstance(sdk_source, str) or not re.fullmatch(r"git\+\S+#[a-f0-9]{40}", sdk_source)
            or sdk_sources != [sdk_source]):
        raise ValueError("SDK source does not match the pinned Git source in the commit's Cargo.lock")


def build(version, target, development):
    if target not in TARGET_MACHINES or not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise ValueError("invalid version or target")
    revision = source_revision(development)
    check_version = [ROOT / "scripts/check-release-version.sh", version]
    if not development:
        check_version.append("--require-pinned-sdk")
    subprocess.run(check_version, cwd=ROOT, check=True)
    metadata = json.loads(
        subprocess.check_output(["cargo", "metadata", "--locked", "--format-version", "1"], cwd=ROOT)
    )
    package_id, = [p["id"] for p in metadata["packages"] if p["id"] in metadata["workspace_members"]
                   and p["name"] == "ai-gateway-connector-codex"]
    source = build_library(target, package_id)
    target_directory = pathlib.Path(metadata["target_directory"])
    output = target_directory / ("development-package" if development else "release-package")
    output.mkdir(parents=True, exist_ok=True)
    name = f"ai-gateway-connector-codex-{version}-{target}"
    archive = output / f"{name}.tar.gz"
    with tempfile.TemporaryDirectory(prefix=".package-", dir=output) as temporary:
        stage = pathlib.Path(temporary) / name
        stage.mkdir()
        shutil.copyfile(source, stage / LIBRARY)
        (stage / LIBRARY).chmod(0o444)
        manifest = inspect_library(stage / LIBRARY, version, target)
        for filename in ("LICENSE", "README.md", "CHANGELOG.md"):
            shutil.copyfile(ROOT / filename, stage / filename)
        shutil.copytree(ROOT / "docs", stage / "docs")
        sdk_source = dependency_licenses(stage, metadata)
        if source_revision(development) != revision:
            raise ValueError("source commit changed during release build")
        write_json(stage / "manifest.json", manifest)
        write_json(
            stage / "build-info.json",
            {
                "schema_version": 1,
                "connector_abi": 1,
                "connector_version": version,
                "target": target,
                "glibc_baseline": ".".join(map(str, GLIBC_BASELINE)),
                "library": LIBRARY,
                "library_sha256": sha256(stage / LIBRARY),
                "source_repository": SOURCE_REPOSITORY,
                "source_revision": revision,
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
    verify(archive, development)
    print(archive)


class BoundedArchiveReader:
    def __init__(self, source):
        self.source = source
        self.read_bytes = 0

    def read(self, size):
        remaining = MAX_ARCHIVE_BYTES - self.read_bytes
        data = self.source.read(min(size, remaining + 1) if size >= 0 else remaining + 1)
        self.read_bytes += len(data)
        if self.read_bytes > MAX_ARCHIVE_BYTES:
            raise ValueError("archive exceeds the uncompressed size limit")
        return data


def extract_archive(archive, destination):
    if destination.is_symlink() or not destination.is_dir() or any(destination.iterdir()):
        raise ValueError("archive destination must be a fresh private directory")
    if archive.stat().st_size > MAX_ARCHIVE_BYTES:
        raise ValueError("archive exceeds the compressed size limit")
    seen = set()
    total_bytes = 0
    with gzip.open(archive, "rb") as expanded:
        with tarfile.open(fileobj=BoundedArchiveReader(expanded), mode="r|") as source:
            for member in source:
                if len(seen) >= MAX_ARCHIVE_MEMBERS:
                    raise ValueError("archive exceeds the member count limit")
                name = member.name
                path = pathlib.PurePosixPath(name)
                if (
                    not name or len(name) > 4096 or len(path.parts) > 32
                    or path.is_absolute() or pathlib.PureWindowsPath(name).drive
                    or "\\" in name or any(ord(char) < 32 or ord(char) == 127 for char in name)
                    or any(part in ("", ".", "..") for part in name.split("/"))
                    or path.as_posix() != name
                ):
                    raise ValueError("archive contains an unsafe or noncanonical path")
                normalized = path.as_posix()
                if normalized in seen:
                    raise ValueError("archive contains duplicate entries")
                seen.add(normalized)
                if member.type not in (tarfile.REGTYPE, tarfile.AREGTYPE, tarfile.DIRTYPE) \
                        or member.sparse is not None:
                    raise ValueError("archive contains nonregular entries")
                if not 0 <= member.size <= MAX_MEMBER_BYTES or (member.isdir() and member.size):
                    raise ValueError("archive member exceeds the size limit")
                total_bytes += member.size
                if total_bytes > MAX_ARCHIVE_BYTES:
                    raise ValueError("archive exceeds the total size limit")
                parent = destination
                for part in path.parts if member.isdir() else path.parts[:-1]:
                    parent /= part
                    parent.mkdir(mode=0o700, exist_ok=True)
                    if parent.is_symlink() or not parent.is_dir():
                        raise ValueError("archive entry conflicts with a directory")
                if member.isdir():
                    continue
                contents = source.extractfile(member)
                if contents is None:
                    raise ValueError("archive regular file has no contents")
                with contents, (destination / normalized).open("xb") as output:
                    os.fchmod(output.fileno(), 0o600)
                    remaining = member.size
                    while remaining:
                        chunk = contents.read(min(remaining, 1024 * 1024))
                        if not chunk:
                            raise ValueError("archive member is truncated")
                        output.write(chunk)
                        remaining -= len(chunk)


def verify(archive, development):
    archive = archive.resolve()
    checksum = archive.with_suffix(archive.suffix + ".sha256").read_text()
    if checksum != f"{sha256(archive)}  {archive.name}\n":
        raise ValueError("archive checksum mismatch")
    with tempfile.TemporaryDirectory(prefix=".verify-", dir=archive.parent) as temporary:
        extraction = pathlib.Path(temporary)
        extract_archive(archive, extraction)
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
        verify_provenance(info, development)
        if info["schema_version"] != 1 or info["connector_abi"] != 1:
            raise ValueError("unsupported build-info schema or ABI")
        if info["glibc_baseline"] != ".".join(map(str, GLIBC_BASELINE)):
            raise ValueError("unsupported GNU libc baseline")
        if info["library"] != LIBRARY or info["library_sha256"] != sha256(stage / LIBRARY):
            raise ValueError("build-info library digest mismatch")
        manifest = inspect_library(stage / LIBRARY, info["connector_version"], info["target"])
        if manifest != json.loads((stage / "manifest.json").read_text()):
            raise ValueError("packaged manifest differs from exported descriptor")
    print(f"verified {archive.name}")


if __name__ == "__main__":
    try:
        args = sys.argv[1:]
        development = args[-1:] == ["--development"]
        if development:
            args.pop()
        if len(args) == 3 and args[0] == "build":
            build(args[1], args[2], development)
        elif len(args) == 2 and args[0] == "verify":
            verify(pathlib.Path(args[1]), development)
        else:
            raise ValueError("usage: release-package.py (build VERSION TARGET | verify ARCHIVE) [--development]")
    except (ValueError, KeyError, OSError, tarfile.TarError, subprocess.CalledProcessError) as error:
        raise SystemExit(str(error)) from error
