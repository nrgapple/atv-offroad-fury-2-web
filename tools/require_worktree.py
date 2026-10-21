#!/usr/bin/env python3
"""Claude Code PreToolUse hook: keep task work out of the shared checkout.

The shared checkout is the repository's main worktree (its `.git` is a
directory) and stays on `main` for coordination; see AGENTS.md "Chat worktrees
and branches". This hook refuses file edits there and git commands that commit,
switch branches or rewrite its tree, except a verified clean fast-forward of
main to origin/main. Ignored shared storage (private/, work/,
target/) stays writable, and linked worktrees (whose `.git` is a file) are
unaffected. Unresolvable paths are allowed rather than guessed.
"""
import json
import os
import re
import shlex
import subprocess
import sys
from pathlib import Path

SHARED_STORAGE = {'private', 'work', 'target'}
GIT_WRITES = {'commit', 'switch', 'checkout', 'merge', 'rebase', 'reset', 'stash', 'cherry-pick',
              'revert', 'am', 'apply', 'push', 'restore', 'rm', 'mv', 'add', 'clean'}
EDIT_TOOLS = {'Edit', 'Write', 'NotebookEdit', 'MultiEdit'}
ADVICE = ('Use your own worktree: git fetch origin && git worktree add -b <branch> '
          '~/Projects/atv-worktrees/<task> origin/main (AGENTS.md "Chat worktrees and branches").')


def checkout(path):
    """Return (root, is_shared) for the checkout containing path, or (None, False)."""
    path = Path(path)
    while not path.exists():
        if path.parent == path:
            return None, False
        path = path.parent
    path = path.resolve()
    for candidate in [path, *path.parents]:
        marker = candidate / '.git'
        if marker.exists():
            return candidate, marker.is_dir()
    return None, False


def blocked_edit(file_path):
    root, shared = checkout(file_path)
    if not shared:
        return None
    path = Path(file_path)
    existing = path
    while not existing.exists() and existing.parent != existing:
        existing = existing.parent
    try:
        relative = (existing.resolve() / path.relative_to(existing)).relative_to(root)
    except ValueError:
        return None
    if relative.parts and relative.parts[0] in SHARED_STORAGE:
        return None
    return f'Refusing to edit {relative} in the shared checkout {root}. {ADVICE}'


def git_subcommand(tokens):
    """Return (subcommand, -C directory) for a git invocation, else (None, None)."""
    index = 0
    while index < len(tokens) and re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*=.*', tokens[index]):
        index += 1
    if index >= len(tokens) or os.path.basename(tokens[index]) != 'git':
        return None, None
    index += 1
    directory = None
    while index < len(tokens) and tokens[index].startswith('-'):
        if tokens[index] == '-C' and index + 1 < len(tokens):
            directory = tokens[index + 1]
            index += 2
        elif tokens[index] in ('-c', '--git-dir', '--work-tree') and index + 1 < len(tokens):
            index += 2
        else:
            index += 1
    return (tokens[index] if index < len(tokens) else None), directory


def shared_main_sync(tokens, root):
    """Allow only the documented fast-forward, with Git's own state checks."""
    args = tokens[1:]
    if args[:1] == ['-C']:
        args = args[2:]
    if args != ['merge', '--ff-only', 'origin/main']:
        return False

    def git(*arguments):
        return subprocess.run(['git', '-C', str(root), *arguments], check=True,
                              capture_output=True, text=True, timeout=5).stdout.strip()

    try:
        if git('symbolic-ref', '--short', 'HEAD') != 'main':
            return False
        if git('status', '--porcelain=v1', '--untracked-files=all'):
            return False
        if git('rev-parse', '--verify', 'origin/main^{commit}') != git(
                'rev-parse', '--verify', 'refs/remotes/origin/main^{commit}'):
            return False
        for state in ('MERGE_HEAD', 'CHERRY_PICK_HEAD', 'REVERT_HEAD',
                      'rebase-merge', 'rebase-apply', 'sequencer', 'index.lock'):
            path = Path(git('rev-parse', '--git-path', state))
            if (path if path.is_absolute() else root / path).exists():
                return False
        git('merge-base', '--is-ancestor', 'HEAD', 'refs/remotes/origin/main')
    except (OSError, subprocess.SubprocessError):
        return False
    return True


def blocked_bash(command, cwd):
    variables = dict(os.environ)
    expand = lambda value: os.path.expanduser(re.sub(r'\$\{?([A-Za-z_][A-Za-z0-9_]*)\}?',
                                                     lambda m: variables.get(m.group(1), m.group(0)), value))
    for segment in re.split(r'&&|\|\||[;\n|]', command):
        try:
            tokens = shlex.split(segment)
        except ValueError:
            continue
        if not tokens:
            continue
        if len(tokens) == 1 and re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*=.*', tokens[0]):
            name, value = tokens[0].split('=', 1)
            variables[name] = expand(value)
            continue
        if tokens[0] == 'cd' and len(tokens) > 1:
            cwd = os.path.join(cwd, expand(tokens[1]))
            continue
        subcommand, directory = git_subcommand(tokens)
        if subcommand not in GIT_WRITES:
            continue
        target = os.path.join(cwd, expand(directory)) if directory else cwd
        root, shared = checkout(target)
        if shared:
            if shared_main_sync(tokens, root):
                continue
            return f'Refusing `git {subcommand}` in the shared checkout {root}. {ADVICE}'
    return None


def main():
    event = json.load(sys.stdin)
    tool, data = event.get('tool_name'), event.get('tool_input') or {}
    cwd = event.get('cwd') or os.getcwd()
    if tool in EDIT_TOOLS:
        target = data.get('file_path') or data.get('notebook_path')
        reason = blocked_edit(os.path.join(cwd, target)) if target else None
    elif tool == 'Bash':
        reason = blocked_bash(data.get('command', ''), cwd)
    else:
        reason = None
    if reason:
        print(reason, file=sys.stderr)
        sys.exit(2)


if __name__ == '__main__':
    main()
