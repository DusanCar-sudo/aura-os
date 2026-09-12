# Task 002 — voice desktop control

Depends on task 001 (install skeleton, Super+A prompt).

## Goal
Dusan speaks, Aura runs the desktop:

    "aura, open chrome on the extended screen and go to ibm.com"
    "aura, tile these windows"            "aura, move slack to screen one"
    "aura, close everything on workspace 3"

## Pipeline
1. Push-to-talk: hold Super+Space (hyprland `bind` / `bindr`). Later,
   optional wake word "aura".
2. Speech to text: whisper.cpp, local, small model. Dusan speaks
   Serbian and English — support both.
3. Aura turns the sentence into ONE or more aura-os-* calls. No raw
   shell. Unknown request → ask back, do nothing.
4. The aura-os-* command runs `hyprctl` and prints what it did.
5. Reply: short spoken confirmation (piper TTS) + a mako notification.

## Deliver
- `bin/aura-os-window` — open <app> [--monitor N|primary|external]
  [--workspace N] [--url U]; move, focus, close, tile, float,
  fullscreen. Monitor names come from `hyprctl monitors -j`, never
  hardcoded.
- `bin/aura-os-listen` — record while key held, transcribe, hand the
  text to Aura, speak the reply.
- `config/hypr/voice.conf` — the push-to-talk binding.
- `tests/voice-phrases.txt` — 20 example sentences (SR + EN) with the
  exact aura-os-window call each should produce. Aura's output must
  match all 20 before this task is done.

## Rules
- Window commands need no snapshot (they change nothing on disk).
- Latency target: under 2 s from releasing the key to the action.
- The VM has one screen: fake a second with
  `hyprctl output create headless` to test "extended screen".
