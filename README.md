# addrlint

A linter for plain-text postal address blocks. Point it at a file where
addresses are separated by blank lines and it reports problems with line
numbers, the same way a compiler warning does.

## Why

Address lists get typed by hand, pasted out of spreadsheets, or scraped from
web forms, and the result is usually inconsistent: a city with no state, a
zip that got mangled by Excel dropping a leading zero, a lowercase state
code, a line so long it won't fit on a label. Mail gets kicked back and
nobody knows why until someone reads every line by eye. addrlint runs the
obvious checks up front.

It expects US-style blocks: a name/company line, one or more street lines,
and a final city/state/zip line, with a blank line between addresses.

```
Maria Alvarez
482 Cedarwood Lane
Springfield IL 62704

James Whitfield
19 Birch St
Portland OR 97201
```

## Usage

```
addrlint [<file>] [--json] [--rules=<spec>]
```

Omit `<file>` or pass `-` to read the address block from stdin instead of a
file, so it can sit in a pipeline:

```
$ cat examples/addresses.txt | addrlint
```

Given `examples/addresses.txt`:

```
Maria Alvarez
482 Cedarwood Lane Apartment 12B, Building C, Second Floor
Springfield IL 62704

James Whitfield
19 Birch St

Tomoko Sato
77 Harbor View Road
Portland or 97201

Deborah Klein
5 Maple Ct   
Austin TX 78701
```

Human-readable output:

```
$ addrlint examples/addresses.txt
examples/addresses.txt:2: warning: line is 58 characters, over the 40-character limit for a printable address line [line-too-long]
examples/addresses.txt:6: error: last line of address block has no recognizable state or territory code [missing-state]
examples/addresses.txt:6: error: last line of address block has no recognizable ZIP code (5 digits, optionally followed by -4) [missing-zip]
examples/addresses.txt:10: warning: state code 'or' should be uppercase [state-not-uppercase]
examples/addresses.txt:13: warning: line has trailing whitespace [trailing-whitespace]
2 error(s), 3 warning(s)
```

`--json` prints the same findings as a single JSON array on stdout, one
object per finding, so the output can be piped into another tool or a CI
step without screen-scraping:

```
$ addrlint examples/addresses.txt --json
[{"file":"examples/addresses.txt","line":2,"rule":"line-too-long","severity":"warning","message":"line is 58 characters, over the 40-character limit for a printable address line"},{"file":"examples/addresses.txt","line":6,"rule":"missing-state","severity":"error","message":"last line of address block has no recognizable state or territory code"}, ...]
```

The exit code is nonzero if any finding is an `error`, so it can gate a
build without parsing output at all.

## Selecting rules

By default every check runs. `--rules` takes a comma-separated list to
narrow that down:

```
$ addrlint --rules=-trailing-whitespace,-line-too-long examples/addresses.txt
```

runs everything except the two named rules, while

```
$ addrlint --rules=missing-state,missing-zip examples/addresses.txt
```

runs only those two. The two forms can be mixed; a bare name switches to an
allow-list, and a `-`-prefixed name removes from whatever set is currently
active. An unrecognized rule name is a usage error, not a silent no-op.

## Current checks

- `line-too-long` — line exceeds 40 characters
- `trailing-whitespace` — line has trailing spaces or tabs
- `incomplete-block` — a block has fewer than two lines
- `missing-state` — last line of a block has no recognizable state/territory code
- `state-not-uppercase` — the state code is valid but not uppercase
- `missing-zip` — last line of a block has no recognizable ZIP or ZIP+4

## Building

Standard `cargo build` / `cargo run -- <file>`. No dependencies.

## License

MIT, see [LICENSE](LICENSE).
