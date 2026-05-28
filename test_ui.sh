#!/bin/bash
rm -f cookies.txt
curl -s -c cookies.txt -X POST -H "Content-Type: application/json" -d '{"username":"admin","password":"password"}' http://localhost:8080/api/auth/login

echo "--- /jobs ---"
curl -s -b cookies.txt http://localhost:8080/jobs | grep -o 'hx-get="/jobs?space=[^"]*"' || true
curl -s -b cookies.txt http://localhost:8080/jobs | grep -o 'selectSpaceTab(this, [^)]*)' || true

echo "--- /jobs/test-verify-job ---"
curl -s -b cookies.txt -X POST -H "Content-Type: application/json" -d '{"name":"test-verify-job","job_type":"one_shot","payload":{"command":"echo 1"},"worker_type":"fargate"}' http://localhost:8080/api/jobs > /dev/null
curl -s -b cookies.txt http://localhost:8080/jobs/test-verify-job | grep -i 'ジョブID' || echo "SUCCESS: Job ID not found"
curl -s -b cookies.txt http://localhost:8080/jobs/test-verify-job | grep -o 'th-sort-container' || echo "WARNING: th-sort-container not found"
curl -s -b cookies.txt http://localhost:8080/jobs/test-verify-job | grep -o 'class="th-sort-icon[^"]*"' || true

