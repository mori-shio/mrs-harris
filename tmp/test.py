import urllib.request
import json

req = urllib.request.Request("http://localhost:8080/api/auth/login", data=b'{"username":"admin",REDACTED}', headers={"Content-Type": "application/json"})
with urllib.request.urlopen(req) as response:
    data = json.loads(response.read().decode())
    token = data.get("token")

req2 = urllib.request.Request("http://localhost:8080/jobs/1/runs/2", headers={"Cookie": "jwt=" + token})
try:
    with urllib.request.urlopen(req2) as response:
        html = response.read().decode()
        with open("/tmp/output.html", "w") as f:
            f.write(html)
        print("HTML saved.")
except Exception as e:
    print("Failed:", e)
