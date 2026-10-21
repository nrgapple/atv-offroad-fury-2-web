import unittest
from pathlib import Path
import tempfile
from check_secrets import candidates
from require_worktree import blocked_bash, blocked_edit
from test_fixtures import git
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


class SharedMainGuard(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        git(self.root, 'init', '-b', 'main')
        git(self.root, 'config', 'user.name', 'Guard tests')
        git(self.root, 'config', 'user.email', 'guard@example.test')
        (self.root / 'README.md').write_text('base\n')
        git(self.root, 'add', 'README.md')
        git(self.root, 'commit', '-m', 'base')
        self.base = git(self.root, 'rev-parse', 'HEAD')
        git(self.root, 'update-ref', 'refs/remotes/origin/main', self.base)

    def blocked(self, command='git merge --ff-only origin/main'):
        return blocked_bash(command, str(self.root))

    def test_clean_main_can_fast_forward_and_match_remote(self):
        git(self.root, 'switch', '-c', 'remote-change')
        (self.root / 'README.md').write_text('remote update\n')
        git(self.root, 'commit', '-am', 'remote update')
        remote = git(self.root, 'rev-parse', 'HEAD')
        git(self.root, 'update-ref', 'refs/remotes/origin/main', remote)
        git(self.root, 'switch', 'main')
        self.assertIsNone(self.blocked())
        self.assertIsNone(self.blocked(f'git -C "{self.root}" merge --ff-only origin/main'))
        git(self.root, 'merge', '--ff-only', 'origin/main')
        self.assertEqual(git(self.root, 'rev-parse', 'HEAD'), remote)
        self.assertIsNone(self.blocked())

    def test_dirty_staged_and_untracked_files_prevent_sync(self):
        (self.root / 'README.md').write_text('local work\n')
        self.assertIsNotNone(self.blocked())
        git(self.root, 'add', 'README.md')
        self.assertIsNotNone(self.blocked())
        git(self.root, 'reset', '--hard', 'HEAD')
        (self.root / 'untracked.md').write_text('unfinished work\n')
        self.assertIsNotNone(self.blocked())

    def test_shadowing_remote_name_prevents_sync(self):
        git(self.root, 'switch', '-c', 'origin/main')
        git(self.root, 'commit', '--allow-empty', '-m', 'shadow remote name')
        git(self.root, 'switch', 'main')
        self.assertIsNotNone(self.blocked())

    def test_local_commits_and_wrong_branch_prevent_sync(self):
        git(self.root, 'commit', '--allow-empty', '-m', 'local work')
        self.assertIsNotNone(self.blocked())
        git(self.root, 'update-ref', 'refs/remotes/origin/main', 'HEAD')
        git(self.root, 'switch', '-c', 'feature')
        self.assertIsNotNone(self.blocked())
        git(self.root, 'switch', '--detach')
        self.assertIsNotNone(self.blocked())

    def test_unfinished_operations_prevent_sync(self):
        for state in ('MERGE_HEAD', 'CHERRY_PICK_HEAD', 'REVERT_HEAD',
                      'rebase-merge', 'rebase-apply', 'sequencer', 'index.lock'):
            with self.subTest(state=state):
                path = self.root / '.git' / state
                path.write_text(self.base + '\n')
                try:
                    self.assertIsNotNone(self.blocked())
                finally:
                    path.unlink()

    def test_other_shared_writes_remain_blocked(self):
        for command in ('git merge origin/main', 'git merge --ff-only feature',
                        'git merge --ff-only origin/main --autostash',
                        'git -c core.hooksPath=/dev/null merge --ff-only origin/main',
                        'git reset --hard origin/main', 'git switch feature',
                        'git commit -am change', 'git push origin main',
                        'git merge --ff-only origin/main && git commit -am change'):
            with self.subTest(command=command):
                self.assertIsNotNone(self.blocked(command))
        self.assertIsNotNone(blocked_edit(self.root / 'README.md'))
        self.assertIsNone(blocked_edit(self.root / 'private' / 'report.md'))

    def test_linked_task_worktree_remains_writable(self):
        task = self.root / 'task'
        git(self.root, 'worktree', 'add', '-b', 'task', str(task))
        self.assertIsNone(blocked_bash('git merge origin/main', str(task)))
        self.assertIsNone(blocked_edit(task / 'README.md'))


if __name__ == '__main__':
    unittest.main()
