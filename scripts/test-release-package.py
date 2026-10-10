#!/usr/bin/env python3
"""Offline regressions for the official runtime's native ABI floor."""

import copy
import importlib.util
import io
import os
from pathlib import Path
import stat
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

    def test_command_protocol_rejects_old_gateways_and_partial_settings(self):
        manifest = {
            "id": "codex", "version": "0.2.0", "protocol_version": 3,
            "operations": list(PACKAGE.OPERATION_PROTOCOLS),
            "commands": [
                "attempt.context", "settings.describe/v1",
                "settings.validate/v1", "settings.compile/v1",
                "attempt.describe/v1", "attempt.capabilities",
            ],
        }
        PACKAGE.verify_manifest(manifest, "0.2.0")
        for protocol in (None, 1, 2, 4):
            with self.assertRaises(ValueError):
                PACKAGE.verify_manifest({**manifest, "protocol_version": protocol}, "0.2.0")
        for command in manifest["commands"]:
            with self.assertRaises(ValueError):
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

    def test_packaging_honors_cargo_target_directory(self):
        with patch.dict(os.environ, {"CARGO_TARGET_DIR": "/private/build/target"}):
            self.assertEqual(PACKAGE.cargo_target_directory(), Path("/private/build/target"))
        with patch.dict(os.environ, {"CARGO_TARGET_DIR": "local-target"}):
            self.assertEqual(PACKAGE.cargo_target_directory(), PACKAGE.ROOT / "local-target")


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
