#!/usr/bin/env python3
"""Offline regressions for the official runtime's native ABI floor."""

import copy
import importlib.util
import io
import json
import os
from pathlib import Path
import stat
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location(
    "release_package", Path(__file__).with_name("release-package.py")
)
PACKAGE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PACKAGE)


class GlibcBaselineTests(unittest.TestCase):
    def test_bookworm_and_older_versions_are_accepted(self):
        PACKAGE.verify_glibc_requirements(
            "Name: GLIBC_2.2.5 Flags: none\nName: GLIBC_2.34 Flags: none\n"
            "Name: GLIBC_2.36 Flags: none\n"
        )

    def test_newer_missing_and_unknown_versions_fail_closed(self):
        for version_info in (
            "Name: GLIBC_2.38 Flags: none",
            "Name: GLIBC_2.36.1 Flags: none",
            "Name: GLIBC_3.0 Flags: none",
            "Name: GLIBC_PRIVATE Flags: none",
            "No version information found",
        ):
            with self.assertRaises(ValueError, msg=version_info):
                PACKAGE.verify_glibc_requirements(version_info)

    def test_command_protocol_rejects_incomplete_gateway_contracts(self):
        manifest = {
            "id": "codex", "version": "0.2.0", "protocol_version": 3,
            "operations": list(PACKAGE.OPERATION_PROTOCOLS),
            "commands": [
                "attempt.context", "settings.describe/v1",
                "settings.validate/v1", "settings.compile/v1",
                "attempt.describe/v1", "attempt.capabilities",
                "attempt.body", "attempt.target", "attempt.headers",
                "attempt.image_edit_plan", "attempt.image_part_plan",
                "endpoints", "authorize_url", "parse_callback", "parse_identity", "parse_expiration",
                "exchange_plan", "exchange_parse", "refresh_plan", "refresh_parse",
                "models_plan", "models_parse", "quota_plan", "quota_parse",
                "quota_reset_plan", "quota_reset_parse",
            ],
        }
        PACKAGE.verify_manifest(manifest, "0.2.0")
        for protocol in (None, 1, 2, 4):
            with self.assertRaises(ValueError):
                PACKAGE.verify_manifest({**manifest, "protocol_version": protocol}, "0.2.0")
        for command in manifest["commands"]:
            with self.subTest(missing=command), self.assertRaisesRegex(ValueError, command):
                PACKAGE.verify_manifest({
                    **manifest,
                    "commands": [value for value in manifest["commands"] if value != command],
                }, "0.2.0")
        for command in ["response.json/v1", "response.event/v1"]:
            with self.assertRaises(ValueError):
                PACKAGE.verify_manifest({
                    **manifest, "commands": [*manifest["commands"], command],
                }, "0.2.0")
        with self.assertRaises(ValueError):
            PACKAGE.verify_manifest({**manifest, "operations": ["responses"]}, "0.2.0")
        with self.assertRaises(ValueError):
            PACKAGE.verify_manifest({**manifest, "commands": ["attempt.context"]}, "0.2.0")

    def test_attempt_descriptor_requires_typed_passthrough_and_actual_upstream_usage(self):
        descriptor = {
            "capabilities": {
                "preserves_affinity_on_failure": True,
                "successful_response_is_sse": True,
                "changes_request_body": True,
            },
            "protocols": [{"protocol": "sse", "response": "passthrough"}],
            "usage": {"parser": "general", "format": "open_ai_responses"},
        }
        for operation, protocol in PACKAGE.OPERATION_PROTOCOLS.items():
            valid = copy.deepcopy(descriptor)
            valid["protocols"][0]["protocol"] = protocol
            PACKAGE.verify_attempt_descriptor(operation, valid)
        invalid_values = [
            {**descriptor, "extra": 1},
            {**descriptor, "usage": None},
            {**descriptor, "usage": {"parser": "plugin", "format": "open_ai_responses"}},
            {**descriptor, "usage": {"parser": "general", "format": "open_ai_images"}},
            {**descriptor, "protocols": [{"protocol": "non_stream", "response": "passthrough"}]},
            {**descriptor, "protocols": [{"protocol": "sse", "response": "sse"}]},
            {**descriptor, "protocols": descriptor["protocols"] * 2},
            {**descriptor, "capabilities": {"successful_response_is_sse": True}},
            {**descriptor, "capabilities": {**descriptor["capabilities"], "changes_request_body": 1}},
        ]
        for invalid in invalid_values:
            with self.subTest(value=invalid), self.assertRaises(ValueError):
                PACKAGE.verify_attempt_descriptor("responses", invalid)
        with self.assertRaises(ValueError):
            PACKAGE.verify_attempt_descriptor("unknown", descriptor)

    def test_build_uses_only_cdylib_reported_for_selected_package(self):
        target = "x86_64-unknown-linux-gnu"
        with tempfile.TemporaryDirectory() as tmp, patch.object(PACKAGE, "ROOT", Path(tmp)):
            stale = Path(tmp, "target", target, "release", PACKAGE.LIBRARY)
            stale.parent.mkdir(parents=True)
            stale.touch()
            selected = Path(tmp, "configured-build-dir", target, "release", PACKAGE.LIBRARY)
            artifact = {"reason": "compiler-artifact", "package_id": "codex-id",
                        "target": {"crate_types": ["cdylib", "rlib"]},
                        "filenames": [str(selected), str(selected.with_suffix(".rlib"))]}
            others = [{**artifact, "package_id": "dependency", "filenames": [str(stale)]},
                      {**artifact, "target": {"crate_types": ["rlib"]}},
                      {"reason": "build-finished", "success": True}]
            for matches in ([], [artifact], [artifact, artifact]):
                messages = "\n".join(map(json.dumps, others + matches))
                result = subprocess.CompletedProcess([], 0, stdout=messages)
                with self.subTest(count=len(matches)), patch.object(subprocess, "run", return_value=result) as run:
                    if len(matches) == 1:
                        self.assertEqual(PACKAGE.build_library(target, "codex-id"), selected)
                    else:
                        with self.assertRaisesRegex(ValueError, "exactly one Codex cdylib"):
                            PACKAGE.build_library(target, "codex-id")
                    self.assertEqual(run.call_args.args[0], ["cargo", "build", "--locked", "--release", "--package",
                                     "ai-gateway-connector-codex", "--lib", "--target", target, "--message-format=json"])


class SourceProvenanceTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        root_patch = patch.object(PACKAGE, "ROOT", self.root)
        root_patch.start()
        self.addCleanup(root_patch.stop)
        self.git("init", "-q")
        (self.root / "Cargo.toml").write_text('[workspace.package]\nversion = "1.2.3"\n')
        self.sdk = "git+https://example.invalid/sdk?rev=" + "1" * 40 + "#" + "1" * 40
        (self.root / "Cargo.lock").write_text(
            f'[[package]]\nname = "ai-gateway-connector-sdk"\nsource = "{self.sdk}"\n')
        self.git("add", "Cargo.toml", "Cargo.lock")
        self.git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
                 "-c", "commit.gpgsign=false", "commit", "-qm", "fixture version")
        self.head = self.git("rev-parse", "HEAD")

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.root, text=True).strip()

    def test_release_uses_clean_commit_dev_does_not_claim_it(self):
        self.assertEqual(PACKAGE.source_revision(False), self.head)
        untracked = self.root / "new.rs"
        untracked.touch()
        with self.assertRaisesRegex(ValueError, "clean checkout"):
            PACKAGE.source_revision(False)
        untracked.unlink()
        (self.root / "Cargo.toml").write_text('[workspace.package]\nversion = "1.2.4"\n')
        with self.assertRaisesRegex(ValueError, "clean checkout"):
            PACKAGE.source_revision(False)
        self.git("add", "Cargo.toml")
        with self.assertRaisesRegex(ValueError, "clean checkout"):
            PACKAGE.source_revision(False)
        self.assertEqual(PACKAGE.source_revision(True), "uncommitted")

    def test_verification_checks_version_and_locked_sdk_at_recorded_commit(self):
        info = {"source_repository": PACKAGE.SOURCE_REPOSITORY, "source_revision": self.head,
                "connector_version": "1.2.3", "sdk_source": self.sdk}
        PACKAGE.verify_provenance(info, False)
        (self.root / "Cargo.lock").write_text("version = 4\n")
        PACKAGE.verify_provenance(info, False)
        for source in (self.sdk.replace("1", "2"), "development-path", None):
            with self.subTest(source=source), self.assertRaisesRegex(ValueError, "SDK source"):
                PACKAGE.verify_provenance({**info, "sdk_source": source}, False)
        with self.assertRaisesRegex(ValueError, "version"):
            PACKAGE.verify_provenance({**info, "connector_version": "1.2.4"}, False)
        with self.assertRaisesRegex(ValueError, "not a commit"):
            PACKAGE.verify_provenance({**info, "source_revision": self.git("rev-parse", "HEAD^{tree}")}, False)
        with self.assertRaisesRegex(ValueError, "source repository"):
            PACKAGE.verify_provenance({**info, "source_repository": "other"}, False)
        for revision in (self.head[:7], "uncommitted", None):
            with self.subTest(revision=revision), self.assertRaisesRegex(ValueError, "full source commit"):
                PACKAGE.verify_provenance({**info, "source_revision": revision}, False)
        PACKAGE.verify_provenance({**info, "source_revision": "uncommitted"}, True)


class ArchiveExtractionTests(unittest.TestCase):
    def archive(self, root, entries):
        archive = root / "package.tar.gz"
        with tarfile.open(archive, "w:gz") as output:
            for name, kind, contents in entries:
                member = tarfile.TarInfo(name)
                member.type = kind
                member.mode = 0o4777
                member.uid = member.gid = 12345
                if kind in (tarfile.SYMTYPE, tarfile.LNKTYPE):
                    member.linkname = "../escaped"
                member.size = len(contents)
                output.addfile(member, io.BytesIO(contents))
        return archive

    def test_regular_files_and_directories_do_not_inherit_tar_metadata(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            destination = root / "output"
            destination.mkdir(mode=0o700)
            archive = self.archive(root, [
                ("package", tarfile.DIRTYPE, b""),
                ("package/docs", tarfile.DIRTYPE, b""),
                ("package/docs/readme", tarfile.REGTYPE, b"safe"),
            ])
            PACKAGE.extract_archive(archive, destination)
            result = destination / "package/docs/readme"
            self.assertEqual(result.read_bytes(), b"safe")
            self.assertEqual(stat.S_IMODE(result.stat().st_mode), 0o600)
            self.assertEqual(result.stat().st_uid, os.geteuid())
            self.assertEqual(stat.S_IMODE(result.parent.stat().st_mode), 0o700)

    def test_unsafe_noncanonical_duplicate_and_special_members_fail_closed(self):
        fixtures = [
            [("../escaped", tarfile.REGTYPE, b"x")],
            [("/escaped", tarfile.REGTYPE, b"x")],
            [("C:/escaped", tarfile.REGTYPE, b"x")],
            [(r"package\escaped", tarfile.REGTYPE, b"x")],
            [("package/./escaped", tarfile.REGTYPE, b"x")],
            [("package//escaped", tarfile.REGTYPE, b"x")],
            [("package/../escaped", tarfile.REGTYPE, b"x")],
            [("package/new\nline", tarfile.REGTYPE, b"x")],
            [("package/file", tarfile.REGTYPE, b"x"), ("package/file", tarfile.REGTYPE, b"y")],
            [("package", tarfile.DIRTYPE, b""), ("package/", tarfile.DIRTYPE, b"")],
            [("package/file", tarfile.REGTYPE, b"x"), ("package/./file", tarfile.REGTYPE, b"y")],
            [("package/link", tarfile.SYMTYPE, b"")],
            [("package/link", tarfile.LNKTYPE, b"")],
            [("package/device", tarfile.CHRTYPE, b"")],
            [("package/pipe", tarfile.FIFOTYPE, b"")],
        ]
        for entries in fixtures:
            with self.subTest(entries=entries), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                destination = root / "output"
                destination.mkdir(mode=0o700)
                archive = self.archive(root, entries)
                with self.assertRaises(ValueError):
                    PACKAGE.extract_archive(archive, destination)
                self.assertFalse((root / "escaped").exists())

    def test_member_count_and_file_size_limits(self):
        for limit, value, entries in [
            ("MAX_ARCHIVE_MEMBERS", 1, [
                ("package/a", tarfile.REGTYPE, b"x"), ("package/b", tarfile.REGTYPE, b"y"),
            ]),
            ("MAX_MEMBER_BYTES", 1, [("package/a", tarfile.REGTYPE, b"xx")]),
            ("MAX_ARCHIVE_BYTES", 1024, [("package/a", tarfile.REGTYPE, b"x" * 2048)]),
        ]:
            with self.subTest(limit=limit), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                destination = root / "output"
                destination.mkdir(mode=0o700)
                archive = self.archive(root, entries)
                with patch.object(PACKAGE, limit, value), self.assertRaises(ValueError):
                    PACKAGE.extract_archive(archive, destination)

    def test_decompressed_reads_are_bounded_even_for_extension_headers(self):
        source = io.BytesIO(b"x" * 256)
        with patch.object(PACKAGE, "MAX_ARCHIVE_BYTES", 128), self.assertRaises(ValueError):
            PACKAGE.BoundedArchiveReader(source).read(1024 * 1024)
        self.assertEqual(source.tell(), 129)

    def test_extraction_rejects_preexisting_destination_content(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            destination = root / "output"
            destination.mkdir()
            (destination / "preexisting").write_text("keep")
            archive = self.archive(root, [("package", tarfile.REGTYPE, b"x")])
            with self.assertRaises(ValueError):
                PACKAGE.extract_archive(archive, destination)
            self.assertEqual((destination / "preexisting").read_text(), "keep")


if __name__ == "__main__":
    unittest.main()
