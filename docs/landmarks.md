# Landmark presence report

`lodwright landmarks` answers one question for a list of named landmarks: is it in the generated output, and where.
It exists so that an object cannot vanish from the far view unnoticed (issue 375).

```
lodwright landmarks --request REQUEST.json --landmarks FILE [--output DIR] [--json]
                    [--write-baseline FILE] [--baseline FILE [--allowed-drop X]]
```

The request is the one the output was generated from: its data folder and load order give the references and the far
models; `--output` defaults to its `output_dir`. Nothing is written except the file given to `--write-baseline`.

## The landmark file

One landmark per line, tab separated: the name (unique), the world's editor id, the model under `meshes` (`-` for the
model of a reference's base object), the place, and an optional note. Blank lines and lines starting with `#` are skipped.

- `ref:` and a reference's `FormID` in hex, as the load order numbers it (the high byte is the plugin's position);
- `pos:x,y,z` or `pos:x,y,z,rx,ry,rz,scale`: a position (rotation in radians), for a landmark that is not one reference.

A list that names real references belongs with the data it was made from, not in this repository; the repository's
own `crates/lodwright-cli/tests/fixtures/landmarks-synthetic.tsv` is made for the tests.

## What is checked

The far model of the landmark (`<model>_lod.nif`) is placed like generation places it (rotation, scale, shape
offset). The landmark counts as written when at least half of the vertices that model uses are found, within half a
game unit, in the object blocks of its world. This follows the landmark's own extents, not a fixed radius around its
position, so a large object is not missed because its centre is far from where its parts are written.

For each landmark the report names the block files, the shape in each (and so the texture the shape draws with), the
triangles found and the cell segments that hold them, next to the segment the landmark's cell should be in.

| status | meaning |
| --- | --- |
| `written` | found in the blocks |
| `missing` | generation takes it by its rules and the blocks do not hold it |
| `excluded` | a rule leaves it out on purpose: not marked visible from far, initially disabled and not persistent, or kept disabled at the start by its enable parent |
| `no_far_model` | no far model, or it cannot be read |
| `unresolved` | the reference is not in the world, or has another model than the file says |

## Baselines

`--write-baseline FILE` saves the report (the `--json` document, `contract` `lodwright-landmarks`). `--baseline FILE`
compares the new report with it and exits 1 when a landmark that was written is no longer written, or has lost more than
`--allowed-drop` (default 0.2) of its triangles. Landmarks that came back, were added to the list or left it are
listed and do not fail the comparison.
