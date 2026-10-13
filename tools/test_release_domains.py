"""CI stages a checked release without trying to reprovision existing domains."""
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import stage_cloudflare
from test_release import COMMIT, fixture


class DomainStagingTests(unittest.TestCase):
    def test_checked_release_keeps_origins_and_preserves_remote_domain_bindings(self):
        for keep_standard_route in (False, True):
            with self.subTest(keep_standard_route=keep_standard_route), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                release = root / 'release'
                fixture(release)
                domains = [{'pattern': 'furyremastered.com', 'custom_domain': True}]
                routes = domains + (['legacy.example/*'] if keep_standard_route else [])
                source = {'routes': routes, 'assets': {}, 'vars': {
                    'GAME_ORIGINS': 'https://furyremastered.com,https://legacy.example'}}
                (root / 'wrangler.jsonc').write_text(json.dumps(source))
                with patch.object(stage_cloudflare, 'ROOT', root):
                    stage_cloudflare.stage(release, COMMIT, preserve_custom_domains=True)
                staged = json.loads((root / 'work/cloudflare/wrangler.jsonc').read_text())
                self.assertEqual(staged['vars']['GAME_ORIGINS'], source['vars']['GAME_ORIGINS'])
                self.assertEqual(staged['vars']['BUILD_COMMIT'], COMMIT)
                self.assertEqual(staged.get('routes', []), ['legacy.example/*'] if keep_standard_route else [])
                self.assertEqual(json.loads((root / 'wrangler.jsonc').read_text()), source)
                self.assertTrue((root / 'work/cloudflare/relay/index_bg.wasm').is_file())
                self.assertTrue((root / 'work/cloudflare/static/play.html').is_file())
