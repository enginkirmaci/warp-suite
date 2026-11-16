# Triage labels

The engineering skills speak in terms of canonical triage roles. This file maps
those roles to the strings this repo uses. The tracker is local markdown, so a
role is a line in the ticket file rather than a label on a service — the name
and the string are the same thing here.

## State roles — recorded as a `Status:` line

| Canonical role    | String here       | Meaning                                  |
| ----------------- | ----------------- | ---------------------------------------- |
| `needs-triage`    | `needs-triage`    | Maintainer needs to evaluate this ticket |
| `needs-info`      | `needs-info`      | Waiting on reporter for more information |
| `ready-for-agent` | `ready-for-agent` | Fully specified, ready for an AFK agent  |
| `ready-for-human` | `ready-for-human` | Requires human implementation            |
| `wontfix`         | `wontfix`         | Will not be actioned                     |

A ticket carries exactly one state role. An untriaged ticket has no `Status:`
line yet; the first triage writes `Status: needs-triage`. When a state role
changes, edit the line in place instead of adding a second one.

## Category roles — recorded as a `Category:` line

| Canonical role | String here   | Meaning                    |
| -------------- | ------------- | -------------------------- |
| `bug`          | `bug`         | Something is broken        |
| `enhancement`  | `enhancement` | New feature or improvement |

## If this repo ever moves to GitHub Issues

The strings are already GitHub's own names — `wontfix`, `bug` and `enhancement`
exist in the repo today. Moving would need only four labels created
(`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`), a
rewritten [`issue-tracker.md`](issue-tracker.md), and this table left as it is.
