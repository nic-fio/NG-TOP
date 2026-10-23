# ngtop documentation

GitHub shows `.html` files as source code: it does not display web pages stored
in a repository. Read the manuals in one of these two ways.

**Online** (https://ngtop.nicfio.it):

| Document | Link |
|---|---|
| User manual | https://ngtop.nicfio.it/User%20Manual.html |
| Technical manual | https://ngtop.nicfio.it/Technical%20Manual.html |
| Design decisions and history | https://ngtop.nicfio.it/decisions.html |

**Offline**: clone the repository (https://github.com/nic-fio/NG-TOP) and open `docs/User Manual.html` in a browser.
Each manual is one HTML file with its style and scripts inside, so the pages
work without an internet connection.

[Design decisions and history](decisions-and-history.md) is Markdown, so GitHub
shows it formatted here.

## Files

| File | What |
|---|---|
| `index.html` | Start page of the documentation. |
| `User Manual.html` | Using ngtop: the five sections, keys, signals, services, configuration. |
| `Technical Manual.html` | Architecture, build, internals, testing, extending ngtop. |
| `decisions-and-history.md` | Why ngtop is the way it is. |
| `sources/` | Sources of the two manuals; `python3 docs/sources/build.py` generates the HTML files. |
