# Issue tracker: GitHub

Issues and PRDs for this repository live as GitHub issues in
`jkeywo/last-aeon`. Use the `gh` CLI for issue operations.

## Conventions

- Create an issue with `gh issue create --repo jkeywo/last-aeon`, supplying a
  title, body, and the appropriate labels.
- Read an issue with `gh issue view <number> --repo jkeywo/last-aeon
  --comments` and request structured JSON when the caller needs to inspect
  labels or comments.
- List issues with `gh issue list --repo jkeywo/last-aeon`, using explicit
  state and label filters.
- Comment with `gh issue comment`, edit labels or milestone with
  `gh issue edit`, and close with `gh issue close`.
- Prefer `--body-file` for substantial issue bodies so Markdown is preserved
  exactly.

## Skill routing

When a skill says to publish to the issue tracker, create a GitHub issue in
`jkeywo/last-aeon`. When it says to fetch a relevant ticket, read that GitHub
issue and its comments.
