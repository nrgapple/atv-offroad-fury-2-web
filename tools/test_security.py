import unittest
from check_secrets import candidates


class CredentialDetection(unittest.TestCase):
    def test_cloudflare_literal_and_private_key_are_detected_without_echoing_values(self):
        secret = b'Z' * 40
        data = b'api_token="' + secret + b'"\n' + b'-----BEGIN ' + b'PRIVATE KEY-----'
        result = candidates(data)
        self.assertEqual({kind for kind, _ in result}, {'credential literal', 'private key'})
        self.assertNotIn(secret.decode(), str(result))

    def test_identifiers_env_references_and_hashes_are_not_credentials(self):
        self.assertEqual(candidates(b'account_id="abc123"\napi_token=os.environ["TOKEN"]\nsha256="' + b'a' * 64 + b'"'), [])


if __name__ == '__main__':
    unittest.main()
