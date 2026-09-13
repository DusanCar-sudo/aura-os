#!/usr/bin/env python3
"""Send keystrokes (and grab screenshots) to the running VM through the QEMU monitor socket.

    ./vm-keys.py key meta_l-q            one key combo (QEMU sendkey names)
    ./vm-keys.py type 'ls -la' --enter   type text, optionally press Enter
"""
import socket, sys, time
from pathlib import Path

import os
# AURA_VM_SOCK picks another VM (run-iso-test.sh uses isotest.sock)
SOCK = Path(os.environ.get("AURA_VM_SOCK") or Path(__file__).with_name("monitor.sock"))
NAMES = {" ": "spc", "-": "minus", "=": "equal", ".": "dot", ",": "comma",
         "/": "slash", ";": "semicolon", "'": "apostrophe", "[": "bracket_left",
         "]": "bracket_right", "\\": "backslash", "`": "grave_accent"}
SHIFTED = {"!": "1", "@": "2", "#": "3", "$": "4", "%": "5", "^": "6", "&": "7",
           "*": "8", "(": "9", ")": "0", "_": "minus", "+": "equal", ":": "semicolon",
           '"': "apostrophe", "{": "bracket_left", "}": "bracket_right", "|": "backslash",
           "<": "comma", ">": "dot", "?": "slash", "~": "grave_accent"}

def keyname(ch):
    if ch.isalpha():
        return f"shift-{ch.lower()}" if ch.isupper() else ch
    if ch.isdigit():
        return ch
    if ch in SHIFTED:
        return f"shift-{NAMES.get(SHIFTED[ch], SHIFTED[ch])}"
    return NAMES[ch]

def send(keys):
    with socket.socket(socket.AF_UNIX) as s:
        s.connect(str(SOCK))
        for k in keys:
            s.sendall(f"sendkey {k}\n".encode())
            time.sleep(0.04)
        time.sleep(0.2)

def shot(path):
    with socket.socket(socket.AF_UNIX) as s:
        s.connect(str(SOCK))
        s.sendall(f"screendump {Path(path).resolve()} -f png\n".encode())
        time.sleep(1)

if __name__ == "__main__":
    mode, arg = sys.argv[1], sys.argv[2]
    if mode == "shot":           # ./vm-keys.py shot screen.png
        sys.exit(shot(arg))
    keys = [arg] if mode == "key" else [keyname(c) for c in arg]
    if "--enter" in sys.argv:
        keys.append("ret")
    send(keys)
