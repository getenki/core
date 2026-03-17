---
sidebar_position: 6
slug: /faq
---

# FAQ

## How do I install the package?

Use `pip install enki-py` or `uv add enki-py`.

For the published package and release metadata, see https://pypi.org/project/enki-py/

## Should I use `Agent` or `EnkiAgent`?

Use `Agent` unless you specifically need to manage raw tool specs and the callback handler yourself.

## Why do I see both `EnkiTool` and `EnkiToolSpec`?

`enki_py.__init__` includes compatibility aliases so either name can exist depending on which generated symbol is available.

## Do I need the low-level API?

No. Most users should start with `Agent` and decorator-based tools from [Getting Started Guide](/docs/agent-wrapper).
