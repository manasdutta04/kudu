# Branch rulesets

`protect-main.json` protects the default branch (`main`):

- No direct pushes: every change lands through a pull request.
- The **macOS app**, **Windows app** and **Linux app** CI checks must pass before merging.
- Review conversations must be resolved before merging.
- No force-pushes and no deleting the branch.
- Repository admins can merge a pull request without waiting on the rules, but can't push to
  `main` directly either.

Approvals are set to 0 so a solo maintainer can still merge their own pull requests. Raise
`required_approving_review_count` once there are more maintainers.

## Applying it

GitHub doesn't read this file automatically. To turn it on:

1. Open **Settings → Rules → Rulesets** on the repository.
2. Click **New ruleset → Import a ruleset** and pick `protect-main.json`.
3. Review it and click **Create**.

Or with the GitHub CLI:

```bash
gh api -X POST repos/manasdutta04/kudu/rulesets --input .github/rulesets/protect-main.json
```
