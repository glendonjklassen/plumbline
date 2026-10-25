Plumbline @VERSION@ — the whole site, to keep or to host

This folder is the complete Plumbline app as served at plumblinebible.org and
plumblinebible.ch: the King James Bible with Strong's numbers and the study
tools, in nine languages, working offline once opened. Nothing else is needed.

To use it on your own computer:

  1. Open a terminal in this folder.
  2. Run:   python3 -m http.server 8000
  3. Open   http://localhost:8000/   in a browser.

Any other static file server does the same. To put it on the web, copy this
folder to any web host and serve it over HTTPS; the app works from any path.
Browsers only allow the offline parts over HTTPS or on localhost, which is why
opening index.html straight from the folder is not enough.

Keep the files exactly as they are. The app loads them by name, and
SHA256SUMS lists every file with its checksum, so a copy can be checked:

  sha256sum --check --quiet SHA256SUMS

The Bible text is public domain. The other data is freely licensed; the
sources and licences are listed in the project's BIBLIOGRAPHY.md, and the
source code is public. Share this folder freely.
