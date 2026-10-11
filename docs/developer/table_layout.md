# Table Layout

The table renderer infers layout from the input data and a lightweight
column spec. Callers should pass intent (roles/optionality), not layout knobs.
The renderer is free to override hints when needed to keep output readable.

`retinue_table` owns column policy and width allocation. `retinue_table_text` owns
cell normalization, ANSI-aware wrapping, and final line formatting.

## Column Specification

Each column can include a spec entry with minimal intent:

- `label`: header text (required for matching).
- `role`: semantic intent (`name`, `time`, `type`, `location`, `details`, `id`,
  `stat`, `link`, `flags`, `extra`).
- `optional`: soft hint that the column can be dropped when space is tight.
- `priority`: optional override for drop ordering (higher = keep longer).
- `kind`/`source`: optional markup for composite data (e.g., `time_range` with
  `source => {row_map, [window_start, window_end]}`) so heuristics can use raw values.
  `kind` also influences wrap/priority heuristics (e.g., time ranges are kept
  longer and prefer no wrapping).

Specs are advisory. If the table is unreadable, the renderer may drop or reorder
columns even when they are marked optional/important.

## Automatic Layout

1. Drop empty columns (all cells empty or `null`).
2. Drop uniform columns (all non-empty cells are the same, row count > 1) only
   when the table would otherwise overflow terminal width.
3. Reorder sparse/optional columns to the right (lowest fill ratio last).
4. Drop low-value columns when the table overflows terminal width.
5. Wrap long text columns and reflow to reduce total row height.

## Width And Wrapping
- If content fits within the terminal width, widths stay at natural content
  size; the renderer does not expand to fill the terminal.
- Columns wrap when they contain spaces, path separators (`/`), or exceed half
  the terminal width as a single token.
- Minimum wrap width is the header width; desired width targets the longest
  word in the column when space allows.
- Roles (`details`, `link`, `flags`, `extra`) bias toward wrapping.

## Automatic Splitting
- When a `type` role column exists and multiple types are present, the renderer
  splits the output into one table per type when other columns are sparse.
- The `Type` column is dropped in each sub-table since it is uniform.

## Priority And Dropping
- Column priority combines role intent with data density (fill count/unique
  value count).
- Optional columns drop before non-optional columns when space is tight.

## ANSI Handling
- Wrapped lines preserve ANSI state by carrying the last SGR across lines and
  appending a reset when needed.
