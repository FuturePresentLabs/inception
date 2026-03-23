#!/usr/bin/env python3
"""
Inception Skill - Manage Inception Registry sessions
"""

import argparse
import json
import os
import sys
from typing import Optional
import urllib.request
import urllib.error

# Default configuration
DEFAULT_REGISTRY_URL = "http://localhost:18080"


def get_config() -> dict:
    """Load config from file."""
    config_file = os.path.expanduser("~/.config/inception/config.json")
    if os.path.exists(config_file):
        try:
            with open(config_file, 'r') as f:
                return json.load(f)
        except:
            pass
    return {}


def get_registry_url() -> str:
    """Get registry URL from environment, config file, or default."""
    return os.environ.get("INCEPTION_REGISTRY_URL") or get_config().get('registry_url') or DEFAULT_REGISTRY_URL


def get_token() -> str:
    """Get auth token from environment or config file."""
    return os.environ.get("INCEPTION_TOKEN") or get_config().get('token') or ""


def api_request(path: str, method: str = "GET", data: Optional[dict] = None) -> dict:
    """Make API request to registry."""
    url = f"{get_registry_url()}{path}"
    headers = {
        "Content-Type": "application/json",
    }
    
    token = get_token()
    if token:
        headers["Authorization"] = f"Bearer {token}"
    
    req = urllib.request.Request(
        url,
        method=method,
        headers=headers,
    )
    
    if data:
        req.data = json.dumps(data).encode("utf-8")
    
    try:
        with urllib.request.urlopen(req) as response:
            return json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as e:
        print(f"Error: {e.code} - {e.reason}", file=sys.stderr)
        try:
            body = json.loads(e.read().decode("utf-8"))
            print(f"Details: {body}", file=sys.stderr)
        except:
            pass
        sys.exit(1)
    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        sys.exit(1)


def cmd_list(args):
    """List all sessions."""
    sessions = api_request("/v1/sessions")
    
    if not sessions:
        print("No sessions found.")
        return
    
    # Filter by status if provided
    if args.status:
        sessions = [s for s in sessions if s.get("status") == args.status]
    
    print(f"{'ID':<20} {'Status':<12} {'Agent State':<15} {'Task':<30}")
    print("-" * 80)
    
    for session in sessions:
        session_id = session.get("id", "unknown")[:18]
        status = session.get("status", "unknown")
        agent_state = session.get("agent_state") or "idle"
        task = (session.get("current_task") or "")[:28]
        
        print(f"{session_id:<20} {status:<12} {agent_state:<15} {task:<30}")


def cmd_get(args):
    """Get session details."""
    session = api_request(f"/v1/sessions/{args.session_id}")
    
    print(f"Session: {session['id']}")
    print(f"Status: {session['status']}")
    print(f"Agent Type: {session['agent_type']}")
    print(f"Agent State: {session.get('agent_state') or 'idle'}")
    print(f"Current Task: {session.get('current_task') or 'None'}")
    
    if session.get('progress') is not None:
        print(f"Progress: {session['progress'] * 100:.0f}%")
    
    print(f"Capabilities: {', '.join(session.get('capabilities', []))}")
    print(f"Created: {session['created_at']}")
    print(f"Last Activity: {session.get('last_activity', 'N/A')}")
    
    if session.get('metadata'):
        print(f"Metadata: {json.dumps(session['metadata'], indent=2)}")


def cmd_attach(args):
    """Attach to a session."""
    if args.session_id:
        # Attach to existing
        session = api_request(f"/v1/sessions/{args.session_id}")
        print(f"Attached to session: {session['id']}")
    else:
        # Create new
        data = {
            "agent_type": "claude_code",
            "capabilities": ["rust", "python", "typescript"],
        }
        session = api_request("/v1/sessions", method="POST", data=data)
        print(f"Created and attached to session: {session['id']}")
    
    print(f"Status: {session['status']}")
    print(f"WebSocket: {session.get('websocket_url', 'N/A')}")


def cmd_status(args):
    """Show current status."""
    sessions = api_request("/v1/sessions")
    
    # Count by status
    status_counts = {}
    agent_states = {}
    
    for session in sessions:
        status = session.get("status", "unknown")
        status_counts[status] = status_counts.get(status, 0) + 1
        
        agent_state = session.get("agent_state") or "idle"
        agent_states[agent_state] = agent_states.get(agent_state, 0) + 1
    
    print(f"Registry: {get_registry_url()}")
    print(f"Total Sessions: {len(sessions)}")
    print()
    print("By Status:")
    for status, count in sorted(status_counts.items()):
        print(f"  {status}: {count}")
    print()
    print("By Agent State:")
    for state, count in sorted(agent_states.items()):
        print(f"  {state}: {count}")


def cmd_update_status(args):
    """Update session status."""
    # Get current session to preserve status if not provided
    current = api_request(f"/v1/sessions/{args.session_id}")
    
    data = {
        "status": args.status or current.get("status", "idle")
    }
    
    if args.state:
        data["agent_state"] = args.state
    if args.progress is not None:
        data["progress"] = args.progress
    
    session = api_request(
        f"/v1/sessions/{args.session_id}/status",
        method="POST",
        data=data
    )
    
    # Update task if provided
    if args.task:
        api_request(
            f"/v1/sessions/{args.session_id}",
            method="PATCH",
            data={"current_task": args.task}
        )
    
    print(f"Updated session: {session['id']}")
    print(f"Status: {session['status']}")
    print(f"Agent State: {session.get('agent_state') or 'idle'}")
    if session.get('progress') is not None:
        print(f"Progress: {session['progress'] * 100:.0f}%")


def cmd_configure(args):
    """Configure registry URL and token."""
    config_dir = os.path.expanduser("~/.config/inception")
    config_file = os.path.join(config_dir, "config.json")
    
    # Load existing config
    config = {}
    if os.path.exists(config_file):
        try:
            with open(config_file, 'r') as f:
                config = json.load(f)
        except:
            pass
    
    # Update config
    if args.url:
        config['registry_url'] = args.url
    if args.token:
        config['token'] = args.token
    
    # Ensure directory exists
    os.makedirs(config_dir, exist_ok=True)
    
    # Save config
    with open(config_file, 'w') as f:
        json.dump(config, f, indent=2)
    
    print(f"Configuration saved to {config_file}")
    print(f"Registry URL: {config.get('registry_url', DEFAULT_REGISTRY_URL)}")
    print(f"Token: {'[set]' if config.get('token') else '[not set]'}")
    print()
    print("To load this config in your shell:")
    print(f"  eval $(inception env)")


def cmd_env(args):
    """Output shell commands to set environment variables."""
    config = get_config()
    
    if config.get('registry_url'):
        print(f"export INCEPTION_REGISTRY_URL=\"{config['registry_url']}\"")
    if config.get('token'):
        print(f"export INCEPTION_TOKEN=\"{config['token']}\"")


def main():
    parser = argparse.ArgumentParser(
        description="Manage Inception Registry sessions",
        prog="inception"
    )
    subparsers = parser.add_subparsers(dest="command", help="Commands")
    
    # list
    list_parser = subparsers.add_parser("list", help="List sessions")
    list_parser.add_argument("--status", help="Filter by status")
    
    # get
    get_parser = subparsers.add_parser("get", help="Get session details")
    get_parser.add_argument("session_id", help="Session ID")
    
    # attach
    attach_parser = subparsers.add_parser("attach", help="Attach to session")
    attach_parser.add_argument("session_id", nargs="?", help="Session ID (optional)")
    
    # status
    subparsers.add_parser("status", help="Show registry status")
    
    # update-status
    update_parser = subparsers.add_parser("update-status", help="Update session status")
    update_parser.add_argument("session_id", help="Session ID")
    update_parser.add_argument("--state", help="Agent state (idle, thinking, executing, waiting_for_user, error)")
    update_parser.add_argument("--progress", type=float, help="Progress (0.0 to 1.0)")
    update_parser.add_argument("--status", help="Session status (spawning, idle, busy, disconnected, terminated)")
    update_parser.add_argument("--task", help="Current task description")
    
    # configure
    config_parser = subparsers.add_parser("configure", help="Configure registry URL and token")
    config_parser.add_argument("--url", help="Registry URL")
    config_parser.add_argument("--token", help="Auth token")
    
    # env
    subparsers.add_parser("env", help="Output shell environment commands")
    
    args = parser.parse_args()
    
    if not args.command:
        parser.print_help()
        sys.exit(1)
    
    commands = {
        "list": cmd_list,
        "get": cmd_get,
        "attach": cmd_attach,
        "status": cmd_status,
        "update-status": cmd_update_status,
        "configure": cmd_configure,
        "env": cmd_env,
    }
    
    commands[args.command](args)


if __name__ == "__main__":
    main()
