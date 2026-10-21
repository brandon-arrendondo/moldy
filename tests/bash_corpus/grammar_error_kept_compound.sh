if [ $# -gt 1 ]
then
	start
	list "$x" \
	| f1 \
	| f2 \
	| f3
else
	a=$(mk) || exit 1
	b=$(mk) || exit 1
fi
