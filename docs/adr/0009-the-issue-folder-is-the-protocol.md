# The issue folder is the protocol

_Status: accepted._

The only durable interface between warp-flow's steps is markdown in the tracker
folder: `spec.md`, `issues/<NN>-<slug>.md`, `workflow.toml`, `run.md`,
`review.md` and `repairs/<NN>.md` under `.scratch/<feature-slug>/`. A step reads
those files, writes them back, and the engine advances by parsing them — the
`Status:` line, `Blocked by:`, the task checkboxes, and the front-matter on
`run.md`. No database, and no state that exists only inside the process. The
deciding factor is that the repo already keeps its issues and specs there
([`docs/agents/issue-tracker.md`](../agents/issue-tracker.md)), so a human can
read and correct any step's result in the editor they already use, and a run
killed mid-way is resumable from the files alone.

Considered options: a sqlite or JSON run store beside the repo (queryable, but a
second truth that reviewers never see and that drifts as soon as someone edits
the markdown); in-memory state only (simplest, but a run cannot pause for days
or survive a crash); GitHub issues (off-repo, and the tracker rule keeps them
empty).

Consequences: the parser and its schema check are load-bearing, so a malformed
artifact is a first-class failure the engine repairs rather than ignores; step
boundaries are the only write points, so parallel tasks never contend for
`run.md`; these files reach public history unless `.scratch/` is gitignored.
