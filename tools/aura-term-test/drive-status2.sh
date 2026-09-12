#!/bin/sh
# drive-status2.sh: write-only-on-change, worktree/detached branch, JSON validity
eval $(~/aura-term-test/hl.sh env)
export LANG=C.UTF-8
P=$XDG_RUNTIME_DIR/aura-os/panes
R=~/aura-term-test/repo
rm -rf $R ~/aura-term-test/wt && git init -q -b main $R && git -C $R -c user.name=t -c user.email=t@t commit -q --allow-empty -m init
git -C $R worktree add -q -b feature ~/aura-term-test/wt
pgrep -x aura-termd >/dev/null || { setsid -f aura-termd --server --log-level=warning >$XDG_RUNTIME_DIR/termd.log 2>&1; sleep 0.5; }
cd $R; setsid -f aura-term -e bash --rcfile ~/aura-term-test/bashrc </dev/null >/dev/null 2>&1; sleep 1.5
t() { wtype -s 300 -d 15 "$1" -k Return; sleep ${2:-0.6}; }
f() { ls $P/*.json; }
i1=$(stat -c %i $(f)); t ""; t ""; i2=$(stat -c %i $(f))
echo "empty Enter x2 (OSC 7 resent, no change): inode $i1 -> $i2 $( [ "$i1" = "$i2" ] && echo UNCHANGED || echo REWRITTEN)"
t "cd ~/aura-term-test/wt"; echo "worktree: $(cat $(f))"
t "git -C ~/aura-term-test/wt checkout -q --detach"; t ""; echo "detached: $(cat $(f))"
python3 -c "import json,sys; d=json.load(open(sys.argv[1])); print('valid JSON, keys:', sorted(d))" $(f)
stat -c 'dir perms: %a %n' $XDG_RUNTIME_DIR/aura-os $P
t "exit" 1
