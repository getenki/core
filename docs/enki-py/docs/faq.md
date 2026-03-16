---
sidebar_position: 6
slug: /faq
---

# FAQ

## Should I use `Agent` or `EnkiAgent`?

Use `Agent` unless you specifically need to manage raw tool specs and the callback handler yourself.

## Where does the native library live?

In the current repo layout, the generated binding and native library live under `python/enki_py/enki_py/`.

## Why do I see both `EnkiTool` and `EnkiToolSpec`?

`enki_py.__init__` includes compatibility aliases so either name can exist depending on which generated symbol is available.

## Is the docs site wired into CI?

Not yet. This change creates the Docusaurus app and content, but it does not add a CI workflow for building or deploying the site.
