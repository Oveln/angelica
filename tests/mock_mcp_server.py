#!/usr/bin/env python3
"""Mock MCP server speaking JSON-RPC 2.0 over stdio.

Usage: python3 mock_mcp_server.py

Supports:
  - initialize handshake
  - notifications/initialized (ignored)
  - tools/list → returns a single "echo" tool
  - tools/call → echoes back the arguments
"""
import sys
import json
from typing import Dict, Optional


def write_response(msg: Dict) -> None:
    line = json.dumps(msg)
    sys.stdout.write(line + "\n")
    sys.stdout.flush()


def handle_request(msg: Dict) -> Optional[Dict]:
    method = msg.get("method", "")
    params = msg.get("params", {})
    req_id = msg.get("id")

    if method == "initialize":
        return {
            "jsonrpc": "2.0",
            "id": req_id,
            "result": {
                "protocolVersion": "2025-03-26",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "mock-mcp", "version": "0.1.0"},
            },
        }

    if method == "tools/list":
        return {
            "jsonrpc": "2.0",
            "id": req_id,
            "result": {
                "tools": [
                    {
                        "name": "echo",
                        "description": "Echo back the input",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "message": {"type": "string"},
                            },
                            "required": ["message"],
                        },
                    }
                ]
            },
        }

    if method == "tools/call":
        name = params.get("name", "")
        arguments = params.get("arguments", {})
        if name == "echo":
            text = arguments.get("message", "")
            return {
                "jsonrpc": "2.0",
                "id": req_id,
                "result": {
                    "content": [
                        {"type": "text", "text": text},
                    ],
                },
            }
        return {
            "jsonrpc": "2.0",
            "id": req_id,
            "error": {"code": -32601, "message": f"unknown tool: {name}"},
        }

    return {
        "jsonrpc": "2.0",
        "id": req_id,
        "error": {"code": -32601, "message": f"unknown method: {method}"},
    }


def main() -> None:
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue

        if "id" not in msg:
            continue

        resp = handle_request(msg)
        if resp is not None:
            write_response(resp)


if __name__ == "__main__":
    main()
