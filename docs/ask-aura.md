# Ask Aura (aura-term, Ctrl+Shift+A)

Sends the selection, or else the window's last failed command, to Aura.
Her answer opens in a new window next to the terminal — never inside the
running program. A command she suggests is shown and runs only when the
user presses Enter, in the answer window.

## Flow

1. Ctrl+Shift+A (`aura-ask` action) in aura-termd writes the request to
   `$XDG_RUNTIME_DIR/aura-os/ask/<shell pid>-<n>.json` (mode 0600):

   ```json
   {"selection": "...", "cwd": "/home/dusan/x"}
   {"command": "ls /nope", "output": "ls: cannot access ...", "exit": 2, "cwd": "/home/dusan"}
   ```

   The failed block comes from the shell integration (OSC 133): the
   command text between the B and C marks, the output between C and D
   (last 16 KiB), and D's exit code. Nothing selected and no failed
   command yet: the key does nothing.

2. It runs, via `swaymsg exec` (so sway places it as a split next to the
   focused terminal; a plain spawn outside sway):

       aura-term --app-id=aura-ask --title='Ask Aura' --working-directory=<cwd> \
           aura-term-ask --input <request> --delete-input

   `app_id` `aura-ask` lets sway rules size or float the answer window.

3. `aura-term-ask` (C, no dependencies) reads the request, starts
   `aura sidecar` and speaks its NDJSON protocol (aura-code
   `docs/PROTOCOL.md`): `session.create` → `turn.send` → streams
   `turn.delta` → `turn.completed` → `session.destroy`. The engine's stderr
   goes to `$XDG_RUNTIME_DIR/aura-os/ask/sidecar.log`.

4. The last one-line ```` ```sh ```` block in the answer becomes the
   suggestion: Enter runs it with `sh -c` in the request's cwd, `q` +
   Enter closes without running anything.

`aura-term-ask` also works standalone: `aura-term-ask < request.json`
(`--no-prompt` skips the Enter/close prompts).

## Read-only

- `session.create` with `"permission": "read-only"` and
  `"allowedTools": []` — the session has no tools at all.
- Every `approval.request` is answered `deny` (and shown as denied).
- The session is destroyed after the answer, which removes its stored
  history.

## Without Aura

If `aura` is not in PATH (the VM today), the window says so and prints the
exact prompt that would have been sent.
