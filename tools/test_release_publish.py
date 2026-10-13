"""R2 staging cannot change live pointers or deploy unverified dependency bytes."""
import io
import json
from pathlib import Path
import tempfile
import unittest
from urllib.parse import unquote
from unittest.mock import patch
from types import SimpleNamespace

import upload_cloudflare_content as uploader
from package_release import scoped_content_source
from test_release_queue import content


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
            return SimpleNamespace(stdout=json.dumps({'entries': []}) if args[0] == 'git' else commit)
        with patch.object(uploader, 'ROOT', root), patch.object(uploader.Path, 'home', return_value=root), patch.object(uploader, 'validate_release'), patch.object(uploader.subprocess, 'run', side_effect=command), patch.object(uploader, 'urlopen', side_effect=network):
            uploader.upload(root / 'release', commit, release_scoped=True)
        return objects

    def test_stages_scoped_manifests_and_verifies_remote_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            objects = self.upload_fixture(root)
            self.assertIn(f'releases/{"a" * 40}/manifest.json', objects)
            self.assertIn(f'releases/{"a" * 40}/gallery.json', objects)
            self.assertNotIn('content/manifest.json', objects)
            self.assertNotIn('devlog-gallery.json', objects)
            self.assertTrue((root / 'private/reports/cloudflare-upload.json').exists())

    def test_bad_remote_bytes_prevent_success_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with self.assertRaisesRegex(ValueError, 'staged object differs'):
                self.upload_fixture(root, corrupt=True)
            self.assertFalse((root / 'private/reports/cloudflare-upload.json').exists())
