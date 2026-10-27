#!/usr/bin/env python3
"""Runs things inside the win11 VM through the QEMU guest agent.
  vm-guest.py run "<PowerShell>" [timeout_s]
  vm-guest.py put LOCAL_DIR GUEST_DIR          copy a folder into Windows
  vm-guest.py install-driver [LOCAL_DIR]       copy the Creative driver in and install it"""
import base64, json, os, subprocess, sys, time
DOMAIN = "win11"

def agent(cmd, timeout=60):
    out = subprocess.run(["virsh", "-c", "qemu:///system", "qemu-agent-command", DOMAIN,
                          "--timeout", str(timeout), json.dumps(cmd)],
                         capture_output=True, text=True, timeout=timeout + 10)
    if out.returncode:
        raise SystemExit(out.stderr.strip())
    return json.loads(out.stdout)["return"]

def run(ps, timeout=300):
    pid = agent({"execute": "guest-exec", "arguments": {
        "path": "powershell.exe", "arg": ["-NoProfile", "-NonInteractive", "-Command", ps],
        "capture-output": True}})["pid"]
    t = time.time()
    while True:
        st = agent({"execute": "guest-exec-status", "arguments": {"pid": pid}})
        if st.get("exited"):
            break
        if time.time() - t > timeout:
            raise SystemExit("guest command timed out")
        time.sleep(0.5)
    out = base64.b64decode(st.get("out-data", "")).decode(errors="replace")
    err = base64.b64decode(st.get("err-data", "")).decode(errors="replace")
    return st.get("exitcode", 0), out, err

def put_file(local, remote):
    h = agent({"execute": "guest-file-open", "arguments": {"path": remote, "mode": "wb"}})
    with open(local, "rb") as f:
        while chunk := f.read(48 * 1024):
            agent({"execute": "guest-file-write", "arguments": {
                "handle": h, "buf-b64": base64.b64encode(chunk).decode()}})
    agent({"execute": "guest-file-close", "arguments": {"handle": h}})

def put_dir(local, remote):
    for root, _, files in os.walk(local):
        rel = os.path.relpath(root, local)
        gdir = remote if rel == "." else remote + "\\" + rel.replace("/", "\\")
        run(f"New-Item -ItemType Directory -Force -Path '{gdir}' | Out-Null")
        for name in files:
            put_file(os.path.join(root, name), gdir + "\\" + name)
            print("copied", os.path.join(rel, name))

if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else ""
    if cmd == "run":
        code, out, err = run(sys.argv[2], int(sys.argv[3]) if len(sys.argv) > 3 else 300)
        print(out, end=""); print(err, end="", file=sys.stderr); sys.exit(code or 0)
    elif cmd == "put":
        put_dir(sys.argv[2], sys.argv[3])
    elif cmd == "install-driver":
        here = os.path.dirname(os.path.abspath(__file__))
        src = sys.argv[2] if len(sys.argv) > 2 else os.path.join(here, "build", "driver", "x")
        put_dir(src, r"C:\sbcap\driver")
        code, out, err = run(r"pnputil /add-driver C:\sbcap\driver\ctHda.inf /subdirs /install", 600)
        print(out, err)
        sys.exit(code or 0)
    else:
        print(__doc__); sys.exit(2)
