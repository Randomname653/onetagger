#!/usr/bin/env bash
# Keeps this fork in sync with Marekkon5/onetagger:
#  1. updates the upstream status line in README.md (latest release + last commit)
#  2. merges new upstream commits; on conflicts nothing is pushed to master,
#     a pull request from the `upstream-master` branch is opened instead.
# Requires: git, gh (GH_TOKEN set), run inside a clone of the fork on master.
set -euo pipefail

UPSTREAM="${UPSTREAM:-Marekkon5/onetagger}"
BRANCH="${BRANCH:-master}"
DRY_RUN="${DRY_RUN:-0}"

git remote get-url upstream >/dev/null 2>&1 || git remote add upstream "https://github.com/$UPSTREAM.git"
git fetch --quiet upstream "$BRANCH"

# --- 1. Upstream status line ---------------------------------------------------
release=$(gh api "repos/$UPSTREAM/releases/latest" --jq '.tag_name + " (" + (.published_at | split("T")[0]) + ")"' 2>/dev/null || echo "unknown")
last_commit=$(git log -1 --format=%cs "upstream/$BRANCH")
line="Last upstream release: $release, last upstream commit: $last_commit."
sed -i -E "s|(<!-- upstream-status -->).*(<!-- /upstream-status -->)|\1$line\2|" README.md
if ! git diff --quiet -- README.md; then
    git commit --quiet -m "Update upstream status: $line" -- README.md
    echo "README: $line"
else
    echo "README: up to date"
fi

# --- 2. Upstream commits -------------------------------------------------------
new_commits=$(git rev-list --count "HEAD..upstream/$BRANCH")
echo "New upstream commits: $new_commits"
pr_needed=0
if [ "$new_commits" -gt 0 ]; then
    git log --oneline "HEAD..upstream/$BRANCH"
    # The Actions token may not push workflow changes: those need a manual merge
    if [ -n "$(git diff --name-only "HEAD...upstream/$BRANCH" -- .github/workflows)" ]; then
        echo "::error::Upstream changed .github/workflows - merge manually: git fetch upstream && git merge upstream/$BRANCH"
        [ "$DRY_RUN" = "1" ] || git push --quiet origin "HEAD:$BRANCH"
        exit 1
    fi
    if git merge --no-edit --quiet "upstream/$BRANCH" -m "Merge upstream $UPSTREAM ($new_commits commits)"; then
        echo "Merged cleanly"
    else
        git merge --abort
        pr_needed=1
        echo "Merge conflicts, opening a pull request instead"
    fi
fi

if [ "$DRY_RUN" = "1" ]; then
    echo "Dry run: not pushing"
    exit 0
fi

git push --quiet origin "HEAD:$BRANCH"

if [ "$pr_needed" = "1" ]; then
    git push --quiet --force origin "upstream/$BRANCH:refs/heads/upstream-$BRANCH"
    existing=$(gh pr list --head "upstream-$BRANCH" --state open --json number --jq 'length')
    if [ "$existing" = "0" ]; then
        gh pr create --base "$BRANCH" --head "upstream-$BRANCH" \
            --title "Merge upstream changes ($new_commits commits, conflicts)" \
            --body "Upstream $UPSTREAM has $new_commits new commits that conflict with changes in this fork and could not be merged automatically. Resolve the conflicts in this pull request."
    fi
fi
