#!/usr/bin/env python3
"""A scripted ACP agent for Sheprd's screencasts: it speaks the Agent Client
Protocol over stdio and, whatever it is asked, streams one rehearsed turn that
reads a file, edits it for real, and runs the tests. Never a real agent."""
import json, os, sys, time

# The checkout the chat runs in (a worktree of it, too), else the demo repo.
REPO = os.getcwd() if os.path.exists("forecast.py") else os.path.expanduser("~/code/forecast")
FILE = os.path.join(REPO, "forecast.py")
OLD = '''def show(days):
    for day in days:
        print(f"{day['date']}  {day['high']}°/{day['low']}°  {day['sky']}")
'''
NEW = '''def show(days, as_json=False):
    if as_json:
        print(json.dumps(days, indent=2))
        return
    for day in days:
        print(f"{day['date']}  {day['high']}°/{day['low']}°  {day['sky']}")
'''

def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()

def update(sid, u):
    send({"jsonrpc": "2.0", "method": "session/update",
          "params": {"sessionId": sid, "update": u}})

def say(sid, text, kind="agent_message_chunk", pace=0.035):
    for word in text.split(" "):
        update(sid, {"sessionUpdate": kind, "content": {"type": "text", "text": word + " "}})
        time.sleep(pace)

def prompt_text(params):
    parts = params.get("prompt") or []
    return " ".join(p.get("text", "") for p in parts if isinstance(p, dict))


SHEPRD = os.path.expanduser("~/Apps/Sheprd.app/Contents/MacOS/sheprd")
# Sheprd opens local pages only outside hidden folders, and worktrees live
# under ~/.herdr: the page is always the main checkout's.
PAGE = os.path.expanduser("~/code/forecast/index.html")


def preview(sid):
    """Opens the demo page in a Sheprd browser tab, tied to this chat, so the
    notes made on it come back here."""
    say(sid, "Opening the page so you can look at it.", "agent_thought_chunk", 0.02)
    update(sid, {"sessionUpdate": "tool_call", "toolCallId": "p1", "title": "sheprd browser open index.html",
                 "kind": "execute", "status": "in_progress"})
    os.system(f"{SHEPRD} browser open {PAGE} >/tmp/screencast-browser.log 2>&1")
    time.sleep(0.6)
    update(sid, {"sessionUpdate": "tool_call_update", "toolCallId": "p1", "status": "completed"})
    say(sid, "It's open beside us. **Annotate** the page and send me your notes.\n\n")
    # Wait for the notes as an agent does, then act on them in the same turn.
    update(sid, {"sessionUpdate": "tool_call", "toolCallId": "p2", "title": "sheprd browser feedback --wait",
                 "kind": "execute", "status": "in_progress"})
    notes = os.popen(f"{SHEPRD} browser feedback --wait 240 2>/dev/null").read()
    update(sid, {"sessionUpdate": "tool_call_update", "toolCallId": "p2", "status": "completed",
                 "content": [{"type": "content", "content": {"type": "text",
                              "text": "1 note: Make this button green"}}]})
    if notes.strip():
        feedback(sid)


def feedback(sid):
    """Acts on notes sent from the page: turns the button green, then reloads."""
    say(sid, "Your note is on the Get daily updates button.", "agent_thought_chunk", 0.02)
    old, new = "background: #8a8f98;", "background: #2f9e5b;"
    update(sid, {"sessionUpdate": "tool_call", "toolCallId": "f1", "title": "Edit index.html",
                 "kind": "edit", "status": "in_progress", "locations": [{"path": PAGE}],
                 "content": [{"type": "diff", "path": PAGE, "oldText": old, "newText": new}]})
    source = open(PAGE).read()
    open(PAGE, "w").write(source.replace(old, new))
    time.sleep(0.5)
    update(sid, {"sessionUpdate": "tool_call_update", "toolCallId": "f1", "status": "completed"})
    os.system(f"{SHEPRD} browser reload >/dev/null 2>&1")
    say(sid, "Done: the button is green now, and the page has reloaded.")


def turn(sid):
    say(sid, "Let me look at how the forecast is printed.", "agent_thought_chunk", 0.02)
    update(sid, {"sessionUpdate": "tool_call", "toolCallId": "t1", "title": "Read forecast.py",
                 "kind": "read", "status": "in_progress", "locations": [{"path": FILE}]})
    time.sleep(0.6)
    update(sid, {"sessionUpdate": "tool_call_update", "toolCallId": "t1", "status": "completed"})
    say(sid, "`show()` prints a table. I'll add an `as_json` switch and a `--json` flag.\n\n")
    source = open(FILE).read()
    update(sid, {"sessionUpdate": "tool_call", "toolCallId": "t2", "title": "Edit forecast.py",
                 "kind": "edit", "status": "in_progress", "locations": [{"path": FILE}],
                 "content": [{"type": "diff", "path": FILE, "oldText": OLD, "newText": NEW}]})
    time.sleep(0.5)
    if OLD in source:
        source = source.replace(OLD, NEW).replace(
            '    show(fetch(args.city))',
            '    show(fetch(args.city), as_json=args.json)').replace(
            '    parser.add_argument("city")',
            '    parser.add_argument("city")\n    parser.add_argument("--json", action="store_true")')
        open(FILE, "w").write(source)
    update(sid, {"sessionUpdate": "tool_call_update", "toolCallId": "t2", "status": "completed"})
    update(sid, {"sessionUpdate": "tool_call", "toolCallId": "t3", "title": "python3 -m unittest",
                 "kind": "execute", "status": "in_progress"})
    time.sleep(0.8)
    update(sid, {"sessionUpdate": "tool_call_update", "toolCallId": "t3", "status": "completed",
                 "content": [{"type": "content", "content": {"type": "text", "text": "Ran 4 tests in 0.002s\n\nOK"}}]})
    say(sid, "Done. `forecast paris --json` now prints the days as JSON, and **all 4 tests pass**.")

for line in sys.stdin:
    try:
        msg = json.loads(line)
    except ValueError:
        continue
    method, mid = msg.get("method"), msg.get("id")
    if method == "initialize":
        send({"jsonrpc": "2.0", "id": mid, "result": {"protocolVersion": 1, "agentCapabilities": {}}})
    elif method == "session/new":
        send({"jsonrpc": "2.0", "id": mid, "result": {"sessionId": "s1"}})
    elif method == "session/prompt":
        text = prompt_text(msg["params"]).lower()
        sid = msg["params"]["sessionId"]
        # Prompts arrive with Sheprd's own context around them, so only the
        # demo's preview request is matched; notes come through the wait.
        if "show me the page preview" in text:
            preview(sid)
        else:
            turn(sid)
        send({"jsonrpc": "2.0", "id": mid, "result": {"stopReason": "end_turn"}})
    elif mid is not None:
        send({"jsonrpc": "2.0", "id": mid, "error": {"code": -32601, "message": "Method not found"}})
