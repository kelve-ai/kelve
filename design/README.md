# kelve's look, outside the app

`kelve.css` is one stylesheet for every page of kelve that isn't the app:

- **kelve.ai** (the `kelve-site` repository) imports it from a kelve
  checkout beside it, the one its deploy already checks out for the demo.
- **The public pages** kelve-graph serves at kelve.ai (`/@sam`, a score's
  own link at `/s/<id>`) compile it in, with `include_str!`.

It holds the canvas's tokens under the app's names (`web/src/styles/app.css`),
light and dark, by the system or `data-theme` on `<html>`; type; buttons,
wells, segmented controls and fields; the squares for better, the same and
worse; the site's header and footer; and how characters move.

The characters' moves are `kelve_core::character::CSS`, copied here as they
are. kelve-core's tests check the copy, so change them there first, then
here.

It loads no fonts. Each page loads Geist, Geist Mono and Pixelify Sans its
own way, and the system's fonts stand in until they come.

Change it with both its users in mind: run kelve-site's build and
kelve-graph's tests after a change.
