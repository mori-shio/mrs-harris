import urllib.request
import urllib.parse

class NoRedirectHandler(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None

opener = urllib.request.build_opener(NoRedirectHandler())
urllib.request.install_opener(opener)

login_data = urllib.parse.urlencode({'username': 'admin', REDACTED}).encode('utf-8')
req = urllib.request.Request('http://127.0.0.1:8080/login', data=login_data, method='POST')
cookie = ''
try:
    with urllib.request.urlopen(req) as resp: pass
except urllib.error.HTTPError as e:
    cookie = e.headers.get('Set-Cookie').split(';')[0]

create_data = urllib.parse.urlencode({
    'name': 'test_job_edit',
    'description': 'test',
    'job_type': 'one_shot',
    'worker_definition_id': '1',
    'timeout_sec': '3600',
    'max_retries': '3',
    'backoff': 'exponential',
    'base_delay_sec': '10',
    'script': 'echo hello'
}).encode('utf-8')

req = urllib.request.Request('http://127.0.0.1:8080/jobs/new', data=create_data, method='POST')
req.add_header('Cookie', cookie)
try:
    with urllib.request.urlopen(req) as resp:
        html = resp.read().decode('utf-8')
        with open("new_page.html", "w") as f: f.write(html)
        print("Saved to new_page.html")
except urllib.error.HTTPError as e:
    print('Err:', e.code)
