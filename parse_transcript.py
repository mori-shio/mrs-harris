import json
import sys

transcript_path = "/Users/shion.morikawa/.gemini/antigravity/brain/72929b88-f222-477b-b77d-1bed95f92b8a/.system_generated/logs/transcript.jsonl"

with open(transcript_path, 'r', encoding='utf-8') as f:
    for line in f:
        try:
            entry = json.loads(line)
            if entry.get("type") == "USER_INPUT":
                content = entry.get("content", "")
                if "<USER_REQUEST>" in content:
                    req = content.split("<USER_REQUEST>")[1].split("</USER_REQUEST>")[0].strip()
                    print(f"- {req}")
                else:
                    print(f"- {content.strip()}")
        except Exception as e:
            pass
