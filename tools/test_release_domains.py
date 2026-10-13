"""CI stages a checked release without trying to reprovision existing domains."""
import json
import hashlib
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import stage_cloudflare
from test_release import COMMIT, fixture


class DomainStagingTests(unittest.TestCase):
    def test_safari_and_legacy_icon_aliases_are_bound_from_checked_manifest(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); release = root / 'release'; fixture(release)
            small, large = '/app-icons/' + '1' * 64 + '.png', '/app-icons/' + '2' * 64 + '.png'
            manifest = release / 'static/manifest.webmanifest'
            manifest.write_text(json.dumps({'icons': [{'src': small, 'sizes': '192x192'}, {'src': large, 'sizes': '512x512'}]}))
            metadata = json.loads((release / 'release.json').read_text())
            metadata['sha256']['static/manifest.webmanifest'] = hashlib.sha256(manifest.read_bytes()).hexdigest()
            (release / 'release.json').write_text(json.dumps(metadata))
            (root / 'wrangler.jsonc').write_text(json.dumps({'assets': {}}))
            with patch.object(stage_cloudflare, 'ROOT', root):
                stage_cloudflare.stage(release, COMMIT, preserve_custom_domains=True)
            config = json.loads((root / 'work/cloudflare/wrangler.jsonc').read_text())
            self.assertEqual(config['vars']['APPLE_TOUCH_ICON_KEY'], small.lstrip('/'))
            self.assertEqual(config['vars']['APP_ICON_512_KEY'], large.lstrip('/'))

    def test_authored_home_icon_aliases_use_static_release_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); release = root / 'release'; fixture(release)
            manifest = release / 'static/manifest.webmanifest'
            manifest.write_text(json.dumps({'icons': [{'src': '/fury-remastered-home-192-v1.png', 'sizes': '192x192'}, {'src': '/fury-remastered-home-512-v1.png', 'sizes': '512x512'}]}))
            metadata = json.loads((release / 'release.json').read_text())
            metadata['sha256']['static/manifest.webmanifest'] = hashlib.sha256(manifest.read_bytes()).hexdigest()
            (release / 'release.json').write_text(json.dumps(metadata))
            (root / 'wrangler.jsonc').write_text(json.dumps({'assets': {}}))
            with patch.object(stage_cloudflare, 'ROOT', root):
                stage_cloudflare.stage(release, COMMIT)
            config = json.loads((root / 'work/cloudflare/wrangler.jsonc').read_text())
            self.assertEqual(config['vars']['APPLE_TOUCH_ICON_KEY'], 'fury-remastered-home-180-v1.png')
            self.assertEqual(config['vars']['APP_ICON_512_KEY'], 'fury-remastered-home-512-v1.png')

    def test_checked_release_keeps_origins_and_preserves_remote_domain_bindings(self):
        for keep_standard_route in (False, True):
            with self.subTest(keep_standard_route=keep_standard_route), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                release = root / 'release'
                fixture(release)
                domains = [{'pattern': 'furyremastered.com', 'custom_domain': True}]
                routes = domains + (['legacy.example/*'] if keep_standard_route else [])
                source = {'observability': {'enabled': True, 'head_sampling_rate': 1, 'redact_query_string': True, 'logs': {'enabled': True, 'invocation_logs': False}}, 'routes': routes, 'assets': {}, 'vars': {
                    'GAME_ORIGINS': 'https://furyremastered.com,https://legacy.example'}}
                (root / 'wrangler.jsonc').write_text(json.dumps(source))
                with patch.object(stage_cloudflare, 'ROOT', root):
                    stage_cloudflare.stage(release, COMMIT, preserve_custom_domains=True)
                staged = json.loads((root / 'work/cloudflare/wrangler.jsonc').read_text())
                self.assertEqual(staged['vars']['GAME_ORIGINS'], source['vars']['GAME_ORIGINS'])
                self.assertEqual(staged['vars']['BUILD_COMMIT'], COMMIT)
                self.assertEqual(staged['observability'], source['observability'])
                self.assertEqual(staged.get('routes', []), ['legacy.example/*'] if keep_standard_route else [])
                self.assertEqual(json.loads((root / 'wrangler.jsonc').read_text()), source)
                self.assertTrue((root / 'work/cloudflare/relay/index_bg.wasm').is_file())
                self.assertTrue((root / 'work/cloudflare/static/play.html').is_file())
