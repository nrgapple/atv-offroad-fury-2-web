#!/usr/bin/env python3
"""Serve ONLY the static web build. Never exposes private/, source, or references."""
import argparse
import re
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1] / 'web/dist'
PROJECT = Path(__file__).resolve().parents[1]
CONTENT = PROJECT / 'private/reports/hosted-content'
ALLOWED = {'index.html', 'landing.css', 'landing.js', 'play.html', 'design-system.css', 'game.css', 'game.js', 'race.js', 'multiplayer.js', 'water.js', 'fog.js', 'train.js', 'wheel-effects.js', 'motorcycle-presentation.js', 'hud.js', 'diagnostics.js', 'frontend.js', 'touch.js', 'touch.css', 'content.js', 'audio.js', 'music.js', 'deployment.json', 'devlog.html', 'devlog.css', 'devlog.js', 'devlog.json', 'workbench.html', 'style.css', 'app.js', 'worker.js', 'input.js', 'controls.js', 'bindings.js', 'atv_web.wasm'}

class StaticServer(ThreadingHTTPServer):
    # ES-module startup opens more connections than Python's default five-slot
    # backlog. Admit the burst without changing the existing resource allowlist.
    request_queue_size = 128


class Handler(SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(ROOT), **kwargs)

    def send_head(self):
        path = unquote(urlsplit(self.path).path)
        if path == '/': path = '/index.html'
        image = re.fullmatch(r'/devlog-images/[a-z0-9-]+\.png', path)
        content = re.fullmatch(r'/content/(?:manifest\.json|[a-f0-9]{64}\.bin)', path)
        if (path.removeprefix('/') not in ALLOWED and not image and not content) or not path.startswith('/'):
            self.send_error(404)
            return None
        self.path = path
        return super().send_head()

    def translate_path(self, path):
        # Only the explicitly requested log and PNG checkpoint gallery are
        # exposed outside dist. Source media and other reports stay inaccessible.
        if re.fullmatch(r'/content/(?:manifest\.json|[a-f0-9]{64}\.bin)', path):
            return str(CONTENT / path.rsplit('/', 1)[1])
        if path == '/devlog.json':
            return str(PROJECT / 'research/devlog.json')
        if re.fullmatch(r'/devlog-images/[a-z0-9-]+\.png', path):
            return str(PROJECT / 'private/reports/devlog-images' / path.rsplit('/', 1)[1])
        return super().translate_path(path)

    def end_headers(self):
        self.send_header('Cache-Control', 'no-store')
        self.send_header('X-Content-Type-Options', 'nosniff')
        self.send_header('Content-Security-Policy', "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; worker-src 'self'; style-src 'self'; media-src blob:; img-src 'self' blob:; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'")
        super().end_headers()

def main():
    global ROOT, PROJECT, CONTENT
    parser = argparse.ArgumentParser()
    parser.add_argument('--bind', default='127.0.0.1', help='Use 0.0.0.0 only with an approved access path')
    parser.add_argument('--port', type=int, default=4173)
    parser.add_argument('--root', type=Path, default=ROOT, help='Static build directory')
    parser.add_argument('--project', type=Path, default=PROJECT, help='Project containing the private checkpoint gallery and log')
    parser.add_argument('--content', type=Path, default=CONTENT, help='Validated private runtime content package')
    args = parser.parse_args()
    ROOT = args.root.resolve()
    PROJECT = args.project.resolve()
    CONTENT = args.content.resolve()
    if not (ROOT / 'atv_web.wasm').is_file(): parser.error('run make web first')
    print(f'Static workbench listening on {args.bind}:{args.port}. Host verification only; remote access is not established.', flush=True)
    StaticServer((args.bind, args.port), Handler).serve_forever()

if __name__ == '__main__': main()
