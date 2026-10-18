import hashlib
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch
import zipfile


spec = importlib.util.spec_from_file_location(
    "fontin", Path(__file__).resolve().parents[1] / "setup-fontin.py"
)
fontin = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fontin)


class FontinTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.cache = Path(self.directory.name)
        self.font = b"fixture font"
        archive = io.BytesIO()
        with zipfile.ZipFile(archive, "w") as package:
            package.writestr(fontin.FONT, self.font)
        self.archive = archive.getvalue()
        self.fetch = Mock(return_value=self.archive)
        for name, data in [("FONT_SHA256", self.font), ("ARCHIVE_SHA256", self.archive)]:
            patched = patch.object(fontin, name, hashlib.sha256(data).hexdigest())
            patched.start()
            self.addCleanup(patched.stop)

    def test_verified_cache_needs_no_download(self):
        path = fontin.prepare(self.cache, self.fetch)
        self.assertEqual(path.read_bytes(), self.font)
        self.assertEqual(fontin.prepare(self.cache, self.fetch), path)
        self.fetch.assert_called_once()

    def test_corrupt_cache_is_replaced(self):
        (self.cache / fontin.FONT).write_bytes(b"corrupt")
        self.assertEqual(fontin.prepare(self.cache, self.fetch).read_bytes(), self.font)

    def test_changed_archive_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "checksum mismatch"):
            fontin.prepare(self.cache, lambda: b"changed upstream")
        self.assertFalse((self.cache / fontin.FONT).exists())

    def test_changed_member_is_rejected(self):
        with patch.object(fontin, "FONT_SHA256", "unexpected"):
            with self.assertRaisesRegex(ValueError, "checksum mismatch"):
                fontin.prepare(self.cache, self.fetch)
        self.assertFalse((self.cache / fontin.FONT).exists())


if __name__ == "__main__":
    unittest.main()
