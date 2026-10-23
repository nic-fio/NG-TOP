# Planned chapters (PIANO.md, phase B)

Build: `python3 docs/sources/build.py [user|technical]` · check: `python3 docs/sources/build.py --check`.
Each chapter is a file `chNN_name.py` exposing `CHAPTER = (title, [(section_title, html), ...])`,
using the helpers of `build.py` (p, h4, ul, ol, steps, note, warn, tip, rif, table, dl, code, term, tree,
box, arrow, path, text, zone, fig, flow, seq, c, sysf, key, ui, pill, file_map, numeri, conta, snap,
keys_table, options_table).

User manual (`user/`)
 1. Introduction · 2. First steps (requirements, start, recommended terminal) · 3. The screen (sections,
 keys, mouse, compact format, help) · 4. Overview · 5. GPU · 6. Processes (tree, details, signals and
 their safety, nice) · 7. Network and disks · 8. Services (actions, password, sudo) · 9. Configuration
 (options, file, W) · 10. Permissions and running as root · 11. Troubleshooting · 12. Keys reference
 (generated) · 13. Options reference (generated) · 14. Glossary and index

Technical manual (`technical/`)
 1. Overview and numbers · 2. Source tree and tools · 3. Architecture (diagrams) · 4. Data sources:
 what is read from /proc and /sys and with which formulas · … (see PIANO.md)
