Writing shell scripts is hard. But so far, they have served us well. So why give up on them?

As they grew a bit larger, it felt like a bit more structure is needed. Here is how this works.

## Naming and layout

Components prefixed with `__` are considered internal.

* `drgadm` - Is the main entry point.
* `bin/` – Contains all scripts which can be run. These scripts can be run directly.
* `cmd/` - Contains all the commands (as called by `drgadm`). These scripts will be "sourced" (not called) and are not intended to be called directly.
* `lib/` - Contains additional functionality, which can be sourced by the other scripts. A main entry point `mod.sh` can be sources, which includes all other files. Libraries should only define functions or variables, but not execute anything just by sourcing them.

## Output handling for `drgadm`

Deployments can be pretty noisy, and in normal (most) cases the debug output is just confusing. However, when
things go wrong, it may be helpful.

`drgadm` captures all output (stdout and stederr) in a temporary log file, which is deleted at the end of
the script. The output cannot be seen.

If the return code of the script indicates a failure (non-zero), the log file is dumped to `stderr` before
it is deleted.

It also creates a new file descriptor (#3 = `fd3`, `stdin` is #0, `stdout` is #1, `stderr` is #2). Output
that gets written to `fd3`, is always displayed to the user. A function named `progress` (which works just
like `echo`) directly writes to `fd3`.

You can set the environment variable `DEBUG` to any non-empty string to directly see the output on the console.

NOTE: One downside of this is, that inner scripts must not exit with using `exit`, but by "failing" a call (e.g.
running `false` with `set -e` active). This is required because otherwise the exit trap will run with the
output redirection enabled. Using `die` will already take care of this.
