import urllib.request
import urllib.parse
import urllib.error
import http.cookiejar
import json
import re

cj = http.cookiejar.CookieJar()
opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(cj))
urllib.request.install_opener(opener)

# 1. Login
url_login = 'http://localhost:8080/api/auth/login'
data = json.dumps({'username': 'admin', REDACTED}).encode('utf-8')
headers = {'Content-Type': 'application/json'}
req = urllib.request.Request(url_login, data=data, headers=headers)
try:
    resp = urllib.request.urlopen(req)
    print("Login (password) status:", resp.getcode())
except urllib.error.HTTPError as e:
    print("Login (password) failed:", e.code)
    data = json.dumps({'username': 'admin', REDACTED}).encode('utf-8')
    req = urllib.request.Request(url_login, data=data, headers=headers)
    resp = urllib.request.urlopen(req)
    print("Login (admin) status:", resp.getcode())
    # The server might not set cookies, let's check cookies
    token = None
    response_json = json.loads(resp.read().decode('utf-8'))
    token = response_json.get('token')
    if token:
        # manually add cookie to cookiejar
        cookie = http.cookiejar.Cookie(version=0, name='token', value=token, port=None, port_specified=False, domain='localhost', domain_specified=False, domain_initial_dot=False, path='/', path_specified=True, secure=False, expires=None, discard=True, comment=None, comment_url=None, rest={'HttpOnly': None}, rfc2109=False)
        cj.set_cookie(cookie)
        print("Token cookie set manually.")

# 2. Check Jobs List
req_jobs = urllib.request.Request('http://localhost:8080/jobs')
resp_jobs = urllib.request.urlopen(req_jobs)
html_jobs = resp_jobs.read().decode('utf-8')
print("--- /jobs ---")
print("Space IDs in hx-get:", re.findall(r'hx-get="/jobs\?space=([^"]+)"', html_jobs)[:3])
print("Space IDs in onclick:", re.findall(r"onclick=\"selectSpaceTab\(this, '([^']+)'\)\"", html_jobs)[:3])

# 3. Create job
url_create = 'http://localhost:8080/api/jobs'
job_data = json.dumps({"name": "test-verify-job", "job_type": "one_shot", "payload": {"command": "echo 1"}, "worker_type": "fargate"}).encode('utf-8')
req_create = urllib.request.Request(url_create, data=job_data, headers=headers)
try:
    urllib.request.urlopen(req_create)
except urllib.error.HTTPError as e:
    print("Create job returned:", e.code)

# 4. Check Job detail
req_detail = urllib.request.Request('http://localhost:8080/jobs/test-verify-job')
resp_detail = urllib.request.urlopen(req_detail)
html_detail = resp_detail.read().decode('utf-8')
print("\n--- /jobs/test-verify-job ---")
if "ジョブID" in html_detail or "Job ID" in html_detail:
    print("WARNING: Job ID is still in the overview card!")
else:
    print("SUCCESS: Job ID not found in the overview card.")

if "th-sort-container" in html_detail:
    print("SUCCESS: th-sort-container found in detail page (runs table).")
    match = re.search(r'class="th-sort-icon[^"]*"', html_detail)
    if match:
        print(f"Sort icon classes: {match.group(0)}")
else:
    print("WARNING: th-sort-container not found.")
