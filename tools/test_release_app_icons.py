"""Authored icons ship atomically; historical recovered icons remain R2-only."""
import hashlib
import base64
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

    def test_authored_icon_references_and_bytes_match_svg_manifest_and_html(self):
        metadata = json.loads((ROOT / 'web/fury-app-icons.json').read_text())
        self.assertEqual(metadata['source_sha256'], hashlib.sha256((ROOT / 'web/fury-remastered.svg').read_bytes()).hexdigest())
        icons = {item['size']: item for item in metadata['icons']}
        self.assertEqual(set(icons), {180, 192, 512})
        manifest = json.loads((ROOT / 'web/manifest.webmanifest').read_text())
        maskable = {item['size']: item for item in metadata['maskable']}
        self.assertEqual([icon['src'] for icon in manifest['icons']], ['/' + icons[size]['file'] for size in [192, 512]] + ['/' + maskable[size]['file'] for size in [192, 512]])
        self.assertEqual([icon['purpose'] for icon in manifest['icons']], ['any', 'any', 'maskable', 'maskable'])
        for size, icon in maskable.items():
            data = base64.b64decode(icon['base64'], validate=True)
            self.assertEqual(hashlib.sha256(data).hexdigest(), icon['sha256'])
            self.assertEqual(struct.unpack('>II', data[16:24]), (size, size))
            self.assertIn(icon['file'], STATIC_FILES)
        favicon = base64.b64decode(metadata['favicon']['base64'], validate=True)
        self.assertEqual(hashlib.sha256(favicon).hexdigest(), metadata['favicon']['sha256'])
        self.assertEqual(favicon[:6], b'\0\0\1\0\3\0')
        self.assertIn('favicon.ico', STATIC_FILES)
        for name in ['index.html', 'play.html']:
            self.assertIn('/' + icons[180]['file'], (ROOT / 'web' / name).read_text())
        for size, icon in icons.items():
            data = base64.b64decode(icon['base64'], validate=True)
            self.assertEqual(hashlib.sha256(data).hexdigest(), icon['sha256'])
            self.assertEqual(data[:8], b'\x89PNG\r\n\x1a\n')
            self.assertEqual(struct.unpack('>II', data[16:24]), (size, size))
            self.assertIn(icon['file'], STATIC_FILES)
        original = json.loads((ROOT / 'web/app-icon-assets.json').read_text())
        self.assertTrue(all(icon['url'].lstrip('/') not in STATIC_FILES for icon in original['icons']))

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
