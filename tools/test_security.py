import unittest
from check_secrets import candidates
import verify_repository as repository


class CredentialDetection(unittest.TestCase):
    def test_cloudflare_literal_and_private_key_are_detected_without_echoing_values(self):
        secret = b'Z' * 40
        data = b'api_token="' + secret + b'"\n' + b'-----BEGIN ' + b'PRIVATE KEY-----'
        result = candidates(data)
        self.assertEqual({kind for kind, _ in result}, {'credential literal', 'private key'})
        self.assertNotIn(secret.decode(), str(result))

    def test_identifiers_env_references_and_hashes_are_not_credentials(self):
        self.assertEqual(candidates(b'account_id="abc123"\napi_token=os.environ["TOKEN"]\nsha256="' + b'a' * 64 + b'"'), [])


class RepositoryHygiene(unittest.TestCase):
    def test_game_resource_suffixes_are_blocked_including_text_formats(self):
        for name in ('assets/RIDER.PB', 'config/game.ini', 'tracks/ENV01.tdf', 'notes/strings.txt', 'a/b.MOT'):
            with self.assertRaisesRegex(ValueError, 'game resource'):
                repository.check_path(name, '100644')
        for name in ('web/app.mjs', 'research/WATER.md', 'Cargo.lock', 'web/fury-app-icons.json'):
            repository.check_path(name, '100644')
        with self.assertRaisesRegex(ValueError, 'symlink'):
            repository.check_path('web/app.mjs', '120000')
        with self.assertRaisesRegex(ValueError, 'private'):
            repository.check_path('private/reports/x.json', '100644')

    def test_decompiler_labels_and_synthesized_names_are_detected(self):
        # Fixtures are concatenated so this file never contains a literal match.
        samples = ['call ' + 'FUN' + '_00123456' + '()', 'read ' + 'DAT' + '_00404868', 'goto ' + 'LAB' + '_0011aa00',
                   'undefined' + '4 value;', 'i' + 'Var' + '3 = 1;', 'pc' + 'Var' + '12 = s;',
                   'f(' + 'param' + '_2)', 'local' + '_1c = 0;']
        for sample in samples:
            with self.subTest(sample=sample), self.assertRaisesRegex(ValueError, 'decompiler output'):
                repository.check_content('research/NOTES.md', sample.encode())
        prose = (b'Address `00404868` holds the swizzle flag; returns undefined when absent.\n'
                 b'let envelope = parameter_count; const variance = 2;\n')
        repository.check_content('research/NOTES.md', prose)
        repository.check_content('web/app.mjs', b'if (value === undefined) return;\n')

    def test_large_base64_blobs_require_the_explicit_allowlist(self):
        blob = b'"' + b'QUJD' * (repository.BASE64_LIMIT // 4 + 1) + b'=="'
        with self.assertRaisesRegex(ValueError, 'base64'):
            repository.check_content('web/other.json', blob)
        repository.check_content('web/fury-app-icons.json', blob)
        repository.check_content('web/other.json', b'"' + b'QUJD' * 1000 + b'"')

    def test_binary_large_and_credential_content_still_fails(self):
        with self.assertRaisesRegex(ValueError, 'binary'):
            repository.check_content('web/app.mjs', b'a\0b')
        with self.assertRaisesRegex(ValueError, 'credential'):
            repository.check_content('web/app.mjs', b'-----BEGIN ' + b'PRIVATE KEY-----')


if __name__ == '__main__':
    unittest.main()
