#!/usr/bin/env python3
"""Format OMP JSONL output into human-readable CI log lines.

Usage:
    omp -p --mode json "..." | python3 .omp/stream-log.py
"""

import json
import sys


def brief(text: str, limit: int = 200) -> str:
    text = text.strip().replace("\n", "\\n")
    return text[:limit] + "..." if len(text) > limit else text


def brief_args(args: dict, limit: int = 120) -> str:
    priority_fields = ["command", "pattern", "query", "path", "paths", "action", "symbol"]
    parts = []
    for key in priority_fields:
        if key in args:
            val = args[key]
            if isinstance(val, list):
                val = ", ".join(str(v) for v in val)
            parts.append(f"{key}={brief(str(val), 60)}")
    if not parts:
        s = json.dumps(args, separators=(",", ":"))
        return s[:limit] + "..." if len(s) > limit else s
    return ", ".join(parts)


turn_number = 0

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    try:
        evt = json.loads(line)
    except json.JSONDecodeError:
        continue

    t = evt.get("type", "")

    if t == "agent_start":
        print("🚀 OMP agent started")
        sys.stdout.flush()

    elif t == "turn_start":
        turn_number += 1
        sys.stdout.flush()

    elif t == "tool_execution_start":
        tool = evt.get("toolName", "?")
        args = evt.get("args", {})
        intent = evt.get("intent", "")
        if intent:
            print(f"  🔧 {tool}: {intent}")
        elif tool == "bash":
            print(f"  🔧 bash: {brief(args.get('command', ''), 150)}")
        elif tool in ("read", "write", "edit"):
            print(f"  🔧 {tool}: {args.get('path', args.get('input', {}).get('path', ''))}")
        elif tool == "search":
            print(f"  🔧 search: {brief(args.get('pattern', ''), 100)}")
        elif tool == "ast_grep":
            print(f"  🔧 ast_grep: {brief(args.get('pattern', ''), 60)}")
        else:
            print(f"  🔧 {tool}({brief_args(args)})")
        sys.stdout.flush()

    elif t == "tool_execution_end":
        tool = evt.get("toolName", "?")
        is_error = evt.get("isError", False)
        result = evt.get("result", {})
        contents = result.get("content", []) if isinstance(result, dict) else []
        out_text = ""
        if isinstance(contents, list):
            out_text = " ".join(
                c.get("text", "") for c in contents if isinstance(c, dict) and c.get("type") == "text"
            ).strip()

        if is_error:
            print(f"  ✗ {tool} error: {brief(out_text, 200)}")
        elif not out_text:
            print(f"  ✓ {tool}: done")
        elif len(out_text.splitlines()) <= 3 and len(out_text) <= 200:
            print(f"  ✓ {tool}: {brief(out_text, 200)}")
        else:
            first = out_text.splitlines()[0][:100]
            line_count = len(out_text.splitlines())
            print(f"  ✓ {tool}: {first}... ({line_count} lines)")
        sys.stdout.flush()

    elif t == "message_end":
        msg = evt.get("message", {})
        role = msg.get("role", "")
        content = msg.get("content", [])

        if role == "assistant" and isinstance(content, list):
            texts = [
                c["text"].strip()
                for c in content
                if isinstance(c, dict) and c.get("type") == "text" and c.get("text", "").strip()
            ]
            if texts:
                combined = "\n".join(texts)
                for ln in combined.splitlines():
                    print(f"  {ln}")
                sys.stdout.flush()

    elif t == "agent_end":
        messages = evt.get("messages", [])
        final_texts = []
        for msg in messages:
            if msg.get("role") == "assistant" and isinstance(msg.get("content"), list):
                for c in msg["content"]:
                    if isinstance(c, dict) and c.get("type") == "text" and c.get("text", "").strip():
                        final_texts.append(c["text"].strip())

        usage = evt.get("usage", {})
        total_tokens = usage.get("totalTokens", 0) if usage else 0

        suffix = f", {total_tokens} tokens" if total_tokens else ""
        print(f"\n✅ Agent finished ({turn_number} turn{'s' if turn_number != 1 else ''}{suffix})")
        sys.stdout.flush()