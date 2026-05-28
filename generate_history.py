import uuid

job_id = "5f6cd505-7fff-44f1-92b1-9a8e9148ffe6"
print("-- (5) history-heavy-job (OneShot / Fargate)")
print("(")
print(f"    '{job_id}',")
print("    'history-heavy-job',")
print("    '設定変更履歴が13件存在するテスト用ジョブ',")
print("    'one_shot',")
print("    '{\"command\": \"echo test\"}',")
print("    NULL,")
print("    'fargate',")
print("    '{\"backoff\": \"fixed\", \"max_retries\": 1, \"base_delay_sec\": 5}',")
print("    600,")
print("    1,")
print("    '[\"history\", \"test\"]',")
print("    'd1a2b3c4-e5f6-7a8b-9c0d-1e2f3a4b5c6d',")
print("    '01111111-1111-1111-1111-111111111111'")
print("),")

print("\n\n-- history-heavy-job v1 to v13")
for i in range(1, 14):
    h_id = f"hx{i:02}x{i:02}-5555-5555-5555-555555555555"
    payload = '{"説明": "v' + str(i) + ' の変更内容", "ジョブ名": "history-heavy-job"}'
    print(f"(")
    print(f"    '{h_id}',")
    print(f"    '{job_id}',")
    print(f"    {i},")
    print(f"    '{payload}',")
    print(f"    'system-admin',")
    print(f"    '2026-05-24 10:{i:02}:00.000'")
    if i == 13:
        print(");")
    else:
        print("),")

