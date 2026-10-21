Escapes are never added: | `no*` |

| a | b |
|---|---|
| no* | x |

```json
{"json":"fences stay verbatim"}
```

- an item whose code span `wraps
  across lines` keeps its indentation

```sh
fenced
```
> a quote after a closed fence is a new top-level block

Tab-indented code keeps its tabs:

	one
	two
