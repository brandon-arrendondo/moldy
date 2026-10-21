matlab -nojvm 1>/dev/null 2>&1 -logfile $LOG <<-EOF
	quit
	EOF
y=`echo "a\$b"`
# The grammar reads a body line starting with `\` as part of the
# redirect, so this statement is kept as written.
cat <<EOF > out.tex
\begin{document}
some $\alpha$ text
\end{document}
EOF
# To bash, a `#` right after a redirection starts a comment, so this line
# is invalid; the grammar instead reads the next line into the
# redirection. Both lines are kept as written.
echo hi >#out.txt
echo two
# The grammar pairs the `{` and `}` in these quoted strings across the
# lines, reading the next lines as words of the first command, so the
# statements are kept as written.
e '{'
c '}'
exit 1
# Bash reads the assignment and the backticks after it as one word.
case $d in
  -L*) p="$p "`echo "X$d" | sed -e x`;; # c
esac
# Two backtick substitutions in one string the grammar pairs wrongly.
line="`hostname` `cat $p`"
# A header comment before a body written on the `do` or `then` line: the
# body moves to its own line, where Prettier keeps some of them inline.
for ((;;)) # c
do b; done
if a; b # c
then x; fi
select i in 1 2 # c
do echo $i; done
# As above, the grammar reads the line after `> #` into the redirection.
> # c
echo b
# A comment on a heredoc line stays on it.
cat <<EOF # c
x
EOF
cat <<EOF | grep x # c
x
EOF
cat <<EOF && echo b # c
x
EOF
# A comment before `in` trails it.
case $x # c
in a) b;; esac
# Header comments after the first take their own lines.
for i in a # c1
# c2
do b; done
# A condition holding a heredoc takes `then` or `do` on the line after the
# heredoc; Prettier writes `<< EOF; then`, which the grammar misreads.
if cmd <<EOF # c
x
EOF
then # d
  :
fi
while cmd <<EOF && b # c
x
EOF
do :; done
# Prettier puts each statement of a condition or substitution holding a
# background `&` on its own line; these stay on one line.
if sleep 1 & true; then echo; fi
while a & b; do c; done
x=$(a & b)
if [ -n "${T}" ] & [ "${T}" != "none" ]; then echo; fi
