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

class PrivateValidation(unittest.TestCase):
    @unittest.skipUnless((Path(__file__).resolve().parents[1] / 'private/reports/mx-map-staging/map-packages.json').exists(), 'run private packaging first')
    def test_all_private_packages(self):
        import json
        import hashlib
        from package_content import Resource
        root = Path(__file__).resolve().parents[1]
        output = root / 'private/reports/mx-map-staging'
        report = json.loads((output / 'map-packages.json').read_text())
        self.assertEqual(len(report), 56)
        for row in report:
            data = (output / (row['sha256']+'.bin')).read_bytes()
            self.assertEqual(len(data), row['bytes'])
            self.assertEqual(hashlib.sha256(data).hexdigest(), row['sha256'])
            resource = Resource(data)
            self.assertEqual(len(resource.members), row['member_count'])
            self.assertTrue(any(n.lower().endswith('.ps2.trn') for n,_,_ in resource.members))
            self.assertFalse(any(n.lower().endswith(('.vuo','.irx','.elf','.iso')) for n,_,_ in resource.members))
            for i, (_,_,size) in enumerate(resource.members):
                self.assertEqual(len(resource.extract(i)), size)

class ManifestTests(unittest.TestCase):
    def test_additive_atomic_update(self):
        import tempfile, json, hashlib
        from package_mx_maps import integrate_manifest
        with tempfile.TemporaryDirectory() as temporary:
            output=Path(temporary)
            blob=b'test'
            digest=hashlib.sha256(blob).hexdigest()
            (output/(digest+'.bin')).write_bytes(blob)
            existing=dict(path='/MX/BIKE50.RES',lba=10,bytes=4)
            (output/'manifest.json').write_text(json.dumps({'files':[existing]}))
            row=dict(archive='/MX/MAPS/FS01.RES',sha256=digest,bytes=4,member_count=1)
            integrate_manifest(output,[row])
            integrate_manifest(output,[row])
            result=json.loads((output/'manifest.json').read_text())['files']
            self.assertEqual(result[0],existing)
            self.assertEqual(len(result),2)
            self.assertGreater(result[1]['lba'],10)
            self.assertFalse((output/'manifest.json.tmp').exists())
