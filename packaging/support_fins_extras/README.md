# STL Editor's extras for Support Fins

`packaging/update_support_fins.py` copies these files next to Support Fins' own plugin entry
(`plugins/shared/engine/` in their repository) and bundles them with it. They call the
website's own modules, unchanged, for features the plugin entry doesn't offer yet:

| file | what | from Support Fins |
|---|---|---|
| `stle_draw_through.js` | curved hand-drawn walls (generated from `draw_entry.js`) | `drawnWall` with `opts.through` |
| `stle_gusset.js` | gussets: a fin off a side of the part, not the plate | `web/gusset.js` `gussetAt` |
| `stle_anchor.js` | anchor fins: a hand-placed rib against a side or a back | `web/sway.js` `swayAtFace` (`anchor`) |
| `stle_orient.js` | suggested print orientations, the strength verdicts and the strongest pose for a load | `web/orient.js` |

Each extra is built and tried on its own. One that a new Support Fins release breaks is left
out (`VERSION.json` lists the `extras` that made it), and the app falls back or hides the tool.
