set -eu
target=$1
remote_command=$2
skip_host_key=$3
shift 3
if [ "$skip_host_key" = yes ]; then
    ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o GlobalKnownHostsFile=/dev/null -o ConnectTimeout=15 -- "$target" "$remote_command"
else
    ssh -o StrictHostKeyChecking=yes -o ConnectTimeout=15 -- "$target" "$remote_command"
fi
exec "$@"
