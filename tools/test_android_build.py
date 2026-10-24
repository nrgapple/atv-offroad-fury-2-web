#!/usr/bin/env python3
"""Synthetic tests for tools/build_android.py (no Android SDK, NDK or content needed)."""
from pathlib import Path
import os
import stat
import sys
import tempfile
import unittest
import xml.etree.ElementTree as ET
import zipfile
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import build_android  # noqa: E402

ANDROID = '{http://schemas.android.com/apk/res/android}'


class ManifestTest(unittest.TestCase):
    def setUp(self):
        self.root = ET.fromstring(build_android.android_manifest('0.2.0', '1021'))

    def test_identity_and_version(self):
        self.assertEqual(self.root.get('package'), 'com.furyremastered.android')
        self.assertEqual(self.root.get(ANDROID + 'versionCode'), '1021')
        self.assertEqual(self.root.get(ANDROID + 'versionName'), '0.2.0')

    def test_native_activity_runs_the_engine_library(self):
        application = self.root.find('application')
        self.assertEqual(application.get(ANDROID + 'hasCode'), 'false')
        self.assertEqual(application.get(ANDROID + 'label'), 'Fury Remastered')
        activity = application.find('activity')
        self.assertEqual(activity.get(ANDROID + 'name'), 'android.app.NativeActivity')
        self.assertEqual(activity.get(ANDROID + 'screenOrientation'), 'sensorLandscape')
        # Rotation and keyboard changes must not restart the activity (the engine is one process).
        for change in ('orientation', 'screenSize', 'keyboardHidden'):
            self.assertIn(change, activity.get(ANDROID + 'configChanges').split('|'))
        meta = activity.find('meta-data')
        self.assertEqual((meta.get(ANDROID + 'name'), meta.get(ANDROID + 'value')),
                         ('android.app.lib_name', 'atv_app'))
        self.assertEqual(build_android.LIBRARY, 'libatv_app.so')

    def test_internet_permission_only(self):
        permissions = [p.get(ANDROID + 'name') for p in self.root.findall('uses-permission')]
        self.assertEqual(permissions, ['android.permission.INTERNET'])


class MembersTest(unittest.TestCase):
    GOOD = ['AndroidManifest.xml', 'resources.arsc', 'lib/arm64-v8a/libatv_app.so',
            'res/mipmap-xxxhdpi-v4/ic_launcher.png', 'res/mipmap-mdpi/ic_launcher.png']

    def test_allowlist_accepts_a_normal_apk(self):
        build_android.check_members(self.GOOD)

    def test_game_content_is_refused(self):
        for name in ('assets/content.pack', 'assets/content/manifest.json', 'assets/ab12.bin',
                     'assets/E01_NAT.RES', 'assets/MUSIC.VAG', 'assets/song.m4a', 'SCUS_972.11',
                     'lib/arm64-v8a/other.so', 'classes.dex'):
            with self.subTest(name=name), self.assertRaises(SystemExit):
                build_android.check_members(self.GOOD + [name])

    def test_library_is_required(self):
        with self.assertRaises(SystemExit):
            build_android.check_members(['AndroidManifest.xml', 'resources.arsc'])

    def test_check_apk_reads_a_synthetic_archive(self):
        with tempfile.TemporaryDirectory() as tmp:
            apk = Path(tmp) / 'test.apk'
            with zipfile.ZipFile(apk, 'w') as z:
                z.writestr('AndroidManifest.xml', b'manifest')
                z.writestr('resources.arsc', b'table')
                info = zipfile.ZipInfo('lib/arm64-v8a/libatv_app.so')
                info.compress_type = zipfile.ZIP_STORED
                z.writestr(info, b'\x7fELF' + b'\0' * 60)
            self.assertEqual(build_android.check_apk(apk), 3)
            with zipfile.ZipFile(apk, 'a') as z:
                z.writestr('assets/content.pack', b'FURYPACK')
            with self.assertRaises(SystemExit):
                build_android.check_apk(apk)


class ContextTest(unittest.TestCase):
    def test_refuses_ci(self):
        for env in ({'GITHUB_ACTIONS': 'true'}, {'CI': '1'}):
            with self.subTest(env=env), self.assertRaises(SystemExit):
                build_android.refuse_ci(env)
        build_android.refuse_ci({})

    def test_workspace_version(self):
        text = '[workspace]\nmembers = []\n\n[workspace.package]\nedition = "2024"\nversion = "1.4.0"\n'
        self.assertEqual(build_android.workspace_version(text), '1.4.0')
        with self.assertRaises(SystemExit):
            build_android.workspace_version('[package]\nversion = "9.9.9"\n')

    def test_apk_name(self):
        self.assertEqual(build_android.apk_name('0.2.0', 'abc1234'),
                         'fury-remastered-0.2.0-abc1234-android-arm64.apk')

    def test_dirty_tree_is_refused(self):
        def fake_git(*args):
            return {'status': ' M Cargo.toml'}.get(args[0], 'abc1234')
        with patch.object(build_android, 'git', fake_git), patch.dict(os.environ, {}, clear=True), \
                self.assertRaises(SystemExit) as raised:
            build_android.main([])
        self.assertIn('dirty', str(raised.exception))

    def test_keystore_is_created_once_with_private_permissions(self):
        with tempfile.TemporaryDirectory() as tmp:
            keystore, password = Path(tmp) / 'keys/k.jks', Path(tmp) / 'keys/k.pass'
            calls = []

            def fake_run(cmd, **kwargs):
                calls.append(cmd)
                keystore.write_bytes(b'jks')
            with patch.object(build_android, 'run', fake_run):
                build_android.ensure_keystore(keystore, password)
                build_android.ensure_keystore(keystore, password)
            self.assertEqual(len(calls), 1)
            self.assertEqual(stat.S_IMODE(password.stat().st_mode), 0o600)
            self.assertEqual(stat.S_IMODE(keystore.stat().st_mode), 0o600)
            self.assertGreater(len(password.read_text().strip()), 30)
            # The keystore must never land inside the repository.
            self.assertNotIn(str(build_android.ROOT), str(build_android.KEYSTORE))


if __name__ == '__main__':
    unittest.main()
