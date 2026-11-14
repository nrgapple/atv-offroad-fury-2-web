import sys
import unittest
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'tools'))
from package_mx_maps import VENUES, package

class PackagingTests(unittest.TestCase):
    def test_catalog_identity(self):
        self.assertEqual(len(VENUES), 56)
        self.assertEqual(len(set(v.lower() for v in VENUES)), 56)

    def test_reject_public_output(self):
        with self.assertRaises(ValueError):
            package(Path('/tmp/mx-public-output'))

if __name__ == '__main__':
    unittest.main()
