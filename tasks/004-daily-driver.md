# Task 004 — daily driver: all apps, multimedia, games, windows, screens

Depends on 001b. AURA.md "Daily driver": Aura OS must run everything
Dusan runs today on Ubuntu. The RAM budget is for the bare desktop;
never drop compatibility to meet it.

## Apps — install and configure all; test only what "Done when" lists
- Chrome: native Wayland (`--ozone-platform-hint=auto` in a
  chrome-flags.conf), YouTube 1080p with hardware decode (VA-API:
  mesa / intel-media-driver), screen share in Meet via
  xdg-desktop-portal-wlr + pipewire.
- Dev: git, docker (+ compose, user in docker group), node via nvm,
  python via uv, rust via rustup, VS Code / Zed, Android SDK tools.
- Unity Hub + editor, Blender, OBS (pipewire capture), Steam — these
  may need Xwayland (lazy, per 001b) — test at least one X11 app.
- Slack, Telegram, Thunderbird; flatpak enabled as the fallback for
  anything not in pacman/AUR. AUR helper: paru.
- Keyboard layouts: us + Serbian latin + Serbian cyrillic, toggle
  with Alt+Shift (Super+Space is reserved for voice, task 002).
- Clipboard (wl-clipboard + cliphist), screenshots (grim + slurp →
  Print key), Bluetooth and Wi-Fi via TUIs (bluetuith, impala/nmtui).
- Multimedia: full codecs (ffmpeg, gst-plugins good/bad/ugly/libav),
  mpv + VLC, imv for images, Spotify, Kdenlive, DaVinci Resolve
  (hardware-only test), GIMP, Krita, Inkscape, Audacity, EasyEffects.
  Volume/devices via a TUI mixer (wiremix or pulsemixer) + media keys
  (playerctl, volume, brightness) working everywhere.
- Games: multilib + 32-bit Vulkan/mesa, Steam with Proton, Heroic and
  Lutris, gamemode, MangoHud, controllers (Xbox/PS via Bluetooth and
  USB). Fullscreen games: sway `allow_tearing`, idle inhibit, no
  compositor overhead. Test one Proton game on the laptop.
- Every app format: pacman, AUR, flatpak, AppImage (fuse2), Wine +
  Bottles for Windows apps, Waydroid for Android apps. Anything Dusan
  now runs as a snap on Ubuntu gets a flatpak/AUR equivalent — list
  the mapping (`snap list` on the host is read-only, allowed).
- Install all of it through `aura-os-install`; add `aura-os-app
  <name>` for things needing more than a package (flags, groups).

## Many windows
- Any number of windows per workspace: tiled, tabbed (Super+T) or
  stacked; dialogs and popups float automatically (for_window rules
  for pop-ups, file pickers, Picture-in-Picture).
- Super+Tab: fuzzel list of all open windows across workspaces,
  pick one to jump to it.
- Scratchpad: Super+` hides/shows a floating terminal.
- New windows of an app open where the focused pane is, never on a
  random workspace; nothing steals focus.

## Many screens
- Laptop + external monitors, hotplug without a restart: kanshi
  profiles (laptop-only, docked, projector).
- Each monitor has its own workspaces/tabs in the top bar.
- Super+Shift+Alt+←→ moves a window to the next monitor;
  Super+Ctrl+Alt+←→ moves the whole workspace.
- Test with a second headless output (`swaymsg create_output`).

## Done when
Test in the VM only the normal stuff:
- Chrome: opens natively on Wayland, loads ibm.com, plays a YouTube
  video (software decode is fine in the VM).
- Terminal system: foot, git, docker run hello-world, node, python
  (uv), clipboard copy/paste, screenshot key, a TUI mixer.
- Audio: actual sound out of the speakers (not just the mixer UI
  showing a live device) — play a short local file or `speaker-test`,
  confirm it's audible. "Done" does not count without this.
- Windows: 6+ windows on one workspace tiled/tabbed, a pop-up floats,
  Super+Tab jumps between them, one window moved to a second
  (headless) monitor.
Everything else (media apps, games, Wine, Waydroid, Unity, OBS):
installed and configured, NOT tested now — list it as "untested".
Bench idle number still under 350 MB with no apps open.
