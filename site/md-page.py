#!/usr/bin/env python3
# Copyright (c) 2026 Nicola Fiorillo
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
"""Converts a Markdown file to a page of the site, in the style of licence.html
(it reuses body() and page() of licence-page.py).

    md-page.py FILE.md PATH TITLE DESCRIPTION > out.html

PATH is the page's name on the site (for the canonical link), e.g. decisions.html.
Markdown understood: headings, paragraphs, lists, tables, ``` code blocks,
**bold**, `code`, [links](url) and <https://autolinks>."""
import importlib.util
import os
import sys

here = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("licence_page", os.path.join(here, "licence-page.py"))
lp = importlib.util.module_from_spec(spec)
spec.loader.exec_module(lp)

path, name, title, desc = sys.argv[1:5]
md = open(path, encoding="utf-8").read()
# the first "# " heading becomes the page's h1 and the others move down one level? No: keep levels.
print(lp.page(title, desc, name, lp.body(md)))
