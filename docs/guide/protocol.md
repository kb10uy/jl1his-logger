# Desktop Client Protocol

The desktop client takes QSOs and entry fields from other programs on the
same machine over UDP, on the addresses listed as `[[listeners]]` in its
settings (see "Desktop Client" in [operations.md](operations.md)). By default
these are `127.0.0.1:2237`, where WSJT-X sends, and `127.0.0.1:2333`, both
off until turned on. The only thing sent back is the entry form, to the
programs on the same machine that subscribe to it (see
[Subscription](#subscription)) on a listener where that is turned on.

There is no authentication: anything that reaches a listener's address is
taken as coming from the user. Keep listeners on the loopback address unless
the sending program runs on another machine of a network you trust.

Each datagram is one message, up to 64 KiB, told apart by how it starts:

| Starts with | Read as |
| --- | --- |
| The bytes `ad bc cb da` | A WSJT-X network protocol message |
| `{` (after any whitespace) | The client's own JSON, below |
| Anything else | ADIF text |

So one listener takes all three, and a program that logs through ADIF and one
that fills the form can share a port. A datagram the client cannot read is
dropped without a trace.

## QSOs

A complete QSO is logged at once without going through the form. It is
written to the client's queue for the Station and Instrument chosen on the
entry screen and sent to the server like one typed in; while no Station can be
chosen, it waits for one. Its own values win: the Station and the Instrument
fill only what the record lacks. A `STATION_CALLSIGN` or `MY_*` value that
differs from the Station's is logged as it came and shown as a warning. The
same QSO (`CALL`, `QSO_DATE`, `TIME_ON`, `BAND` and `MODE`, compared ignoring
case) arriving again within two minutes is dropped, so WSJT-X sending to two
ports logs once. A QSO that reached the server is announced with an operating
system notification naming its call, the program, the band and the mode.

### WSJT-X

Of the WSJT-X network protocol, which JTDX and MSHV speak too, only
`Logged ADIF` (type 12) is read: its ADIF text is read as below and its `Id`
names the program in the notification. `QSO Logged` (type 5), sent for the
same QSO, and every other message type are ignored. In WSJT-X, set
Settings › Reporting › UDP Server to `127.0.0.1` and the port of a listener.

### ADIF text

A datagram of ADIF text holds one or more records, with or without a header.
Field lengths are read as codepoints, bytes or UTF-16 units, whichever makes
the record consistent. Field names are read in any case; empty fields are
dropped. The header's `PROGRAMID`, when there is one, names the program.

```text
<call:6>JA1ABC <qso_date:8>20261027 <time_on:6>120015 <band:3>20m <mode:3>SSB <rst_sent:2>59 <rst_rcvd:2>59 <eor>
```

## Entry Messages

The client's own JSON drives the entry form field by field instead of logging
a QSO. Every message carries `"v": 1` and a `type`; a message of another
version, an unknown `type`, or one that does not parse is ignored. Only the
entry screen takes them. While it is not shown, as when the settings screen
is open, the fields of each `set` are held, the last value of each field
(up to 64 fields), and filled in once when the entry screen comes back; a
`clear` drops what is held, and a `log` is lost.

| `type` | Effect |
| --- | --- |
| `set` | Fills the fields given in `fields`, leaving the rest as they are |
| `clear` | Clears the form, as Esc does |
| `log` | Logs the form, as the Log button does |

`fields` is an object of ADIF field names (in any case) to string values; a
value of another JSON type ignores the whole message. An empty string clears
the field. The fields the form has are `CALL`, `RST_SENT`, `RST_RCVD`,
`SRX_STRING` (the received exchange), `NAME`, `QTH`, `COMMENT`, `TX_PWR`,
`NOTES`, the Logbook's area field (such as `CNTY`), and the tuning fields
`FREQ`, `BAND`, `MODE` and `SUBMODE`, which move each other as typing them
does: a `BAND` sets the band's representative frequency, a `FREQ` moves the
band, and a `MODE` is taken with the `SUBMODE` given beside it. Any other
QSO field the form accepts as an extra field is set as one; a name the form
refuses is ignored.

```json
{"v": 1, "type": "set", "fields": {"CALL": "JA1ABC", "SRX_STRING": "13M", "RST_RCVD": "59"}}
```

```json
{"v": 1, "type": "log"}
```

## Subscription

A program that shows or uses the entry form, for templates, macros or its own
display, can subscribe to it on a listener whose Share the form (`subscribe`
in the settings) is on. It is off by default.

| `type` | Effect |
| --- | --- |
| `subscribe` | Registers the sender's address, or renews it, and answers it at once with a `form` |
| `unsubscribe` | Removes the sender's address |

Only a sender on the loopback address (`127.0.0.0/8` or `::1`) can subscribe,
even to a listener on `0.0.0.0`; a `subscribe` or `unsubscribe` from any other
address is ignored. So nothing on the network can make the client send to a
third party, and the form, which may hold another station's name, does not
leave the machine.

A subscriber sends `subscribe` every 5 seconds; the `form` that answers it
also tells it the client is still there. One that has not subscribed for 15
seconds is dropped. At most 32 addresses are subscribed at once; a new one
past that is ignored until another is dropped. Every reply goes out from the
socket of the listener that took the latest `subscribe`.

```json
{"v": 1, "type": "form", "fields": {"CALL": "JA1ABC", "NAME": "", "QTH": "", "RST_RCVD": "595", "RST_SENT": "595", "SRX_STRING": "012", "STX_STRING": "013"}}
```

A `form` always holds all seven fields, an empty one as `""`, never only what
changed, so one that is lost is made good by the next. The values are as the
QSO would be logged: trimmed, and `CALL` and `SRX_STRING` in capitals.
`SRX_STRING` is the received exchange and `STX_STRING` the next one to send,
both without the RST and both empty unless contest mode is on. The tuning
fields are not sent: the client reads them from the rig, as a subscriber can.

Besides answering `subscribe`, a `form` goes to every subscriber, including
one that sent the `set`, when the form settles on new values: when a field is
left, when a `set` or `clear` is filled in, on Esc, after a QSO is logged
(which empties the form and moves the serial number), and when the contest
settings change the exchange to send. It is not sent for every key typed, nor
when the values are the same as the last one sent. Until the entry screen has
shown the form once, and after signing out, there is no form and `subscribe`
is not answered; a `set` held while the entry screen is not shown is not in
the form until it is filled in.

```json
{"v": 1, "type": "bye"}
```

`bye` tells every subscriber that it no longer is one: the client is quitting,
its listeners were changed in the settings, or it signed out. A program may
subscribe again; a client that is still running answers once it has a form.
