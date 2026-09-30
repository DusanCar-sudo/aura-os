# Third-party software in Aura OS

Aura adOS's own code is MIT licensed (see LICENSE). The ISO also contains
open-source software owned by others. Each ISO release must meet their
terms. This is the checklist the ISO build (task 007) enforces.

## In this repository

| Path | Upstream | License | Obligation |
|---|---|---|---|
| `term/` (aura-term) | foot, © Daniel Eklöf and contributors | MIT | Keep `term/LICENSE` and foot's copyright notice in the source and in every build (install to `/usr/share/licenses/aura-term/`). Aura's own changes to it are MIT too (see LICENSE). |
| `assets/fonts/` | Audiowide (Astigmatic), Michroma (Vernon Adams), Share Tech Mono (Carrois) — Google Fonts | SIL OFL 1.1 | Keep each `OFL-*.txt` next to its font. Fonts may ship inside a commercial product but may not be sold on their own. They were drawn into the earlier boot splash (`tools/make-splash.py`). The current splash (`assets/splash/aura-splash.png`) is the ∞K adOS lockup plus Press Start 2P (CodeMan38, SIL OFL 1.1). Rendered images carry no license obligation. |

## Bundled in the ISO (Arch Linux packages)

Everything installed from Arch repositories keeps its own license;
Arch places the texts in `/usr/share/licenses/<pkg>/` and the ISO must
keep them. Notable groups:

- **GPL-2.0 / GPL-3.0** (Linux kernel, bash, coreutils, GRUB/systemd-boot
  parts, util-linux, ...): distributing binaries obliges us to provide
  the **complete corresponding source**. Per ISO release, publish a
  source archive (the exact PKGBUILDs + source tarballs of every
  GPL/LGPL package in the ISO) at the same place as the ISO, or ship a
  written offer valid for 3 years.
- **LGPL** (glibc, pipewire parts, gtk, ...): same source obligation for
  the library itself.
- **MIT / BSD / ISC** (sway, wlroots, waybar, mako, fuzzel, ...): keep
  the notices.
- **MPL-2.0** (Firefox): keep notices; source of the MPL files is
  available from Mozilla / Arch (we don't modify them).
- **Proprietary firmware** (linux-firmware blobs): redistributable under
  their own terms; keep `/usr/share/licenses/linux-firmware/`.
- **aura-code** (npm) — Dusan's own package, under its own terms.

## Names and marks

"Based on Arch Linux" is allowed wording. Do not use the Arch Linux
logo or imply Aura OS is an official Arch product. Aura OS's design
follows its own instincts and current trends; it includes no code from
other distributions.

*This file is a working checklist, not legal advice.*
