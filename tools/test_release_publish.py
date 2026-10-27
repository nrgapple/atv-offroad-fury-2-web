"""R2 staging cannot change live pointers or deploy unverified dependency bytes."""
from contextlib import nullcontext
import io
import json
import hashlib
import struct
import zlib
from pathlib import Path
import tempfile
import unittest
from urllib.parse import unquote
from unittest.mock import patch
from types import SimpleNamespace

import upload_cloudflare_content as uploader
from package_release import scoped_content_source
from test_fixtures import content


class Response(io.BytesIO):
    status = 200


class PublicationTests(unittest.TestCase):
    def test_browser_manifest_is_pinned_and_unexpected_source_fails_closed(self):
        commit = 'a' * 40
        source = "const response = await fetch('/content/manifest.json');"
        self.assertIn(f'/content/manifests/{commit}.json', scoped_content_source(source, commit))
        with self.assertRaises(ValueError):
            scoped_content_source(source + source, commit)

    def upload_fixture(self, root, corrupt=False):
        commit = 'a' * 40
        content(root / 'private/reports/hosted-content')
        icon_dir = root / 'private/reports/app-icons'
        icon_dir.mkdir(parents=True)
        icons = []
        def chunk(kind, data):
            return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
        for size in [180, 192, 512]:
            png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', size, size, 8, 2, 0, 0, 0)) + chunk(b'IDAT', zlib.compress((b'\0' + b'\x99\x10\x10' * size) * size)) + chunk(b'IEND', b'')
            digest = hashlib.sha256(png).hexdigest()
            (icon_dir / (digest + '.png')).write_bytes(png)
            icons.append({'size': size, 'sha256': digest, 'url': f'/app-icons/{digest}.png', 'bytes': len(png)})
        auth = root / 'Library/Preferences/.wrangler/config/default.toml'
        auth.parent.mkdir(parents=True)
        auth.write_text('oauth_token = "fixture-token"\n')
        objects = {}
        def network(request, timeout):
            key = unquote(request.full_url.split('/objects/')[1])
            if request.get_method() == 'PUT':
                objects[key] = request.data
                return Response(b'')
            data = objects[key]
            if corrupt and key.endswith('.bin'):
                data += b'corrupt'
            return Response(data)
        def command(args, **kwargs):
            return SimpleNamespace(stdout=json.dumps({'version': 1, 'icons': icons} if args[-1].endswith(':web/app-icon-assets.json') else {'entries': []}) if args[0] == 'git' else commit)
        with patch.object(uploader, 'ROOT', root), patch.object(uploader.Path, 'home', return_value=root), patch.object(uploader, 'validate_release'), patch.object(uploader, 'require_deployed_release'), patch.object(uploader, 'read_devlog', return_value={'entries': []}), patch.object(uploader.subprocess, 'run', side_effect=command), patch.object(uploader, 'urlopen', side_effect=network):
            uploader.upload(root / 'release', commit, release_scoped=True)
        return objects

    def test_content_first_requires_an_additive_manifest(self):
        live = json.dumps({'files': [{'path': '/A.RES'}, {'path': '/MX/BIKE125.RES'}]}).encode()
        uploader.require_additive(json.dumps({'files': [{'path': '/A.RES'}, {'path': '/MX/BIKE125.RES'},
                                                         {'path': '/NEW.RES'}]}).encode(), live)
        with self.assertRaisesRegex(ValueError, 'remove live runtime'):
            uploader.require_additive(json.dumps({'files': [{'path': '/A.RES'}]}).encode(), live)

    def test_any_chat_may_publish_content_without_a_checkpoint_claim(self):
        with patch.object(uploader, 'upload_content') as publish:
            uploader.main(['--content-only'])
        publish.assert_called_once_with(None, False, checkpoint=None, owner=None)
        with patch.object(uploader, 'publication_lock', return_value=nullcontext()), \
                patch.object(uploader, 'require_owner') as require_owner, \
                patch.object(uploader, 'validated_content', side_effect=RuntimeError('stop')), \
                self.assertRaisesRegex(RuntimeError, 'stop'):
            uploader.upload_content()
        require_owner.assert_not_called()
        with patch('sys.stderr', io.StringIO()), self.assertRaises(SystemExit):
            uploader.main(['--content-only', '--checkpoint', 'combined'])

    def test_stages_scoped_manifests_and_verifies_remote_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            objects = self.upload_fixture(root)
            self.assertIn(f'releases/{"a" * 40}/manifest.json', objects)
            self.assertIn(f'releases/{"a" * 40}/gallery.json', objects)
            self.assertNotIn('content/manifest.json', objects)
            self.assertNotIn('devlog-gallery.json', objects)
            self.assertEqual(sum(key.startswith('app-icons/') for key in objects), 3)
            self.assertTrue((root / 'private/reports/cloudflare-upload.json').exists())

    def test_bad_remote_bytes_prevent_success_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with self.assertRaisesRegex(ValueError, 'staged object differs'):
                self.upload_fixture(root, corrupt=True)
            self.assertFalse((root / 'private/reports/cloudflare-upload.json').exists())
