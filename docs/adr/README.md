# Architecture Decision Records

These records hold accepted decisions. An accepted behavioral target is not
an implemented guarantee until its status says so. Read the status and context
of each record before relying on it.

| ADR  | Decision                                                                                                                                                 | Status                 |
| ---- | -------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------- |
| 0001 | [A published artifact cites its sources, ships as a sanitized export, names its accountable reviewer, and discloses AI use](0001-published-artifacts.md) | Accepted               |
| 0002 | [The changelog tells users what changed; it is not a git log or a task list](0002-changelog-is-for-users.md)                                             | Accepted               |
| 0003 | [An incomplete or failed run never passes as clean](0003-incomplete-runs-never-pass-as-clean.md)                                                         | Accepted (implemented) |
| 0004 | [Moldy formats text and never runs analysis transforms](0004-format-text-without-analysis-transforms.md)                                                 | Accepted               |
| 0005 | [Preserve source line endings unless conversion is requested](0005-preserve-source-line-endings.md)                                                      | Proposed               |
| 0006 | [Moldy normalizes layout, and a project can keep its own conventions](0006-layout-not-authored-style.md)                                                 | Accepted (implemented) |
| 0007 | [Moldy checks that code reads the same; layout follows the reference tool](0007-correctness-is-checked-layout-is-policy.md)                              | Proposed               |

Contributor rules remain in [AGENTS.md](../../AGENTS.md).
