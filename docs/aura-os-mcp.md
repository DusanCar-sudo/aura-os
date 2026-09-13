# aura-os-mcp — Aura OS for agents

A local MCP server (stdio, Bash + jq). It runs only while an agent is
connected; there is no daemon. Every call is logged with the agent's name
to `~/.local/state/aura-os/mcp.log`.

## Connect an agent
- Claude Code: `claude mcp add --scope user aura-os -- /usr/local/bin/aura-os-mcp`
- Aura: ask her to connect with her `mcp` tool, command `aura-os-mcp`
- Anything else that speaks MCP over stdio: command `aura-os-mcp`

## Tools (phase 1)
read: windows_list, desks_list, system_status, agents_status,
recent_items, apps_list, theme_list
reversible: window_focus, window_move_to_desk, window_float, desk_switch,
app_open (.desktop ids only), url_open (http/https only), tab_rename,
tab_color, theme_apply, wallpaper_set, sidebar_toggle, notify

## Not in phase 1, on purpose
No shell/command tool. Closing windows (unsaved work), reading the
clipboard (passwords), packages, snapshots and reboot come in phase 2,
behind a confirmation the user clicks.

## Tested (2026-09-13, VM)
Scripted session: 19 tools listed; `run_shell` → unknown tool; URL with
`;touch /tmp/pwned` rejected (file not created); app id
`../../etc/passwd` rejected; whole session 0.39 s. Real Claude Code:
`claude mcp list` → connected; `claude -p` answered desks + RAM through
desks_list and system_status.
