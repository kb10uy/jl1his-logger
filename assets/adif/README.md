# ADIF Specification Tables

These CSV files are data files exported from the tables of the ADIF
specification, version 3.1.8 (released 2026-10-12). The server and the client
generate their ADIF tables from them at build time:

| File | Used for |
| --- | --- |
| `enumerations_band.csv` | Bands and their frequency ranges |
| `enumerations_mode.csv`, `enumerations_submode.csv` | Modes, submodes and import-only modes |
| `fields.csv` | The QSO fields the client offers by name and the enumeration each takes (header and import-only fields left out) |
| `enumerations_qsl_sent.csv`, `enumerations_qsl_rcvd.csv`, `enumerations_qsl_via.csv` | The values the server accepts for the QSL fields |
| `enumerations_dxcc_entity_code.csv` | DXCC entity names and deleted flags |
| `enumerations_primary_administrative_subdivision.csv` | `STATE` / `MY_STATE` choices, without deleted and import-only rows |

`server/build.rs` writes Rust tables into `OUT_DIR` and `client/scripts/adif.js`
(`pnpm run adif`) writes `client/src/lib/adif/tables.ts`; neither output is
committed. Both refuse a malformed file and name the file and row. Their
`ADIF Version` is also the `ADIF_VER` the server writes in exports.

## Source

The files come from the ADIF resources archive, `https://adif.org.uk/318/resources`,
under `exports/csv/`, with their names unchanged. Only the files listed above
are kept here.

They are committed byte for byte as exported: UTF-8 with a byte order mark
and LF line endings. Keeping them unchanged lets a new export be copied over
them and compared with `git diff` without normalizing anything first, and both
generators strip the byte order mark. `.editorconfig` marks them `utf-8-bom` so
that an editor keeps the mark.

The ADIF specification and its resources state no license or redistribution
terms; adif.org.uk describes ADIF as an open standard prepared by the ADIF
Developers Group.

## Updating to a New ADIF Version

1. Download `https://adif.org.uk/<version>/resources` (for 3.1.8, `318`), a
   zip archive, and extract it.
2. Copy the files listed above from its `exports/csv/` over the ones here. Add a
   file only when code starts reading it.
3. Rebuild the server (`cargo build` in `server/`) and run `pnpm run adif` in
   `client/`. Every row must carry the same `ADIF Version`.
4. Review `git diff` of the CSVs, then run the server and client tests. Tests
   that pin values of the old version (band edges, mode counts, subdivision
   counts) and the hand-written client lists (`BAND_GROUPS`, `MODE_ENTRIES`, the
   QSL labels) may need updating: a new band needs a representative frequency
   and a label, and a new QSL value a localized label.
5. Update the version and date at the top of this file and the ADIF version in
   `docs/development/decisions.md`.
