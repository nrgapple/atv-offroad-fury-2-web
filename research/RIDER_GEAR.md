# Rider gear and quad paint — 2026-10-15

## Original source

The original Profile Store (`QuadzillaUIPageProfileStore_Helmet`, `_Jersey`,
`_Gear`, `_Glove`, `_Goggle`, `_Boot`, `_ATV`) reads `ui/frontend/uiRider.txt`
from `RIDEINFO.RES` (exact-build `0021db50`). Its seven sections are Helmets
(26), Gloves (23), Boots (15), Shirts (26), Sleeves (26), Pants (26) and
Goggles (11). Each item has a display name, a `_ui` preview texture, store
`Access`, a brand logo and a points price.

In-game rider materials share the item stem without `_ui`:

- Shirts use their rider-mesh slot variant, `<stem>001`.
- Boot previews named `_boot` use the `_boots` material.
- Sleeves and pants pair with the shirt at the same index; their catalog names
  are generic ("pants 10").

All 153 items resolve to materials in `RIDER_MT.RES`. The port has no store
progression, so every item is selectable, consistent with the Garage's existing
access to every quad.

## Port

- `rider_gear::parse` (op 107) reads the catalog. The Garage gains **Rider gear**,
  a page with Helmet, Jersey (shirt and matching sleeves), Pants, Gloves, Boots,
  Goggles and Quad colour steppers. It shows live quad and rider previews in
  desktop, portrait and landscape.
- Choices persist with the Garage selection. Index 0 keeps the rider's original
  material.
- Rides replace the player's rider-mesh slot materials with the chosen gear.
- **Quad colour is an authored enhancement.** The originals' paint variants are
  separate ATVs. Quad colour recolours only saturated pixels of the quad body
  atlases (`qNNs01m1/m2`) to a chosen hue, or lifts them to white or darkens them
  to black, keeping their shading. Frames, engines, shocks and tires stay original.
- Multiplayer protocol 5 carries an 8-byte appearance (seven gear indices and
  the paint) on each lobby participant. Remote riders draw with that gear and
  paint. Changing gear mid-ride uses the existing in-place change-ride reload.
- The content package hosts `RIDEINFO.RES` compacted to `uiRider.txt` (3KB).
  `RIDER_MT.RES` keeps every catalog and CPU-outfit material (3.7MB). Older
  content packages hide nothing; gear rows stay at Original until the catalog
  is published.

## Checks

- Unit tests: catalog parsing and material resolution, appearance
  normalization and stepping, protocol round trip and range rejection.
- `tools/test_rider_gear.mjs`: every row cycles in three layouts, targets fit,
  choices persist, both previews are ready, and rides load the saved
  appearance. With `ATV_TEST_MULTIPLAYER=1` against a relay-backed site, two
  riders each see the other's gear and paint.
