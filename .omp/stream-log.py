#!/usr/bin/env python3
"""Format OMP JSONL output into human-readable CI log lines.

Usage:
    omp -p --mode json "..." | python3 .omp/stream-log.py
"""

import json
import sys


def _as_str(value) -> str:
    """Coerce an arbitrary JSON-decoded value to a string for safe concatenation.

    JSONL content can include non-string ``text`` fields (None, ints, lists,
    dicts) when the upstream tool result is not the expected shape. Treat them
    as empty so downstream joins never raise ``TypeError`` and break the
    ``omp`` pipe.
    """
    if value is None:
        return ""
    if isinstance(value, str):
        return value
    if isinstance(value, (list, dict)):
        try:
            return json.dumps(value, separators=(",", ":"))
        except (TypeError, ValueError):
            return str(value)
    return str(value)


def brief(text, limit: int = 200) -> str:
    text = _as_str(text).strip().replace("\n", "\\n")
    return text[:limit] + "..." if len(text) > limit else text


def brief_args(args, limit: int = 120) -> str:
    priority_fields = ["command", "pattern", "query", "path", "paths", "action", "symbol"]
    parts = []
    for key in priority_fields:
        if isinstance(args, dict) and key in args:
            val = args[key]
            if isinstance(val, list):
                val = ", ".join(str(v) for v in val)
            parts.append(f"{key}={brief(val, 60)}")
    if not parts:
        s = json.dumps(args, separators=(",", ":")) if args is not None else ""
        return s[:limit] + "..." if len(s) > limit else s
    return ", ".join(parts)


def _path_from_args(args) -> str:
    """Extract a display path from a possibly-malformed ``args`` payload.

    Read/write/edit tool events can nest the path under ``path`` directly, or
    under an ``input`` sub-object. Some tool wrappers serialise the path as a
    positional argument instead, so fall back to ``brief_args`` when no known
    shape matches.
    """
    if not isinstance(args, dict):
        return brief_args(args)
    direct = args.get("path")
    if isinstance(direct, str):
        return direct
    input_block = args.get("input")
    if isinstance(input_block, dict):
        nested = input_block.get("path")
        if isinstance(nested, str):
            return nested
    return brief_args(args)


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
            command = args.get("command", "") if isinstance(args, dict) else args
            print(f"  🔧 bash: {brief(command, 150)}")
        elif tool in ("read", "write", "edit"):
            print(f"  🔧 {tool}: {_path_from_args(args)}")
        elif tool == "search":
            pattern = args.get("pattern", "") if isinstance(args, dict) else args
            print(f"  🔧 search: {brief(pattern, 100)}")
        elif tool == "ast_grep":
            pattern = args.get("pattern", "") if isinstance(args, dict) else args
            print(f"  🔧 ast_grep: {brief(pattern, 60)}")
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
                _as_str(c.get("text")) for c in contents if isinstance(c, dict) and c.get("type") == "text"
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
                _as_str(c.get("text")).strip()
                for c in content
                if isinstance(c, dict) and c.get("type") == "text" and _as_str(c.get("text")).strip()
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
                    if isinstance(c, dict) and c.get("type") == "text":
                        stripped = _as_str(c.get("text")).strip()
                        if stripped:
                            final_texts.append(stripped)

        usage = evt.get("usage", {})
        total_tokens = usage.get("totalTokens", 0) if usage else 0

        suffix = f", {total_tokens} tokens" if total_tokens else ""
        print(f"\n✅ Agent finished ({turn_number} turn{'s' if turn_number != 1 else ''}{suffix})")
        sys.stdout.flush()
