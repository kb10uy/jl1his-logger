# Search Query Syntax

The log list, the native API (`q` on `GET /qsos`, `GET /targets` and the
export filters; see "Search" in [api.md](api.md)) and the web client's chips
all take one text query.

A query is a list of terms separated by spaces, all of which must hold. A term
is `key:value` or a bare word. Keys are read ignoring case. A value with a
comma lists alternatives, any one of which matches; a value with spaces or
commas is quoted with `"`, and the quotes may surround the whole term or any
part of a value (`station:"Mt Takao"`, `station:"a,b",c`). A term beginning
with `-` must not hold.

```text
call:JA1* band:40m,20m -mode:FT8 date:2026-10-01.. station:"Mt Takao" yokohama
```

## Terms

| Term | Holds when |
| --- | --- |
| `call:JA1*` | The call matches, ignoring case, with `*` and `?` as wildcards |
| `base:JA1XYZ` | The call without prefixes and suffixes equals the value's, ignoring case (`base:JA1XYZ/P` is `base:JA1XYZ`) |
| `band:40m,20m`, `mode:SSB`, `contest:ALLJA1` | `BAND`, `MODE` or `CONTEST_ID` is one of the values, ignoring case |
| `sent:R,Q`, `rcvd:Y,V` | `QSL_SENT` or `QSL_RCVD` is one of the values, ignoring case |
| `lotw-sent:Y`, `lotw-rcvd:Y`, `eqsl-sent:Y`, `eqsl-rcvd:Y,V` | `LOTW_QSL_SENT`, `LOTW_QSL_RCVD`, `EQSL_QSL_SENT` or `EQSL_QSL_RCVD` is one of the values, ignoring case |
| `station:Takao` | The Station's name or the QSO's `STATION_CALLSIGN` contains the value, ignoring case |
| `station-id:0192…` | The QSO's Station is one of the IDs |
| `instrument:IC-705` | The Instrument's name or the QSO's `MY_RIG` contains the value, ignoring case |
| `instrument-id:0192…` | The QSO's Instrument is one of the IDs; a QSO without one never holds |
| `park:JP-0012` | One of the references in `MY_POTA_REF` is the value, ignoring case and spaces |
| `dxcc:339` | `DXCC` is one of the entity codes |
| `date:2026-10-18`, `date:2026-10-01..2026-10-19`, `date:>=2026-10-18` | The UTC date of the QSO time is the day, is in the range (both ends included, either may be left open: `date:2026-10-01..`) or compares with `>`, `>=`, `<`, `<=`; one value only |
| `freq:14.074`, `freq:14.0..14.35`, `freq:>=50` | `FREQ` in MHz is the value, is in the range (both ends included, either may be left open) or compares with `>`, `>=`, `<`, `<=`; one value only. A QSO without `FREQ` never holds |
| `pwr:<=5`, `pwr:10..50` | `TX_PWR` in watts, as `freq:` does. A QSO whose `TX_PWR` is missing or not a decimal number never holds |
| `has:freq`, `no:freq` | The ADIF field, by name ignoring case, is present or absent; an empty value counts as absent. The keys `park`, `sent`, `rcvd`, `contest`, `lotw-sent`, `lotw-rcvd`, `eqsl-sent`, `eqsl-rcvd` and `pwr` name their fields, so `no:park` is `no:MY_POTA_REF` |
| `adif.GRIDSQUARE:PM95` | The ADIF field, by name ignoring case, is one of the values, ignoring ASCII case |
| `yokohama`, `"two words"` | The call, `NAME`, `QTH` or `COMMENT` contains the word, ignoring case |
| `-term` | The term does not hold (`-no:freq` is `has:freq`) |

Values are compared as the ADIF strings the QSO holds, so `band:40m` is
`band:40M`, and a field's value is never normalized for the comparison.
`freq:` and `pwr:` are the exceptions: both sides are read as decimal numbers
to six places, exactly, so `freq:14.074` matches `14.0740`. A term lists at
most 100 values.

## Errors

A query that cannot be read is refused as a whole, never partly: the log list
marks the term, and the API answers `400` naming it (see "Search" in
[api.md](api.md) for the shape). The causes are an unknown key (`qrg:7`), a
missing value (`call:`), too many values, a bad date (`date:2026-13-01`), a
bad number (`freq:7MHz`, `pwr:5,10`), a field name that is not an ADIF one
(`adif.1X:1`), a quote that is not closed, and an empty word (`""`). In the
web client an unfinished term such as `call:` is shown as pending and the
query is not sent until it is complete.
