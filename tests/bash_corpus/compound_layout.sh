{ echo a; }
{ echo a; echo b; }
f() { a; b; }
if true; then a; fi
if true; then a; b; fi
if true; then a; else b; fi
for i in 1 2; do a; b; done
while true; do a; done
a; b
a & b
x=1 y=2 cmd
if true
then a
fi
echo "a" >&2
echo a 2>/dev/null >out
cat <<-EOT
	tabbed
	EOT
cat <<'EOF2'
$raw
EOF2
case $x in a) ;; esac
echo a | \
  grep b
local -a arr=( a b )
declare -r X=1
[[ $a =~ ^x ]]
(( i++ ))
echo ${#arr[@]} ${x:-def}
! true
time sleep 1
coproc foo
echo 'single' "dbl" $'ansi'
if x; then :; else
  a
  b
fi
if x; then
  a
else b; fi
if x; then a
fi
if x; then a; else
  b
fi
for i in 1; do a
done
while x; do a; done; echo
if x; then a; elif y; then
  b
fi
{ a
}
f() { a
}
# A command run in the background keeps its `&`.
for i in 1; do a & done
{ a & }
f() { a & }
(a &)
if x; then a & fi
case x in a) b & ;; esac
if a; then b & else c & fi
