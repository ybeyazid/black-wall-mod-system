# Originality and interoperability

BWMS is an independent implementation. This page states exactly what that means, because the
project sits next to a PC modding ecosystem whose frameworks are open source, and "looks similar"
deserves a precise answer rather than a vague one.

## What is in this repository

**The runtime is original work.** The dylib, the hooks, the RTTI bridge, the archive and TweakDB
tooling — all written for this project, in Rust. No third-party implementation was translated,
transliterated or adapted into it.

**The redscript layer declares interfaces, and implements its own behaviour.** Files under
`r6/scripts/blackwall-mods/` contain two kinds of line:

- *Signatures and engine facts* — `public native func GetKey() -> EInputKey`, an `enum` that the
  game's own reflection defines, an `@addField` naming a field that already exists in the game's
  RTTI. A mod written for the PC ecosystem calls these names; for that mod to run here, the name
  and the shape have to match. This is the shape of the plug, not the machine behind it, and any
  compatible implementation — however it was written — declares the same thing.
- *Bodies* — the actual behaviour. These are ours. Where a body would have to be someone else's
  work to exist, the file is not shipped at all.

## How that is enforced, not just claimed

`dist/auditoria-originalidade.py` runs as a mandatory gate in **both** the packaging script and
the public-export script. It compares every published `.reds` against the vendored third-party
sources and **fails the build** if any file contains a contiguous block of copied *body*.

Two deliberate choices in that check:

- It looks for **contiguous blocks**, not matching lines. `return true;`, `wrappedMethod();` and
  `let i: Int32 = 0;` appear in every redscript file ever written; a single shared line proves
  nothing. A run of identical lines, in the same order, against the same source file, does.
- It ignores **signatures and engine facts**, and only weighs bodies — for the reason above.

The gate was validated in both directions: it passes on what ships, and it correctly flags a
known-copied set when pointed at one.

## What was removed, and why

In August 2026 an audit of this kind found 31 redscript files in the release candidate that were
literal ports of a third-party framework (MIT-licensed). Rather than ship them with attribution,
they were **removed** — the project's rule is that third-party code does not travel in it, license
permitting or not. Some capability was lost with them; that was the accepted cost. Anything
rewritten later will be written from the behaviour, not from the source.

That first pass cleaned the release. It did not clean the workbench: running the same gate against
the development tree the next day found **29 more files still there**, renamed and regrouped rather
than removed, three of them under names that read as this project's own. They were removed too, and
the gate is now run against the working tree, not only against what leaves it. The rule is that
third-party code does not live here — not merely that it does not ship.

Two of the capability gaps this project tracks had been closed inside those files. They were
reopened. Work that cannot ship is not work that is done.

## Game content

Nothing produced by CD PROJEKT RED is redistributed here: no assets, no extracted scripts, no
symbol dumps, no save data. The tooling reads what is already on the machine of whoever installs
it, from their own copy of the game and their own saves. The export script enforces this with an
allowlist — the default is *not* published — plus a size guard that catches extracted game data
by weight.

`Cyberpunk 2077` and `CD PROJEKT RED` are trademarks of their respective owners. This project is
not affiliated with, endorsed by, or connected to them.
