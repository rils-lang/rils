from __future__ import annotations

import importlib.util
from pathlib import Path
import shutil
import sys
import tarfile
import unittest
import uuid
import zipfile


SCRIPT_PATH = Path(__file__).resolve().parents[1] / "package-rils.py"
SPEC = importlib.util.spec_from_file_location("package_rils", SCRIPT_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"Cannot load {SCRIPT_PATH}")
package_rils = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = package_rils
SPEC.loader.exec_module(package_rils)


class PackageRilsTests(unittest.TestCase):
    def create_test_root(self) -> Path:
        test_root = SCRIPT_PATH.parents[1] / "target" / "package-tests"
        test_root.mkdir(parents=True, exist_ok=True)
        root = test_root / uuid.uuid4().hex
        root.mkdir()
        self.addCleanup(shutil.rmtree, root)
        return root

    def test_resolve_target_accepts_supported_explicit_target(self) -> None:
        target = package_rils.resolve_target("x86_64-unknown-linux-gnu", None)
        self.assertEqual(target, "x86_64-unknown-linux-gnu")

    def test_resolve_target_rejects_unknown_explicit_target(self) -> None:
        with self.assertRaisesRegex(RuntimeError, "Unsupported package target"):
            package_rils.resolve_target("wasm32-unknown-unknown", None)

    def test_zip_and_checksum_keep_top_level_package_directory(self) -> None:
        root = self.create_test_root()
        staging_root = root / "rils-0.4.0-windows-x86_64"
        (staging_root / "bin").mkdir(parents=True)
        (staging_root / "bin" / "rils.exe").write_bytes(b"rils")

        archive_path = package_rils.create_archive(staging_root, root, "zip")
        checksum_path = package_rils.write_checksum(archive_path)

        with zipfile.ZipFile(archive_path) as archive:
            self.assertEqual(
                archive.namelist(),
                ["rils-0.4.0-windows-x86_64/bin/rils.exe"],
            )
        checksum = checksum_path.read_text(encoding="utf-8")
        self.assertTrue(checksum.endswith(f"  {archive_path.name}\n"))
        self.assertEqual(len(checksum.split()[0]), 64)

    def test_tar_archive_normalizes_release_metadata(self) -> None:
        root = self.create_test_root()
        staging_root = root / "rils-0.4.0-linux-x86_64"
        (staging_root / "bin").mkdir(parents=True)
        (staging_root / "bin" / "rils").write_bytes(b"rils")

        archive_path = package_rils.create_archive(staging_root, root, "tar.gz")

        with tarfile.open(archive_path, "r:gz") as archive:
            entry = archive.getmember("rils-0.4.0-linux-x86_64/bin/rils")
            self.assertEqual(entry.mtime, 0)
            self.assertEqual(entry.uid, 0)
            self.assertEqual(entry.gid, 0)


if __name__ == "__main__":
    unittest.main()
