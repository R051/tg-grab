"""Exercise the built image with disposable data and no Telegram credentials."""
import base64
import http.client
import json
import os
from pathlib import Path
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
import uuid

image = sys.argv[1]
name = 'tg-grab-check-' + uuid.uuid4().hex[:12]
root = Path(tempfile.mkdtemp(prefix='tg-grab-container-'))
token = 'tg-grab-disposable-test-only'
(root / 'config.json').write_text(json.dumps({'server_token': token}))
files = root / 'downloads' / 'qa folder'
files.mkdir(parents=True)
(files / 'probe.mp4').write_bytes(b'0123456789ABCDEF')
(files / 'probe.mp4.part').write_bytes(b'partial')
(files / 'probe.mp4.progress').write_bytes(b'progress')


def docker(*args):
    return subprocess.check_output(['docker', *args], text=True).strip()


def request(path, body=None, headers=None, method=None, auth=True):
    all_headers = {'Authorization': 'Bearer ' + token} if auth else {}
    all_headers.update(headers or {})
    payload = None if body is None else json.dumps(body).encode()
    if payload is not None:
        all_headers['Content-Type'] = 'application/json'
    req = urllib.request.Request(base + path, data=payload, headers=all_headers, method=method)
    try:
        response = urllib.request.urlopen(req, timeout=3)
    except urllib.error.HTTPError as error:
        response = error
    with response:
        return response.status, {key.lower(): value for key, value in response.headers.items()}, response.read()


def ready():
    for _ in range(30):
        try:
            if request('/')[0] == 200:
                return
        except (OSError, http.client.HTTPException):
            pass
        time.sleep(0.5)
    raise RuntimeError('Container did not become ready')


def invoke(command, body=None, expected=200):
    status, _, data = request('/api/invoke/' + command, body or {})
    assert status == expected, (command, status, data.decode()[:200])
    return json.loads(data)


try:
    docker('run', '-d', '--rm', '--name', name, '-p', '127.0.0.1::8080',
           '-v', str(root) + ':/data', image)
    port = int(docker('port', name, '8080/tcp').rsplit(':', 1)[1])
    base = f'http://127.0.0.1:{port}'
    ready()
    status, _, html = request('/')
    assert status == 200
    asset = re.search(r'<script[^>]+src="([^"]+)"', html.decode()).group(1)
    status, _, script = request(asset)
    assert status == 200
    script = script.decode()
    assert '个人媒体工作台' not in script and 'Personal media workspace' not in script
    assert all(command in script for command in ['joined_channels', 'history_search', 'library_get'])
    assert request('/brand/logo.svg')[0] == 200
    assert request('/api/invoke/get_status', {}, auth=False)[0] == 401
    assert invoke('get_status')['authorized'] is False
    assert token not in json.dumps(invoke('get_settings'))
    assert invoke('history_search', {'link': 'https://t.me/telegram', 'kind': 'invalid'}, 400)['error'] == 'INVALID_MEDIA_KINDS'
    assert invoke('joined_channels', expected=400)['error'] == 'API_NOT_CONFIGURED'
    folder = invoke('folder_save', {'name': 'Container check'})
    favorite = invoke('favorite_save', {'item': {'kind': 'document', 'title': 'Container check',
        'url': 'https://t.me/telegram/1', 'folderId': folder['id'], 'notes': 'Temporary test entry'}})
    assert invoke('library_get')['items'][0]['id'] == favorite['id']
    backup = invoke('library_export')
    invoke('library_import', {'data': backup})
    assert len(invoke('library_get')['items']) == 1
    assert [item['name'] for item in invoke('list_files')] == ['qa folder/probe.mp4']
    path = '/files/qa%20folder/probe.mp4'
    status, headers, content = request(path, headers={'Range': 'bytes=3-7'})
    assert status == 206 and content == b'34567'
    assert headers['content-range'] == 'bytes 3-7/16'
    assert request(path, method='HEAD')[2] == b''
    assert request(path, headers={'Range': 'bytes=99-'})[0] == 416
    assert request('/files/%2e%2e/config.json')[0] == 404
    with socket.create_connection(('127.0.0.1', port), timeout=3) as ws:
        key = base64.b64encode(os.urandom(16)).decode()
        ws.sendall((f'GET /ws?token={token} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n'
            f'Upgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\n'
            'Sec-WebSocket-Version: 13\r\n\r\n').encode())
        assert ws.recv(1024).startswith(b'HTTP/1.1 101')
    docker('restart', name)
    port = int(docker('port', name, '8080/tcp').rsplit(':', 1)[1])
    base = f'http://127.0.0.1:{port}'
    ready()
    assert invoke('library_get')['items'][0]['id'] == favorite['id']
    assert request(path)[2] == b'0123456789ABCDEF'
    print('Passed: startup, Web assets, API auth, search routes, collections, backups, '
          'file listing, Range/HEAD, traversal, WebSocket, and restart persistence.')
finally:
    subprocess.run(['docker', 'rm', '--force', name], check=False, stdout=subprocess.DEVNULL)
    shutil.rmtree(root, ignore_errors=True)
