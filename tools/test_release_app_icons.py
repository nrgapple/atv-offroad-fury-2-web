"""Original app icon pixels stay outside the authored CI artifact."""
import hashlib
import json
from pathlib import Path
import struct
import tempfile
import unittest

from app_icons import validated_icons
from package_release import STATIC_FILES, ROOT


class AppIconTests(unittest.TestCase):
    def fixture(self, directory):
        icons = []
        for size in [180, 192, 512]:
            data = b'\x89PNG\r\n\x1a\n' + struct.pack('>I', 13) + b'IHDR' + struct.pack('>II', size, size)
            digest = hashlib.sha256(data).hexdigest()
            (directory / (digest + '.png')).write_bytes(data)
            icons.append({'size': size, 'sha256': digest, 'url': f'/app-icons/{digest}.png', 'bytes': len(data)})
        return {'version': 1, 'icons': icons}

    def test_original_icon_references_match_manifest_and_html_but_pixels_are_not_packaged(self):
        metadata = json.loads((ROOT / 'web/app-icon-assets.json').read_text())
        icons = {item['size']: item for item in metadata['icons']}
        manifest = json.loads((ROOT / 'web/manifest.webmanifest').read_text())
        self.assertEqual([icon['src'] for icon in manifest['icons']], [icons[192]['url'], icons[512]['url']])
        for name in ['index.html', 'play.html']:
            self.assertIn(icons[180]['url'], (ROOT / 'web' / name).read_text())
        self.assertTrue(all(not icon['url'].lstrip('/') in STATIC_FILES for icon in icons.values()))
        self.assertNotIn('base64', json.dumps(metadata))

    def test_hash_mismatch_path_escape_and_symlink_are_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            metadata = self.fixture(directory)
            self.assertEqual(len(validated_icons(metadata, directory)), 3)
            bad = json.loads(json.dumps(metadata)); bad['icons'][0]['url'] = '/private/source.iso'
            with self.assertRaisesRegex(ValueError, 'unsafe'):
                validated_icons(bad, directory)
            icon = directory / (metadata['icons'][0]['sha256'] + '.png')
            icon.write_bytes(b'changed')
            with self.assertRaisesRegex(ValueError, 'checksum'):
                validated_icons(metadata, directory)
            icon.unlink(); icon.symlink_to(directory / (metadata['icons'][1]['sha256'] + '.png'))
            with self.assertRaisesRegex(ValueError, 'symlink'):
                validated_icons(metadata, directory)
