+++
type = "task"
mode = "afk"
status = "closed"
blocked_by = []
repos = ["greenspotter"]
+++
# f02 — names.reveal-intent: binding positions f01 left unmarked

From greenspotter#63 ("Known gaps, not in this PR"). Each position below binds a
name, and a weak name there still reaches a reviewer unmarked.

## Question

Which binding positions that greenspotter#63 left unmarked should `names.reveal-intent` mark in Python, Go and Rust?

## Look at

- `rules/candidates/names.reveal-intent/python.yml`: `with open(p) as f:`,
  `except OSError as e:`, and the walrus `if (n := len(xs)) > 3:`.
- `rules/candidates/names.reveal-intent/go.yml`: a variadic parameter
  (`func f(xs ...string)`), a type-switch binding (`switch v := x.(type)`),
  a select receive (`case m := <-ch:`), and a `const` declaration.
- `rules/candidates/names.reveal-intent/rust.yml`: the parameters of a trait
  method signature without a body (`fn get(&self, k: &str) -> V;`).
- `go.yml` again: the parameters of an interface method
  (`Read(p []byte) (n int, err error)`).
- Model each on the positions #63 added, with fixtures in
  `tests/rules/candidates/names.reveal-intent/`. Only `i`, `j`, `k` stay
  unmarked. A `_` or blank identifier declares nothing.

## Answer looks like

- `ast-grep test --skip-snapshot-tests` passes with a valid and an invalid
  fixture for each position above.
- `with open(p) as f:`, `case m := <-ch:` and `fn get(&self, k: &str);`
  each produce a candidate; `for i := 0; i < n; i++` still does not.
- The CHANGELOG Unreleased section names the change.

## Resolution

Every position above is marked, each with a valid and an invalid fixture (`ast-grep test --skip-snapshot-tests`: 71 passed). Python: `as_pattern_target` (a `with` or `except` alias, a tuple alias included) and a `named_expression` name. Go: `variadic_parameter_declaration`, a `type_switch_statement` alias, a `receive_statement` left side declared with `:=` (`case m = <-ch` reuses a name and stays unmarked), `const_spec`, and `method_elem` parameters and named results. Rust: the parameters of a `function_signature_item`. The ticket's `fn get(&self, k: &str);` stays unmarked, because `k` is a loop index the regex exempts; the fixture uses `p`. A pilot scan reports `with open(p) as f:`, `case m := <-ch:` and `fn get(&self, p: &str);`, and not `for i := 0; i < n; i++`.
