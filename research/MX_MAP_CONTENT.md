# MX venue packaging staging checkpoint

`python3 tools/package_mx_maps.py --output private/reports/mx-map-staging`
creates deterministic resource containers preserving every member and original
path in each of the 56 main venue archives. Outputs remain ignored and are not
registered in the production manifest. Source executable hash is checked only
to identify the disc; executable bytes are never packaged.

Interface: `map-packages.json` records stable `mx_fs01` style IDs,
`source_stem`, `/MX/MAPS/FS01.RES` archive keys, SHA256, byte size and member
count. This is staging metadata, not a complete game catalog.

Incomplete: external dependency closure has not been audited, names/groups
have not been decoded from TrackList/atylist, and runtime validation has not
been performed. Do not publish these packages. A follow-up must resolve
external references, add closure tests, and implement serialized atomic
manifest integration. The current script deliberately never mutates a manifest.

Motorcycle regeneration now replaces only its four bike records, preserving
map records and other future MX content.
