# Deliberate differences from 1.x

Every place where 2.0 does not match 1.x on purpose, with the reason.

## Expressions (`stl-engine::params`)

- **Non-finite results are errors.** The result of every function call, `^` and `%` is checked
  with `is_finite()`. A non-finite result gives `"{src} doesn't give a number"`. Reason: this is
  what 1.x does on its ValueError/OverflowError path; in Rust `sqrt(-1)` is NaN instead of an
  exception, so without the check `max(sqrt(-1), 3)` could give a wrong number.
- **`0^-1`** gives `"{src} doesn't give a number"`. Reason: in 1.x this raises ZeroDivisionError,
  which escapes the expression engine as a crash instead of a message.
- **Nesting deeper than 200 levels** gives `"{src} is nested too deeply"`. Reason: 1.x hits
  Python's RecursionError (an uncaught crash); Rust would overflow the stack.
- **Long runs of unary minus/plus signs, long `^` chains and long chains of parameters that refer
  to each other** give their result. 1.x crashes with RecursionError: for runs of signs or `^`
  from about 1000 in a row, and for parameter chains from a few hundred levels when the list has
  them in reverse order. Reason: 2.0 evaluates these without recursion, so there is no limit to hit;
  a crash is never the right answer. Brackets and function calls keep the 200-level limit above.
- **Panics in features on wasm32 abort the worker.** Reason: on native targets a panic is caught
  per feature and shown as that feature's error, but wasm32 has no unwinding here. Handled in
  milestone 4.

## Document (`stl-engine::document`)

- **An unexpected failure inside a feature** shows `"Internal error: {panic message}"` on that
  feature; 1.x shows `"{ExceptionType}: {e}"` (for example `ValueError: math domain error`).
  Reason: Rust has no exception types. A bug in a feature is a panic, which carries only a
  message, and "Internal error" says plainly that it is our fault and not something to fix in
  the model. A feature type the build doesn't know (1.x: `KeyError: 'warp'`) shows
  `"Unknown feature warp."`, the message 1.x already uses when adding one.
- **`preview` with a missing edit id** returns an error result `"That feature no longer exists."`
  (with the current state) instead of raising. Reason: the plan's signature isn't a `Result`;
  not visible to the user.
- **`body_name` for a missing body** returns `""` instead of raising. Reason: same, the
  signature isn't a `Result`; not visible to the user.

## Project files (`stl-engine::stlx`)

- **`"This isn't an STL Editor project."`** when the file isn't a zip, has no `document.json`,
  or `document.json` isn't readable as a project. Reason: 1.x lets `BadZipFile` / `KeyError` /
  `JSONDecodeError` escape as crashes.
- **`"This project was made by a newer version of STL Editor."`** when `format` is above 1.
  Reason: 1.x doesn't check the format number and would misread a newer file.
- **`"The imported file is missing from the project."`** when a source listed in `document.json`
  has no entry in the zip. Reason: 1.x raises `KeyError` while opening; this is the message 1.x
  already shows for a missing source during evaluation.
- **Unknown top-level keys in `document.json` are kept and written back on save.** Reason: 1.x
  drops them; a project from a newer or other version keeps its data this way.
- **`"Couldn't open the file: ..."` / `"Couldn't save the project: ..."`** for file system
  errors. Reason: 1.x lets `OSError` escape; Rust needs a message in its `FileError`.
