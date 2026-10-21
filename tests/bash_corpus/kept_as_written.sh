# A comment inside `[[ ]]` or `(( ))` keeps the statement as written.
[[ a && # c
  b ]]
((x # c
))
# Text the grammar skips keeps its statement as written: a `-` before a
# redirection, and escaped blanks.
cat   - <<EOF
x
EOF
r=$a\ $b
echo f\	   g
echo f\ 
# A statement kept as written that holds a heredoc takes `then` or `do` on
# the next line.
if cmd - <<EOF
x
EOF
then :; fi
if a; then :; elif cmd - <<EOF
x
EOF
then :; fi
until cmd - <<EOF
x
EOF
do :; done
# A redirection the grammar runs on into later lines.
cat a | sed "x" | sed "y" > out
echo done
cat b | sed "w" \
  > out2
awk '{x}' \
  in | sort | head -1
> in
# A line continuation inside a word joins it.
echo a\
b
X=a\
b
ec\
ho a
echo "a"\
"b"
X=\
"value"
echo $(echo a)\
b
echo `echo a`\
b
X=$((1))\
2
cat <(true)\
x
echo a\
`echo b`
echo a\
\
b
