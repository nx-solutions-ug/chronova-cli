---
# find and search paths must be arrays

When calling `find` or `search` tools, the `paths` parameter MUST be an array of strings, never a single string.

Correct: `find(paths=["src/**"])`
Correct: `search(pattern="foo", paths=["src", "tests"])`
Wrong: `find(paths="src/**")`
Wrong: `search(pattern="foo", paths="src")`

This applies to all tools that accept a `paths` parameter. Passing a string causes a validation error that wastes turns.
---