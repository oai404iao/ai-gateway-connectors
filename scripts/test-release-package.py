#!/usr/bin/env python3
"""Offline regressions for the official runtime's native ABI floor."""

import importlib.util
from pathlib import Path
import unittest


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


if __name__ == "__main__":
    unittest.main()
