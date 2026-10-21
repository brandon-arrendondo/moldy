#!/usr/bin/env bash
set -euo pipefail
# comment
readonly   NAME="moldy"   # trailing
function greet {
echo "hi $1"
}
greet2 () { echo two; }
if [[ -n "${NAME}" ]] && [ -d /tmp ]
then
    echo ok
elif [ "$x" == y ]; then echo elif
else
  echo no
fi
for f in *.c ;do
  gcc -c "$f"  -o "${f%.c}.o"|| exit 1
done
while read -r line; do echo "$line"; done < input.txt
case "$1" in
  start) run ;;
  stop|halt)
    halt
    ;;
  *) echo "usage";;
esac
x=$(ls -la | grep foo)
y=`date`
arr=(one two   three)
cat <<EOT
heredoc  keeps   spacing
EOT
some_command --flag \
    --other-flag value
{ echo a; echo b; } > out
( cd /tmp && ls )
[ -f x ] || { echo missing; exit 1; }
echo $(( 1+2 ))
cmd1 && cmd2 || cmd3
cmd1 &&
  cmd2


echo done
