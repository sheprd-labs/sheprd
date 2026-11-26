# Runs on the terminal host. The caller validates the pane ID and passes the
# serialized recipe as one literal argument. Only Sheprd's managed panes call it.
set -eu
dir=${XDG_STATE_HOME:-"${HOME:?}/.local/state"}/herdr/acp/restore/tmux
file=$dir/$2
case $1 in
    forget) rm -f "$file.chat" "$file.shell"; exit ;;
    close-chat) rm -f "$file.chat"; exit ;;
    read|claim)
        if [ -f "$file.chat" ]; then file=$file.chat
        elif [ -f "$file.shell" ]; then file=$file.shell
        elif [ "$1" = read ]; then exit 0
        else exit 1; fi ;;
    shell|chat) file=$file.$1 ;;
    # A shell that moved keeps its new folder; a chat or a closed pane has no
    # shell recipe to update.
    cwd) file=$file.shell; [ -f "$file" ] || exit 0 ;;
    *) exit 1 ;;
esac
if [ -r /proc/sys/kernel/random/boot_id ]; then
    IFS= read -r boot < /proc/sys/kernel/random/boot_id
else
    boot=$(/usr/sbin/sysctl -n kern.bootsessionuuid)
fi
[ -n "$boot" ]
case $1 in
    read|claim)
        [ "$(wc -c < "$file")" -le 16384 ]
        exec 3< "$file"
        IFS= read -r saved <&3
        if [ "$boot" = "$saved" ]; then [ "$1" = read ]; exit; fi
        if [ "$1" = read ]; then head -c 16385 <&3; exit; fi
        # A concurrent close or provider switch invalidates this launch.
        [ "$(head -c 16385 <&3)" = "$3" ] ;;
esac
# An attempted recovery is stamped before exec. A provider that fails to
# resume must not be restarted repeatedly by a reconnecting view.
umask 077
mkdir -p "$dir"
tmp=$(mktemp "$file.XXXXXXXX")
trap 'rm -f "$tmp"' EXIT
printf '%s\n%s\n' "$boot" "$3" > "$tmp"
mv -f "$tmp" "$file"
