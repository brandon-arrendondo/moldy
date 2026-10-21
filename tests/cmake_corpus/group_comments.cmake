# A parenthesized argument holding a comment takes one argument per line.
set(A (a # c
 b))
set(A ( # c
 a))
list(APPEND L ( # c
 a))
string(REPLACE ( # c
 x) y OUT IN)
# A comment after a trailing bracket comment takes its own line.
set(A b #[[x]] # c
)
if(A #[[x]] # c
)
endif()
# Text touching a group's `)` is a separate argument.
set(A (a # c
 b)y)
set(A ((a # c
 b)y) z(
 # c
 b)z)
if((A # c
)AND B)
endif()
if((A # c
)OR(B))
endif()
set(A (a b)y)
if((A)AND B)
endif()
